//! The engine's crossfades and loops: overlapping the next track with the
//! current one's end, fading a track's edges, and wrapping or halting at a
//! loop's bounds.

use super::*;

impl Engine {
    /// Opens the next track to overlap the current one's last frames, once
    /// the chunk at `from` reaches them. Tried once per track: a track that
    /// cannot be crossfaded joins gaplessly, as with no crossfade.
    pub(super) fn begin_crossfade(&mut self, from: u64, frames: u64) {
        if self.crossfade.is_zero() || self.pause_at.is_some() {
            return;
        }
        let (Some(conv), Some(stream), Some(plan)) = (
            self.conv.as_ref(),
            self.stream.as_ref(),
            self.out.as_ref().map(|o| o.plan),
        ) else {
            return;
        };
        let spec = conv.src();
        let at_rate = |d: Duration| (d.as_secs_f64() * f64::from(spec.rate)) as u64;
        let Some(len) = stream.duration().map(at_rate) else {
            return;
        };
        let wanted = at_rate(self.crossfade).min(len / 2);
        if from + frames <= len.saturating_sub(wanted) {
            return;
        }
        let current = self.marks.back().map_or(self.index, |m| m.1);
        if self.crossfade_tried.replace(current) == Some(current) {
            return;
        }
        let Some(next) = self.following(current, false) else {
            return;
        };
        // Opened quietly: a file that fails is reported when its turn comes.
        let Some(path) = self.queue.get(next) else {
            return;
        };
        let Ok(mut next_stream) = AudioStream::open(path) else {
            return;
        };
        let Ok(Some(first)) = next_stream.next_chunk().map(|c| c.map(<[f32]>::to_vec)) else {
            return;
        };
        let speed = self.speed();
        let joins = next_stream.spec() == spec
            && self.plan_for(spec).ok() == Some(plan)
            && self
                .conv
                .as_ref()
                .is_some_and(|c| c.continues(spec, plan, speed));
        // Otherwise it is converted to the stream's plan, and resampled to its
        // rate for the rest of the track, over the gap a new stream leaves.
        let (mut buf, mut conv, mut decoded) = (first, None, 0);
        if !joins {
            let next_spec = next_stream.spec();
            let mut own = Converter::new(next_spec, plan, speed);
            let mut out = Vec::new();
            own.push(&buf, &mut out);
            decoded = (buf.len() / next_spec.channels.max(1) as usize) as u64;
            (buf, conv) = (out, Some(own));
        }
        let duration = next_stream.duration();
        let n = duration.map_or(wanted, |d| wanted.min(at_rate(d) / 2));
        self.incoming = Some(Incoming {
            stream: next_stream,
            index: next,
            duration,
            buf,
            conv,
            decoded,
            done: false,
            start: len - n,
            frames: n.max(1),
            marked: false,
        });
    }

    /// Carries `chunk`, at frame `from` of the outgoing track, mixed with the
    /// incoming track past the overlap's start. Each side takes its own gain
    /// first, then an equal-power curve, so the level holds across the join
    /// for uncorrelated music.
    pub(super) fn carry_crossfaded(&mut self, chunk: &[f32], from: u64) {
        let channels = self
            .conv
            .as_ref()
            .map_or(1, |c| c.src().channels.max(1) as usize);
        let frames = chunk.len() / channels;
        let Some(start) = self.incoming.as_ref().map(|i| i.start) else {
            return;
        };
        let before = (start.saturating_sub(from) as usize).min(frames);
        if before > 0 {
            self.carry_faded(&chunk[..before * channels], from);
        }
        if before == frames {
            return;
        }
        let outgoing = self.marks.back().map_or(self.index, |m| m.1);
        self.mark_incoming();
        let Some(mut inc) = self.incoming.take() else {
            return;
        };
        let mixed_frames = frames - before;
        if inc.conv.is_some() {
            let from = from + before as u64;
            self.mix_converted(&mut inc, &chunk[before * channels..], from, outgoing);
            self.incoming = Some(inc);
            return;
        }
        while inc.buf.len() < mixed_frames * channels && !inc.done {
            match inc.stream.next_chunk() {
                Ok(Some(c)) => inc.buf.extend_from_slice(c),
                _ => inc.done = true,
            }
        }
        let (g_out, g_in) = (self.gain_at(outgoing), self.gain_at(inc.index));
        let mut mixed = Vec::with_capacity(mixed_frames * channels);
        for k in 0..mixed_frames {
            let into = (from + (before + k) as u64).saturating_sub(inc.start);
            let t = ((into as f32 + 0.5) / inc.frames as f32).min(1.0);
            let angle = t * std::f32::consts::FRAC_PI_2;
            let (a, b) = (angle.cos() * g_out, angle.sin() * g_in);
            for c in 0..channels {
                let out = chunk[(before + k) * channels + c];
                let incoming = inc.buf.get(k * channels + c).copied().unwrap_or(0.0);
                mixed.push(out * a + incoming * b);
            }
        }
        let used = inc.buf.len().min(mixed_frames * channels);
        inc.buf.drain(..used);
        inc.decoded += (used / channels) as u64;
        self.incoming = Some(inc);
        // The gains are in already.
        if let Some(conv) = self.conv.as_mut() {
            conv.push(&mixed, &mut self.carry);
        }
    }

    /// [`Engine::carry_crossfaded`] for an incoming track with its own
    /// converter: `chunk`, from frame `from` of the outgoing track, is
    /// converted first, and the curve runs over output frames.
    pub(super) fn mix_converted(
        &mut self,
        inc: &mut Incoming,
        chunk: &[f32],
        from: u64,
        outgoing: usize,
    ) {
        let (Some(conv), Some(own)) = (self.conv.as_mut(), inc.conv.as_mut()) else {
            return;
        };
        let src_channels = conv.src().channels.max(1) as usize;
        let channels = self
            .out
            .as_ref()
            .map_or(1, |o| o.plan.channels.max(1) as usize);
        let in_channels = inc.stream.spec().channels.max(1) as usize;
        let mut out = Vec::new();
        conv.push(chunk, &mut out);
        let n = out.len() / channels;
        while inc.buf.len() < n * channels && !inc.done {
            match inc.stream.next_chunk() {
                Ok(Some(c)) => {
                    inc.decoded += (c.len() / in_channels) as u64;
                    own.push(c, &mut inc.buf);
                }
                _ => {
                    inc.done = true;
                    own.finish(&mut inc.buf);
                }
            }
        }
        let (g_out, g_in) = (self.gain_at(outgoing), self.gain_at(inc.index));
        // The outgoing source frame each output frame came from, near enough.
        let per = (chunk.len() / src_channels) as f32 / n.max(1) as f32;
        for k in 0..n {
            let at = from as f32 + k as f32 * per;
            let into = (at - inc.start as f32).max(0.0);
            let t = ((into + 0.5) / inc.frames as f32).min(1.0);
            let angle = t * std::f32::consts::FRAC_PI_2;
            let (a, b) = (angle.cos() * g_out, angle.sin() * g_in);
            for c in 0..channels {
                let incoming = inc.buf.get(k * channels + c).copied().unwrap_or(0.0);
                out[k * channels + c] = out[k * channels + c] * a + incoming * b;
            }
        }
        inc.buf.drain(..inc.buf.len().min(n * channels));
        self.carry.extend_from_slice(&out);
    }

    /// Places the mark where the incoming track starts to be heard: after
    /// what is converted so far, as a gapless join places it.
    pub(super) fn mark_incoming(&mut self) {
        let channels = self
            .out
            .as_ref()
            .map_or(1, |o| o.plan.channels.max(1) as usize);
        let at = self.written + (self.carry.len() / channels) as u64;
        let Some(inc) = self.incoming.as_mut().filter(|i| !i.marked) else {
            return;
        };
        inc.marked = true;
        let (index, duration) = (inc.index, inc.duration);
        self.push_mark(at, index, duration, 0);
    }

    /// Makes the incoming track the one decoded, once the outgoing one ends.
    /// What it decoded past the mix is carried at its own gain.
    pub(super) fn promote_incoming(&mut self) {
        self.mark_incoming();
        let Some(inc) = self.incoming.take() else {
            return;
        };
        self.stream = Some(inc.stream);
        self.crossfaded_in = true;
        self.decoded = inc.decoded;
        if let Some(own) = inc.conv {
            // The outgoing converter's tail is dropped, not finished: the
            // curve was applied after it, so its last frames would be heard
            // at full level. It holds at most the resampler's delay.
            self.conv = Some(own);
            let gain = self.gain_at(inc.index);
            self.carry.extend(inc.buf.iter().map(|x| x * gain));
            return;
        }
        if !inc.buf.is_empty() {
            let channels = self
                .conv
                .as_ref()
                .map_or(1, |c| c.src().channels.max(1) as usize);
            self.decoded += (inc.buf.len() / channels) as u64;
            self.convert_and_carry(&inc.buf);
        }
    }

    /// Forgets a crossfade under way, as a seek or a stop does.
    pub(super) fn drop_incoming(&mut self) {
        self.incoming = None;
        self.crossfade_tried = None;
    }

    /// Converts `chunk`, whose first frame is frame `from` of the track, as
    /// [`Engine::convert_and_carry`] does, faded where it lies within the
    /// fade of the track's start or end. Not on a loop or a one-shot, which
    /// have their own edges.
    pub(super) fn carry_faded(&mut self, chunk: &[f32], from: u64) {
        let (Some(conv), Some(stream)) = (self.conv.as_ref(), self.stream.as_ref()) else {
            return self.convert_and_carry(chunk);
        };
        let (rate, channels) = (conv.src().rate, conv.src().channels.max(1) as usize);
        let n = (self.fade.as_secs_f64() * f64::from(rate)) as u64;
        // An unknown length fades in only.
        let len = stream
            .duration()
            .map_or(u64::MAX, |d| (d.as_secs_f64() * f64::from(rate)) as u64);
        let frames = (chunk.len() / channels) as u64;
        // A crossfade replaces both fades at a join: the incoming track is
        // faded in by it, and a fade out would dip before it starts.
        let fade_in = if self.crossfaded_in { 0 } else { n };
        let fade_out = if self.crossfade.is_zero() { n } else { 0 };
        let inside = from >= fade_in && from + frames <= len.saturating_sub(fade_out);
        if n == 0 || inside || self.loop_here().is_some() {
            return self.convert_and_carry(chunk);
        }
        let faded: Vec<f32> = chunk
            .chunks_exact(channels)
            .zip(from..)
            .flat_map(|(frame, f)| {
                // Past a length estimated short, the frames play unfaded
                // rather than silent.
                let gain = match f < len {
                    true => crate::samples::fade_gain(f, len, (fade_in, fade_out)),
                    false => 1.0,
                };
                frame.iter().map(move |&s| s * gain)
            })
            .collect();
        self.convert_and_carry(&faded);
    }

    /// The loop, when it is on the track being decoded.
    pub(super) fn loop_here(&self) -> Option<Loop> {
        let decoding = self.marks.back().map_or(self.index, |m| m.1);
        self.looping
            .filter(|l| l.index == decoding && self.stream.is_some())
    }

    /// Returns the decoder to `l`'s start, and marks the output frame where
    /// the device reaches it, so the position jumps back as it is heard.
    pub(super) fn wrap(&mut self, l: Loop) {
        let (Some(stream), Some(conv), Some(out)) =
            (self.stream.as_mut(), self.conv.as_ref(), self.out.as_ref())
        else {
            return;
        };
        let rate = conv.src().rate.max(1);
        let at = Duration::from_secs_f64(l.start as f64 / rate as f64);
        if stream.seek(at).is_err() {
            self.looping = None;
            return;
        }
        self.decoded = l.start;
        let duration = stream.duration();
        let heard = self.written + (self.carry.len() / out.plan.channels.max(1) as usize) as u64;
        let output_rate = self.shared.position_rate.load(Ordering::Relaxed) as f64;
        let offset = (at.as_secs_f64() * output_rate) as u64;
        self.push_mark(heard, l.index, duration, offset);
    }

    /// Ends a one-shot range: leaves the decoder at `l`'s end and marks the
    /// output frame the device reaches it at, so nothing past the range is
    /// fed and the pause lands on the last frame of it.
    ///
    /// As [`Engine::wrap`], but forward to the end rather than back to the
    /// start, so playing on afterwards continues the track from there.
    pub(super) fn halt(&mut self, l: Loop) {
        self.looping = None;
        let (Some(stream), Some(conv), Some(out)) =
            (self.stream.as_mut(), self.conv.as_ref(), self.out.as_ref())
        else {
            return;
        };
        let rate = conv.src().rate.max(1);
        let at = Duration::from_secs_f64(l.end as f64 / rate as f64);
        let heard = self.written + (self.carry.len() / out.plan.channels.max(1) as usize) as u64;
        self.pause_at = Some(heard);
        if stream.seek(at).is_err() {
            return;
        }
        self.decoded = l.end;
        let duration = stream.duration();
        let output_rate = self.shared.position_rate.load(Ordering::Relaxed) as f64;
        let offset = (at.as_secs_f64() * output_rate) as u64;
        self.push_mark(heard, l.index, duration, offset);
    }

    /// Loops `bounds` of the audible track, or stops looping.
    ///
    /// The decoder runs ahead of the device, so it may already have passed a
    /// new end, or returned under the old bounds. Seeking to the position then
    /// discards that audio, at the cost of a short gap.
    pub(super) fn set_loop(&mut self, bounds: Option<(u64, u64)>, once: bool, fades: (u64, u64)) {
        // Stopped, the track is closed; cue it, as a seek does.
        if bounds.is_some() && self.state == State::Stopped && !self.queue.is_empty() {
            self.load(self.index, false, false);
        }
        // A range set again replaces any pause the last one scheduled.
        self.pause_at = None;
        // A loop plays on, so the track it loops is not about to end.
        if bounds.is_some() {
            self.drop_incoming();
        }
        let (Some(&(_, audible, duration, _)), Some(conv)) = (self.marks.front(), &self.conv)
        else {
            self.looping = None;
            return;
        };
        let rate = conv.src().rate.max(1);
        let ahead = self.marks.len() > 1 || self.stream.is_none() || self.staged.is_some();
        let at = self.elapsed();
        let Some((start, end)) = bounds else {
            if self.looping.take().is_some() && ahead {
                self.seek(at);
            }
            return;
        };
        let frame = |d: Duration| (d.as_secs_f64() * rate as f64).round() as u64;
        // A one-shot stops a frame short of the track's end: it pauses by
        // seeking the decoder to its end, which refuses the end itself.
        let last = duration.map_or(u64::MAX, |d| frame(d).saturating_sub(u64::from(once)));
        let end = end.min(last);
        if start >= end {
            self.looping = None;
            return;
        }
        self.looping = Some(Loop {
            index: audible,
            start,
            end,
            once,
            fades,
        });
        let now = frame(at);
        // A one-shot is heard from its start each time it is sent.
        if once || now < start || now >= end {
            self.seek(Duration::from_secs_f64(start as f64 / rate as f64));
        } else if ahead || self.decoded >= end {
            self.seek(at);
        }
    }
}
