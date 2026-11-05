//! Doing `:tape` actions for a frontend: loading, taking over, recording, bouncing, and taking in what the deck finished.

use super::*;

/// Takes in what the deck finished since the last frame: loads, saves,
/// recordings and device errors. Files written are added to the library.
pub fn poll(f: &mut impl Frontend) {
    for found in f.tape().poll() {
        report(f, found);
    }
    let long = f.tape().live.as_ref().and_then(|l| l.long.as_ref());
    if long.is_some_and(|(_, at)| at.elapsed() >= LONG_BOUNCE) {
        end_long(f);
    }
    take_over(f);
    let volume = f.mix().gain(crate::mix::Strip::Tape);
    f.tape().follow_volume(volume);
    let interp = f.mix().interp();
    f.tape().follow_interp(interp);
}

fn report(f: &mut impl Frontend, found: Found) {
    match found {
        Found::Note(m) => f.notify(m.into()),
        Found::Wrote(m, path) => match f.session_mut().add_to_library(&path) {
            Ok(()) => f.notify(m.into()),
            Err(e) => f.notify(TapeMessage::Failed(e).into()),
        },
        Found::WroteLoop(m, path, beats) => match f.session_mut().add_loop_file(&path, beats) {
            Ok(()) => f.notify(m.into()),
            Err(e) => f.notify(TapeMessage::Failed(e).into()),
        },
    }
}

/// Folds lane `lane`, from 1, if open, or opens it; `None` is the FX lane.
fn fold(deck: &mut Deck, lane: Option<u8>) {
    match lane {
        None => deck.layout.fx ^= true,
        Some(n) => {
            let i = usize::from(n) - 1;
            let folded = deck.layout.is_folded(i, deck.voice_name(i).is_none());
            deck.layout.folded[i] = Some(!folded);
        }
    }
}

/// Does `action`.
pub fn act(f: &mut impl Frontend, action: TapeAction) {
    match action {
        TapeAction::Load(slot) => return load(f, slot),
        TapeAction::Take => return take(f),
        TapeAction::Record => return record(f),
        TapeAction::Bounce(Some(Bounce::Long)) => return bounce_long(f),
        TapeAction::Bounce(None) if f.tape().live.as_ref().is_some_and(|l| l.long.is_some()) => {
            return end_long(f);
        }
        TapeAction::Loop(n, id) => return load_loop(f, id, Some(n)),
        TapeAction::Fold(lane) => return fold(f.tape(), lane),
        TapeAction::Drawer(n) => {
            let i = usize::from(n) - 1;
            f.tape().layout.drawer[i] ^= true;
            return;
        }
        _ => {}
    }
    let samples = f.session().samples_dir().to_path_buf();
    let playing = f.session().player().status().state == State::Playing;
    let sent = match f.tape().live.as_mut() {
        Some(live) => send(live, action, samples),
        None => Err(TapeMessage::NoTape),
    };
    if let Err(m) = sent {
        return f.notify(m.into());
    }
    if f.tape()
        .live
        .as_ref()
        .is_some_and(|l| l.extent.frames == 0 && l.state.empty.iter().all(|e| *e))
    {
        return eject(f);
    }
    // The tape plays alone; the player stays paused until asked.
    if action == TapeAction::Play && playing {
        f.session().send(playr_core::audio::Cmd::TogglePause);
    }
    if action != TapeAction::Save {
        f.notify(TapeMessage::Done(action).into());
    }
}

/// Drops the tape once every voice is empty, finishing its recording first.
fn eject(f: &mut impl Frontend) {
    let deck = f.tape();
    let Some(mut live) = deck.live.take() else {
        return;
    };
    deck.taking = None;
    if let Some(found) = stop_recording(&mut live) {
        report(f, found);
    }
    f.notify(TapeMessage::Emptied.into());
}

/// Sends `action` to the looper.
fn send(live: &mut Live, action: TapeAction, samples: PathBuf) -> Result<(), TapeMessage> {
    send_to(live, action, samples)?;
    // Writing changes the write loop from its source.
    if action == TapeAction::Write(true) {
        live.printed = true;
    }
    Ok(())
}

fn send_to(live: &mut Live, action: TapeAction, samples: PathBuf) -> Result<(), TapeMessage> {
    let failed = |e: playr_looper::Error| TapeMessage::Failed(e.to_string());
    match action {
        TapeAction::Reset => {
            live.handle.reset().map_err(failed)?;
            live.printed = false;
            Ok(())
        }
        TapeAction::Clear(n) | TapeAction::Eject(n) if live.state.empty[usize::from(n) - 1] => {
            Err(TapeMessage::Empty(n))
        }
        TapeAction::Save if live.extent.frames == 0 => Err(TapeMessage::Empty(4)),
        // Only the write loop is cleared: a voice's own loop is a library
        // loop or a range, which a clear would only lose.
        TapeAction::Clear(n) if live.own[usize::from(n) - 1].is_some() => {
            Err(TapeMessage::OwnLoop(n))
        }
        TapeAction::Clear(_) => {
            live.handle.clear(playr_looper::TAPE).map_err(failed)?;
            live.printed = true;
            Ok(())
        }
        // Voice 4's loop is the write loop.
        TapeAction::Eject(n) if usize::from(n) - 1 == playr_looper::WRITER => {
            live.eject(playr_looper::TAPE).map_err(failed)
        }
        TapeAction::Eject(n) => live.eject(usize::from(n) - 1).map_err(failed),
        TapeAction::Share(n) => live.share(usize::from(n) - 1).map_err(failed),
        TapeAction::Bounce(Some(Bounce::Pass)) => {
            if live.extent.frames == 0 {
                live.blank_tape()?;
            }
            live.handle.set(Setting::Bounce(true)).map_err(failed)?;
            live.bounce = Some(false);
            live.printed = true;
            Ok(())
        }
        TapeAction::Bounce(_) => live.handle.set(Setting::Bounce(false)).map_err(failed),
        // The first Write makes the write loop.
        TapeAction::Write(true) if live.extent.frames == 0 => {
            live.blank_tape()?;
            let s = Setting::Write(true);
            live.handle.set(s).map_err(failed)?;
            live.state.apply(s);
            Ok(())
        }
        TapeAction::Save if live.save_to.is_some() => Err(TapeMessage::AlreadySaving),
        TapeAction::Save => {
            live.handle.snapshot().map_err(failed)?;
            live.save_to = Some(samples);
            Ok(())
        }
        _ => {
            match action {
                TapeAction::Fx(FxSetting::Sync(d)) => live.state.fx.sync = d,
                TapeAction::Fx(FxSetting::Time(_)) => live.state.fx.sync = None,
                _ => {}
            }
            settings(action, extent_for(live, action))?
                .into_iter()
                .try_for_each(|s| {
                    live.handle.set(s)?;
                    live.state.apply(s);
                    Ok(())
                })
                .map_err(failed)?;
            live.resync();
            Ok(())
        }
    }
}

/// The shape a window in `action` counts in: its voice's loop's.
fn extent_for(live: &Live, action: TapeAction) -> Extent {
    match action {
        TapeAction::Voice(n, _) => live.extent_of(usize::from(n).saturating_sub(1)),
        _ => live.extent,
    }
}

fn load(f: &mut impl Frontend, slot: Option<u8>) {
    if let Some((path, rate, span)) = range(f, slot) {
        drop_take(f);
        let mut src = track_source(path, rate, span);
        // Into voice 1 alone, so the other lanes keep their loops.
        src.voice = Some(0);
        start_load(f, src);
    }
}

/// A span of a track, with the track either side as its rolls.
fn track_source(path: PathBuf, rate: u32, span: (u64, u64)) -> Source {
    Source {
        name: samples::name_for(&path),
        path,
        rate,
        span,
        circular: false,
        voice: None,
        beats: None,
    }
}

/// Loads loop `id` of the loop library into voice `n`, from 1, or voice 1.
pub fn load_loop(f: &mut impl Frontend, id: i64, voice: Option<u8>) {
    let Some(l) = f.session().library_loop(id) else {
        return f.notify(TapeMessage::NoSuchLoop.into());
    };
    drop_take(f);
    let path = PathBuf::from(&l.path);
    start_load(
        f,
        Source {
            name: samples::name_for(&path),
            path,
            rate: l.rate,
            span: (l.start, l.end),
            // A slot is a span of a track; anything else was cut to loop.
            circular: l.origin != playr_core::db::loops::Origin::Slot,
            voice: Some(voice.map_or(0, |n| usize::from(n).saturating_sub(1))),
            beats: l.beats,
        },
    );
}

/// Puts back the tape `saved` kept, in place of any tape there is: each
/// voice's loop read again from its file, then its settings. A file that is
/// gone leaves its voice empty, and is named when the resume is done.
pub fn resume(f: &mut impl Frontend, saved: SavedTape) {
    drop_take(f);
    if let Some(found) = f.tape().live.as_mut().and_then(stop_recording) {
        report(f, found);
    }
    let deck = f.tape();
    deck.live = None;
    deck.loading = None;
    deck.layout = saved.layout;
    let mut queue = Vec::new();
    let mut lost = Vec::new();
    for (i, r) in saved.reads.iter().enumerate() {
        if let Reads::File(src) = r {
            match src.path.is_file() {
                true => queue.push(Source {
                    voice: Some(i),
                    ..src.clone()
                }),
                false => lost.push(src.path.clone()),
            }
        }
    }
    queue.reverse();
    deck.restoring = Some(Restoring { saved, queue, lost });
    let found = deck.restore_next();
    match found.is_empty() {
        true => f.notify(TapeMessage::Loading.into()),
        false => found.into_iter().for_each(|x| report(f, x)),
    }
}

/// Loads the range as `load` does, to take over from the player once in.
fn take(f: &mut impl Frontend) {
    if let Some((path, rate, span)) = range(f, None) {
        drop_take(f);
        f.tape().taking = Some(Taking {
            path: path.clone(),
            rate,
            span,
            loaded: false,
            waiting: false,
            armed: false,
        });
        start_load(f, track_source(path, rate, span));
    }
}

/// Drops a take under way, and the handover it armed.
fn drop_take(f: &mut impl Frontend) {
    if f.tape().taking.take().is_some_and(|t| t.armed) {
        f.session().player().cancel_handover();
    }
}

/// The playing track, its rate, and the span `slot` or the sampler's range
/// marks in it; or says why there is none.
fn range(f: &mut impl Frontend, slot: Option<u8>) -> Option<(PathBuf, u32, (u64, u64))> {
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => {
            f.notify(refusal.into());
            return None;
        }
    };
    let span = match slot {
        Some(n) => f.session_mut().loops_for(Some(&path))[usize::from(n) - 1],
        None => f.sampler().range(Some(&path)),
    };
    match (span, slot) {
        (Some(span), _) => Some((path, rate, span)),
        (None, Some(n)) => {
            f.notify(TapeMessage::EmptySlot(n).into());
            None
        }
        (None, None) => {
            f.notify(TapeMessage::NoRange.into());
            None
        }
    }
}

fn start_load(f: &mut impl Frontend, src: Source) {
    match f.tape().load(src) {
        Ok(()) => f.notify(TapeMessage::Loading.into()),
        Err(e) => {
            f.tape().taking = None;
            f.notify(TapeMessage::Failed(e).into());
        }
    }
}

/// Hands the player over to the tape once a take's loop is in: voice 1 at
/// the player's position inside the range, or at its start once the player
/// reaches it. On the player's stream the handover is on one frame, which
/// the player's callback keeps; a paused player leaves the tape cued there.
fn take_over(f: &mut impl Frontend) {
    let Some(t) = f.tape().taking.clone().filter(|t| t.loaded) else {
        return;
    };
    let player = f.session().player();
    let state = player.status().state;
    let on_track = f.session().playing_track().is_ok_and(|(p, _)| p == t.path);
    let gone = !on_track || state == State::Stopped;
    if t.armed {
        match f.session().player().handover() {
            Handover::Done => return took(f),
            Handover::Armed if !gone => return,
            _ => {}
        }
        f.session().player().cancel_handover();
        f.tape().taking.as_mut().expect("taken above").armed = false;
    }
    if gone {
        f.tape().taking = None;
        return f.notify(TapeMessage::TakeDropped.into());
    }
    let player = f.session().player();
    let status = player.status();
    let rate = f64::from(t.rate);
    let now = player.position().as_secs_f64() * rate;
    let (start, end) = (t.span.0 as f64, t.span.1 as f64);
    let bus = f
        .tape()
        .live
        .as_ref()
        .and_then(|l| l.attached.as_ref().map(Attachment::id));
    // The frame the tape starts on: now, or ahead by the lead on the bus.
    let lead = match (bus, state) {
        (Some(_), State::Playing) => TAKE_LEAD * playr_core::audio::speed_at(status.cents) * rate,
        _ => 0.0,
    };
    let from = start.max(now + lead);
    if from >= end {
        f.tape().taking = None;
        return f.notify(TapeMessage::PastRange.into());
    }
    if now < start && !t.waiting {
        f.tape().taking.as_mut().expect("taken above").waiting = true;
        f.notify(TapeMessage::TakeWaiting.into());
    }
    if now < start && (bus.is_none() || state != State::Playing) {
        return;
    }
    let Some(live) = f.tape().live.as_mut() else {
        return;
    };
    let e = live.extent_of(0);
    let head = e.range.start as f64 + (from - start) * f64::from(e.rate) / rate;
    if let Err(err) = live.handle.set(Setting::Head(0, head)) {
        f.tape().taking = None;
        return f.notify(TapeMessage::Failed(err.to_string()).into());
    }
    match (bus, state) {
        (Some(id), State::Playing) => {
            // Refused while a seek settles or before a track change: tried
            // again next frame.
            let at = Duration::from_secs_f64(from / rate);
            if f.session().player().hand_over(id, at) {
                f.tape().taking.as_mut().expect("taken above").armed = true;
            }
        }
        (None, State::Playing) => {
            // Play pauses the player, as the tape's Play does.
            f.tape().taking = None;
            act(f, TapeAction::Play);
            f.notify(TapeMessage::Took.into());
        }
        _ => {
            f.tape().taking = None;
            f.notify(TapeMessage::Took.into());
        }
    }
}

/// Finishes a take the player's callback handed over: the tape plays and
/// the player paused, on one frame.
fn took(f: &mut impl Frontend) {
    f.tape().taking = None;
    if let Some(live) = f.tape().live.as_mut() {
        live.state.apply(Setting::Play);
    }
    f.notify(TapeMessage::Took.into());
}

/// Starts a bounce until stopped: the mix, recorded to a WAV, which
/// [`end_long`] loads into voice 4.
fn bounce_long(f: &mut impl Frontend) {
    let Some(live) = f.tape().live.as_mut() else {
        return f.notify(TapeMessage::NoTape.into());
    };
    if live.long.is_some() {
        return;
    }
    let loading = live.bounced.is_some()
        || f.tape()
            .loading
            .as_ref()
            .is_some_and(|l| l.bounce.is_some());
    let live = f.tape().live.as_mut().expect("checked above");
    if live.handle.recording() || loading {
        return f.notify(TapeMessage::RecorderBusy.into());
    }
    static BOUNCES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = BOUNCES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("playr-bounce-{}-{n}.wav", std::process::id());
    let path = std::env::temp_dir().join(name);
    match live.handle.record(&path) {
        Ok(()) => {
            live.long = Some((path, Instant::now()));
            f.notify(TapeMessage::Done(TapeAction::Bounce(Some(Bounce::Long))).into());
        }
        Err(e) => f.notify(TapeMessage::Failed(e.to_string()).into()),
    }
}

/// Ends a bounce until stopped, and loads what it recorded into voice 4,
/// as a loop of its length whose rolls are its own other end.
fn end_long(f: &mut impl Frontend) {
    let Some(live) = f.tape().live.as_mut() else {
        return;
    };
    let Some((path, _)) = live.long.take() else {
        return;
    };
    let rate = live.handle.sample_rate();
    let src = match live.handle.stop_recording() {
        Ok(r) if r.frames > 0 => Source {
            name: live.name.clone(),
            path: path.clone(),
            rate,
            span: (0, r.frames),
            circular: true,
            voice: Some(playr_looper::WRITER),
            beats: None,
        },
        Ok(_) => {
            _ = std::fs::remove_file(&path);
            return f.notify(TapeMessage::NothingBounced.into());
        }
        Err(e) => {
            _ = std::fs::remove_file(&path);
            return f.notify(TapeMessage::Failed(e.to_string()).into());
        }
    };
    live.bounced = Some(TempFile(path));
    start_load(f, src);
}

fn record(f: &mut impl Frontend) {
    let samples = f.session().samples_dir().to_path_buf();
    let Some(live) = f.tape().live.as_mut() else {
        return f.notify(TapeMessage::NoTape.into());
    };
    if live.long.is_some() {
        return f.notify(TapeMessage::RecorderBusy.into());
    }
    if let Some(found) = stop_recording(live) {
        return report(f, found);
    }
    let started =
        new_file(&samples, &live.name, "mix").and_then(|path| match live.handle.record(&path) {
            Ok(()) => Ok(path),
            Err(e) => {
                _ = std::fs::remove_dir(path.parent().expect("in its directory"));
                Err(e.to_string())
            }
        });
    match started {
        Ok(path) => f.notify(TapeMessage::Recording(path).into()),
        Err(e) => f.notify(TapeMessage::Failed(e).into()),
    }
}
