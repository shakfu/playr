//! The sampler's edits: auditioning, scrubbing, marks, ranges, slices, and undo.

use super::*;

/// The playing track's peaks, once the sampler view has read them.
pub(super) fn peaks(f: &impl Frontend) -> Option<std::sync::Arc<Peaks>> {
    let status = f.session().player().status();
    sampler::peaks_of(f.sampler(), status.current()).ok()
}

/// How far from a click a mark still counts as under it: one column of the
/// view, so what looks like a hit is one, or 10 ms before anything is drawn.
pub(super) fn near(peaks: &Peaks, scale: Option<sampler::Scale>) -> u64 {
    match scale {
        Some(scale) => scale.per_column.max(1),
        None => (peaks.rate / 100).max(1) as u64,
    }
}

/// Plays once, and pauses at the end of: the selected mark up to the next,
/// or the range when an end of it is selected. Otherwise whichever of these
/// has both ends: the range, the planned slice the playhead is in, or the
/// region around it.
///
/// The range first, because setting one is how a listener says what they mean;
/// then the plan, which is what `:slice` is about to write; then the region,
/// which is what `:slice region` would take.
pub(super) fn audition(f: &mut impl Frontend) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let (rate, last) = (peaks.rate, peaks.frames);
    // A selection says what is meant more plainly than the playhead.
    match f.sampler().selected(Some(&path)) {
        Some(Selected::Mark(frame)) => {
            let end = (mark_frames(f, &path).into_iter())
                .find(|&m| m > frame)
                .unwrap_or(last);
            return play_once(f, path, (frame, end), rate);
        }
        Some(Selected::Edge(_)) => {
            if let Some(range) = f.sampler().range(Some(&path)) {
                return play_once(f, path, range, rate);
            }
        }
        Some(Selected::Slice(start)) => {
            if let Some((plan, i)) = planned_slice(f, start) {
                return play_once(f, path, (start, plan.spans[i].1.unwrap_or(last)), rate);
            }
        }
        None => {}
    }
    // A span with no end runs to the end of the track.
    let ends = |end: Option<u64>| end.unwrap_or(last);

    // An audition pauses on its span's end, which is the next span's start.
    let again = f
        .sampler()
        .auditioned
        .as_ref()
        .filter(|(p, _, end)| *p == path && at.abs_diff(*end) <= u64::from(rate) / 100)
        .map(|&(_, start, end)| (start, end));
    // A planned slice is cut from the range, so it is the narrower choice.
    let slice = f.sampler().pending.as_ref().and_then(|plan| {
        let spans: Vec<(u64, u64)> = plan.spans.iter().map(|&(s, e)| (s, ends(e))).collect();
        again
            .filter(|span| spans.contains(span))
            .or_else(|| spans.into_iter().rfind(|&(start, _)| start <= at))
    });
    let span = slice
        .or_else(|| f.sampler().range(Some(&path)))
        .unwrap_or_else(|| {
            let marks: Vec<u64> = f
                .session_mut()
                .marks_for(Some(&path))
                .iter()
                .map(|m| m.frame)
                .collect();
            let (start, end) = playr_core::samples::region(&marks, at);
            again.unwrap_or((start, ends(end)))
        });
    play_once(f, path, span, rate);
}

/// Selects and plays the planned slice after the selected one, or before it;
/// with none selected, after the one last heard, or the one under the
/// playhead. Past either end it wraps round.
pub(super) fn audition_slice(f: &mut impl Frontend, forward: bool) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let Some(plan) = f.sampler().pending.as_ref() else {
        return f.notify(Message::NoSlicesPlanned);
    };
    let spans: Vec<(u64, u64)> = plan
        .spans
        .iter()
        .map(|&(start, end)| (start, end.unwrap_or(peaks.frames)))
        .collect();
    let near = u64::from(peaks.rate) / 100;
    let heard = f
        .sampler()
        .auditioned
        .as_ref()
        .filter(|(p, start, end)| *p == path && at >= *start && at <= end + near)
        .and_then(|&(_, start, end)| spans.iter().position(|&s| s == (start, end)));
    let under = spans.iter().rposition(|&(start, _)| start <= at);
    let selected = (f.sampler().selected_slice(Some(&path)))
        .and_then(|s| spans.iter().position(|span| span.0 == s));
    let n = spans.len();
    let to = match (selected.or(heard).or(under), forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    f.sampler_mut().selected = Some((path.clone(), Selected::Slice(spans[to].0)));
    play_once(f, path, spans[to], peaks.rate);
}

/// The playing track, its peaks and the playhead in frames, or why not.
pub(super) fn hearing(
    f: &mut impl Frontend,
) -> Option<(std::path::PathBuf, std::sync::Arc<Peaks>, u64)> {
    let status = f.session().player().status();
    let Some(path) = status.current().cloned() else {
        f.notify(Refusal::NothingPlaying.into());
        return None;
    };
    let Some(peaks) = peaks(f) else {
        f.notify(Message::NoWaveform);
        return None;
    };
    let at = sampler::frame_of(f.session().player().position(), peaks.rate);
    Some((path, peaks, at))
}

/// Plays `start..end` once, faded as an export would write it.
pub(super) fn play_once(
    f: &mut impl Frontend,
    path: std::path::PathBuf,
    (start, end): (u64, u64),
    rate: u32,
) {
    if end <= start {
        return f.notify(Message::NothingToAudition);
    }
    // As `Session::slice_job` decides: a looping range is written unfaded.
    let status = f.session().player().status();
    let looped = status.looping.is_some() && f.sampler().range(Some(&path)) == Some((start, end));
    let fades = match f.session().slice_edges() {
        playr_core::samples::Edges::Fade if !looped => {
            let fades = f.session().fades();
            let frames = |d: Duration| sampler::frame_of(d, rate);
            (frames(fades.fade_in), frames(fades.fade_out))
        }
        _ => (0, 0),
    };
    f.session().send(Cmd::PlayOnce(start, end, fades));
    f.sampler_mut().auditioned = Some((path, start, end));
    f.notify(Message::Auditioning);
}

/// Plays [`sampler::SCRUB`] from `at` once, faded so grains do not click.
/// No message: a drag sends one a frame, which would hide the range's.
pub(super) fn scrub(f: &mut impl Frontend, at: Duration) {
    let rate = match f.session().playing_track() {
        Ok((_, rate)) => rate,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let frames = |d: Duration| sampler::frame_of(d, rate);
    let start = frames(at);
    let fade = frames(sampler::SCRUB_FADE);
    f.session().send(Cmd::PlayOnce(
        start,
        start + frames(sampler::SCRUB),
        (fade, fade),
    ));
}

/// `at`, moved to the nearest zero crossing when the sampler view shows with
/// snap on and its waveform read; otherwise as it is.
pub(super) fn snapped(f: &impl Frontend, at: Duration) -> Duration {
    if f.view() != View::Sampler || !f.sampler().snap {
        return at;
    }
    match peaks(f) {
        Some(peaks) => sampler::time_of(
            sampler::snap(&peaks, sampler::frame_of(at, peaks.rate)),
            peaks.rate,
        ),
        None => at,
    }
}

/// Keeps a running loop on the range as it changes; a range cleared or left
/// with one end stops it.
pub(super) fn follow_loop(f: &impl Frontend) {
    let status = f.session().player().status();
    if status.looping.is_some() {
        let range = f.sampler().range(status.current());
        f.session().send(Cmd::Loop(range));
    }
}

/// Sets the range to the region between the marks around the playhead.
pub(super) fn range_to_region(f: &mut impl Frontend) -> Result<(), Message> {
    let status = f.session().player().status();
    let path = status.current().cloned().ok_or(Refusal::NothingPlaying)?;
    let peaks = peaks(f).ok_or(Message::NoWaveform)?;
    let at = sampler::frame_of(f.session().player().position(), peaks.rate);
    let marks: Vec<u64> = (f.session_mut().marks_for(Some(&path)).iter())
        .map(|m| m.frame)
        .collect();
    let (start, end) = playr_core::samples::region(&marks, at);
    let end = end.unwrap_or(peaks.frames);
    if end <= start {
        return Err(Message::EmptyRange);
    }
    f.sampler_mut().range = Some(sampler::Range {
        path,
        start: Some(start),
        end: Some(end),
    });
    Ok(())
}

/// Recalls loop `slot` into the range and loops it, or saves the range to
/// the slot, or empties it.
pub(super) fn loop_slot(f: &mut impl Frontend, slot: u8, op: crate::action::SlotOp) {
    use crate::action::SlotOp;
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let saved = f.session_mut().loops_for(Some(&path))[usize::from(slot) - 1];
    match (op, saved) {
        (SlotOp::Use, Some((start, end))) => {
            f.sampler_mut().range = Some(sampler::Range {
                path,
                start: Some(start),
                end: Some(end),
            });
            act(Action::Loop(Some(true)), f);
            f.notify(Message::LoopRecalled {
                slot,
                start,
                end,
                rate,
            });
        }
        (SlotOp::Use | SlotOp::Save, _) => match f.sampler().range(Some(&path)) {
            Some(range) => {
                let notice = f.session_mut().save_loop(slot, range);
                f.notify(notice.into());
            }
            None => f.notify(Message::NoRangeToSave(slot)),
        },
        (SlotOp::Clear, _) => {
            let notice = f.session_mut().clear_loop(slot);
            f.notify(notice.into());
        }
    }
}

/// Moves the range's ends set on the playing track to the nearest zero
/// crossings, as ends set with snap on would be. A range too short to keep
/// both ends apart stays as it is.
pub(super) fn snap_range(f: &mut impl Frontend) {
    let Some(peaks) = peaks(f) else { return };
    let Some(path) = f.session().player().status().current().cloned() else {
        return;
    };
    let (start, end) = f.sampler().range_ends(Some(&path));
    let snap = |e: Option<u64>| e.map(|e| sampler::snap(&peaks, e));
    let (start, end) = (snap(start), snap(end));
    if start.zip(end).is_some_and(|(a, b)| a >= b) || (start, end) == (None, None) {
        return;
    }
    f.sampler_mut().range = Some(sampler::Range { path, start, end });
    follow_loop(f);
}

/// Marks `at`, or the position now. In the sampler view the mark may snap,
/// and may fall as close as a frame to another, where fine cuts need it.
///
/// The new mark is selected.
pub(super) fn mark(f: &mut impl Frontend, at: Option<Duration>) {
    let at = at.unwrap_or_else(|| f.session().player().position());
    let notice = if f.view() == View::Sampler {
        let at = snapped(f, at);
        f.session_mut().add_mark_within(Some(at), Duration::ZERO)
    } else {
        f.session_mut().add_mark(Some(at))
    };
    if let (Notice::Done(Outcome::Marked { at, .. }), Ok((path, rate))) =
        (&notice, f.session().playing_track())
    {
        let frame = sampler::frame_of(*at, rate);
        f.sampler_mut().selected = Some((path, Selected::Mark(frame)));
    }
    f.notify(notice.into());
}

/// The selection on the playing track.
pub(super) fn selected(f: &impl Frontend) -> Option<(PathBuf, Selected)> {
    let path = f.session().player().status().current().cloned()?;
    let selected = f.sampler().selected(Some(&path))?;
    Some((path, selected))
}

/// The playing track's marks, in frames.
pub(super) fn mark_frames(f: &mut impl Frontend, path: &PathBuf) -> Vec<u64> {
    (f.session_mut().marks_for(Some(path)).iter())
        .map(|m| m.frame)
        .collect()
}

/// Selects the next mark after the selected one, or the previous; from the
/// playhead with none selected. Plays it once, up to the mark after it.
pub(super) fn select_mark(f: &mut impl Frontend, forward: bool) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let from = f.sampler().selected_mark(Some(&path)).unwrap_or(at);
    let marks = mark_frames(f, &path);
    let next = marks
        .iter()
        .copied()
        .filter(|&m| if forward { m > from } else { m < from })
        .min_by_key(|&m| m.abs_diff(from));
    let Some(frame) = next else {
        return f.notify(if forward {
            Refusal::NoLaterMark.into()
        } else {
            Refusal::NoEarlierMark.into()
        });
    };
    f.sampler_mut().selected = Some((path.clone(), Selected::Mark(frame)));
    let end = marks
        .into_iter()
        .find(|&m| m > frame)
        .unwrap_or(peaks.frames);
    play_once(f, path, (frame, end), peaks.rate);
}

/// Moves the selected mark from `from` to `to`, keeping it selected.
pub(super) fn move_selected_mark(f: &mut impl Frontend, from: u64, to: u64) {
    let notice = f.session_mut().move_mark(from, to);
    if let (Notice::Done(Outcome::MarkMoved { .. }), Some(path)) =
        (&notice, f.session().player().status().current().cloned())
    {
        f.sampler_mut().selected = Some((path, Selected::Mark(to)));
    }
    f.notify(notice.into());
}

/// The planned slices on the playing track, and the index of the one
/// starting at `start`.
pub(super) fn planned_slice(f: &impl Frontend, start: u64) -> Option<(Plan, usize)> {
    let path = f.session().player().status().current().cloned()?;
    let plan = f.sampler().pending.clone().filter(|p| p.job.path == path)?;
    let i = plan.index_of(start)?;
    Some((plan, i))
}

/// Moves the start of the planned slice starting at `from` to `to`, as
/// [`Plan::move_start`] does, keeping it selected.
pub fn move_slice(f: &mut impl Frontend, from: u64, to: u64) {
    let Some((mut plan, i)) = planned_slice(f, from) else {
        return f.notify(Message::NothingSelected);
    };
    let to = plan.move_start(i, to, peaks(f).as_deref());
    let (path, rate) = (plan.job.path.clone(), plan.job.rate);
    f.sampler_mut().pending = Some(plan);
    f.sampler_mut().selected = Some((path, Selected::Slice(to)));
    f.notify(Message::SliceMoved {
        slice: i + 1,
        at: to,
        rate,
    });
}

/// Joins the planned slice starting at `start` to the one before it.
pub(super) fn remove_slice(f: &mut impl Frontend, start: u64) {
    let Some((mut plan, i)) = planned_slice(f, start) else {
        return f.notify(Message::NothingSelected);
    };
    if !plan.join(i) {
        return f.notify(Message::FirstSlice);
    }
    f.sampler_mut().pending = Some(plan);
    f.sampler_mut().selected = None;
    f.notify(Message::SlicesJoined { slice: i });
}

/// Selects a range end, and centres on it while fitting. False when that end is not set.
pub(super) fn select_edge(f: &mut impl Frontend, edge: sampler::Edge) -> bool {
    f.sampler_mut().edge = edge;
    f.sampler_mut().fit_edge = true;
    let path = f.session().player().status().current().cloned();
    let (start, end) = f.sampler().range_ends(path.as_ref());
    let set = match edge {
        sampler::Edge::Start => start.is_some(),
        sampler::Edge::End => end.is_some(),
    };
    match path.filter(|_| set) {
        Some(path) => {
            f.sampler_mut().selected = Some((path, Selected::Edge(edge)));
            f.notify(Message::Edge(edge));
            true
        }
        None => {
            f.notify(Message::NoEdge(edge));
            false
        }
    }
}

/// Sets `edge` of the range at `at`, or at the playhead, snapped when snap is on.
pub(super) fn range_end_at(f: &mut impl Frontend, edge: sampler::Edge, at: Option<Duration>) {
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let at = at.unwrap_or(f.session().player().position());
    let at = sampler::frame_of(snapped(f, at), rate);
    match edge {
        sampler::Edge::Start => f.sampler_mut().set_range_start(&path, at),
        sampler::Edge::End => f.sampler_mut().set_range_end(&path, at),
    }
    let (start, end) = f.sampler().range_ends(Some(&path));
    follow_loop(f);
    f.notify(Message::Range { start, end, rate });
}

/// Marks the start of each planned slice on the playing track, but the
/// track's own start, in one step for undo.
pub(super) fn mark_slices(f: &mut impl Frontend) {
    let Ok((path, _)) = f.session().playing_track() else {
        return f.notify(Refusal::NothingPlaying.into());
    };
    let Some(plan) = f.sampler().pending.clone().filter(|p| p.job.path == path) else {
        return f.notify(Message::NoSlicesPlanned);
    };
    let mut frames = mark_frames(f, &path);
    let before = frames.len();
    frames.extend(plan.spans.iter().map(|s| s.0).filter(|&s| s > 0));
    frames.sort_unstable();
    frames.dedup();
    let added = frames.len() - before;
    match f.session_mut().set_marks(&frames) {
        Ok(()) => f.notify(Message::SlicesMarked { added }),
        Err(notice) => f.notify(notice.into()),
    }
}

/// Moves `edge` of the range, as a nudge moves the playhead.
pub(super) fn move_edge(f: &mut impl Frontend, edge: sampler::Edge, nudge: crate::action::Nudge) {
    let status = f.session().player().status();
    let Some(path) = status.current().cloned() else {
        return f.notify(Refusal::NothingPlaying.into());
    };
    let (Some(peaks), Some(scale)) = (peaks(f), f.sampler().scale) else {
        return f.notify(Message::NoWaveform);
    };
    let (start, end) = f.sampler().range_ends(Some(&path));
    let from = match edge {
        sampler::Edge::Start => start,
        sampler::Edge::End => end,
    };
    let Some(from) = from else {
        return f.notify(Message::NoEdge(edge));
    };
    let to = sampler::nudge(&peaks, from, scale.frames(nudge), f.sampler().snap);
    set_edge(f, &path, edge, to);
    let (start, end) = f.sampler().range_ends(Some(&path));
    f.notify(Message::Range {
        start,
        end,
        rate: peaks.rate,
    });
}

/// Puts `edge` of the range on `path` at `to`, a frame short of the other
/// end so the range stays, and keeps a running loop on it.
pub fn set_edge(f: &mut impl Frontend, path: &PathBuf, edge: sampler::Edge, to: u64) {
    let (start, end) = f.sampler().range_ends(Some(path));
    match edge {
        sampler::Edge::Start => {
            let to = end.map_or(to, |e| to.min(e.saturating_sub(1)));
            f.sampler_mut().set_range_start(path, to);
        }
        sampler::Edge::End => {
            let to = start.map_or(to, |s| to.max(s + 1));
            f.sampler_mut().set_range_end(path, to);
        }
    }
    follow_loop(f);
}

/// The playing track's marks and range, before an edit.
pub fn before(f: &mut impl Frontend) -> Option<Before> {
    let path = f.session().player().status().current().cloned()?;
    let marks = mark_frames(f, &path);
    let range = f.sampler().range.clone().filter(|r| r.path == path);
    let plan = f.sampler().pending.clone().filter(|p| p.job.path == path);
    let selected = f.sampler().selected(Some(&path));
    Some(Before {
        path,
        marks,
        range,
        plan,
        selected,
    })
}

/// Whether `a` and `b` are the same cut of the same track, with or without
/// slice starts set by hand.
pub(super) fn same_cut(a: &Plan, b: &Plan) -> bool {
    let cut = |p: &Plan| Job {
        cuts: None,
        ..p.job.clone()
    };
    cut(a) == cut(b)
}

/// Whether `plan` has slice starts set by hand.
pub(super) fn edited(plan: &Option<Plan>) -> bool {
    plan.as_ref().is_some_and(|p| p.job.cuts.is_some())
}

/// Keeps `state` for undo, and ends redo.
pub fn remember(f: &mut impl Frontend, state: Before) {
    keep(&mut f.sampler_mut().history, state);
    f.sampler_mut().future.clear();
}

/// After an action: keeps `before` for undo if the action changed the marks
/// or range, or edited the planned slices, and drops a selection that is gone.
///
/// A plan made or replaced is not an edit here: undo would put back a cut that
/// a later one replaced. An edited plan replaced or discarded is kept where
/// that happens, as its edits would otherwise be lost.
pub fn settle(f: &mut impl Frontend, before: Option<Before>) {
    let after = self::before(f);
    if let (Some(before), Some(after)) = (before, &after) {
        let edited = match (&before.plan, &after.plan) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        let changed = before.marks != after.marks || before.range != after.range || edited;
        if before.path == after.path && changed {
            remember(f, before);
        }
    }
    let gone = match (f.sampler().selected.clone(), &after) {
        (None, _) => false,
        (Some((path, selected)), Some(after)) if path == after.path => match selected {
            Selected::Mark(frame) => !after.marks.contains(&frame),
            Selected::Edge(edge) => {
                let r = after.range.as_ref();
                match edge {
                    sampler::Edge::Start => r.and_then(|r| r.start).is_none(),
                    sampler::Edge::End => r.and_then(|r| r.end).is_none(),
                }
            }
            // Planned again, the new plan checks it when it lands.
            Selected::Slice(start) => match &after.plan {
                Some(plan) => !plan.spans.iter().any(|s| s.0 == start),
                None => f.sampler().planning.is_none(),
            },
        },
        _ => true,
    };
    if gone {
        f.sampler_mut().selected = None;
    }
}

/// Pushes `state` onto an undo or redo stack, dropping the oldest past
/// [`sampler::UNDO_DEPTH`].
pub(super) fn keep(stack: &mut Vec<Before>, state: Before) {
    stack.push(state);
    let over = stack.len().saturating_sub(sampler::UNDO_DEPTH);
    stack.drain(..over);
}

/// Puts back the marks and range as they were before the last edit, keeping
/// the state now for redo; or, with `redo`, the other way round.
pub(super) fn undo(f: &mut impl Frontend, redo: bool) {
    let (none, done) = match redo {
        false => (Message::NothingToUndo, Message::Undone),
        true => (Message::NothingToRedo, Message::Redone),
    };
    let path = match f.session().playing_track() {
        Ok((path, _)) => path,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let Some(then) = stacks(f.sampler_mut(), redo).0.pop() else {
        return f.notify(none);
    };
    if then.path != path {
        f.sampler_mut().history.clear();
        f.sampler_mut().future.clear();
        return f.notify(none);
    }
    let now = before(f).expect("a track is playing");
    if let Err(notice) = restore(f, &then) {
        // Kept, so the step can be tried again: nothing of it was done.
        stacks(f.sampler_mut(), redo).0.push(then);
        return f.notify(notice.into());
    }
    keep(stacks(f.sampler_mut(), redo).1, now);
    settle(f, None);
    f.notify(done);
}

/// The stack an undo takes from and the one it keeps the state now on; the
/// other way round for a redo.
pub(super) fn stacks(s: &mut Sampler, redo: bool) -> (&mut Vec<Before>, &mut Vec<Before>) {
    match redo {
        false => (&mut s.history, &mut s.future),
        true => (&mut s.future, &mut s.history),
    }
}

/// Makes the playing track's marks and range `state`'s.
pub(super) fn restore(f: &mut impl Frontend, state: &Before) -> Result<(), Notice> {
    f.session_mut().set_marks(&state.marks)?;
    f.sampler_mut().range = state.range.clone();
    follow_loop(f);
    // The plan shown, if it is still the cut `state` edited, or if this step
    // replaced or discarded an edited plan.
    let plan = f.sampler_mut().pending.take();
    let swap = edited(&plan)
        || edited(&state.plan)
        || matches!((&plan, &state.plan), (Some(now), Some(then)) if same_cut(now, then));
    f.sampler_mut().pending = match swap {
        true => state.plan.clone(),
        false => plan,
    };
    if swap {
        // A plan still being made would replace the one put back.
        f.sampler_mut().planning = None;
        f.sampler_mut().onsets_wanted = None;
        // Edges or fades changed since: plan it their way, keeping its starts.
        if let Some(plan) = f.take_plan() {
            match f.session().plans_current(&plan.job) {
                true => f.sampler_mut().pending = Some(plan),
                false => {
                    let id = f.session_mut().replan(plan.job);
                    f.planning(id);
                }
            }
        }
    }
    f.sampler_mut().selected = state.selected.map(|s| (state.path.clone(), s));
    Ok(())
}

/// Plans slices of the playing track in the sampler view, which shows them
/// before they are written; elsewhere, writes them at once.
pub(super) fn slice(f: &mut impl Frontend, slicing: Slicing) {
    let range = {
        let status = f.session().player().status();
        f.sampler().range(status.current())
    };
    let cut = match slicing {
        Slicing::Region => Cut::Region,
        Slicing::Marks => Cut::Marks,
        Slicing::Equal(n) => Cut::Equal(n),
        Slicing::Onsets(s) => Cut::Onsets(s.unwrap_or(f.onset_sensitivity())),
        Slicing::Beats(n) => match beats(f, n) {
            Some(cut) => cut,
            None => return,
        },
    };
    cut_with(f, cut, range);
}

/// The cut every `n` beats at the playing track's tempo. A track with none
/// is analysed first and cut by [`tempo_found`] when that lands.
pub(super) fn beats(f: &mut impl Frontend, n: f32) -> Option<Cut> {
    let path = match f.session().playing_track() {
        Ok((path, _)) => path,
        Err(refusal) => {
            f.notify(refusal.into());
            return None;
        }
    };
    if let Some(cut) = beat_cut(f, &path, n) {
        return Some(cut);
    }
    match f.session_mut().analyze(Some(path.clone())) {
        Ok(job) => {
            f.sampler_mut().tempo_for = Some((job, path, n));
            f.notify(Outcome::FindingTempo.into());
        }
        Err(refusal) => f.notify(refusal.into()),
    }
    None
}

/// Cuts every `n` beats once the analysis [`beats`] started has landed, if
/// `path` still plays. Analysed and still without a tempo, it is refused.
pub(crate) fn tempo_found(f: &mut impl Frontend, path: &std::path::Path, n: f32) {
    let playing = f.session().playing_track().ok().map(|(p, _)| p);
    if playing.as_deref() != Some(path) {
        return;
    }
    let Some(cut) = beat_cut(f, path, n) else {
        return f.notify(Refusal::NoTempo.into());
    };
    let range = {
        let status = f.session().player().status();
        f.sampler().range(status.current())
    };
    cut_with(f, cut, range);
}

/// The cut every `n` beats at `path`'s tempo, its grid's when it has one.
pub(super) fn beat_cut(f: &impl Frontend, path: &std::path::Path, n: f32) -> Option<Cut> {
    let bpm = f.session().bpm(path)?;
    Some(Cut::Beats { beats: n, bpm })
}

/// Plans `cut` in the sampler, or exports it from any other view.
pub(super) fn cut_with(f: &mut impl Frontend, cut: Cut, range: Option<(u64, u64)>) {
    if f.view() == View::Sampler {
        // A slider being dragged asks every frame; a sensitivity that comes
        // while a plan is being made waits for it, and only the last is planned.
        if let (Cut::Onsets(s), Some(_)) = (cut, f.sampler().planning) {
            f.sampler_mut().onsets_wanted = Some(s);
            return;
        }
        f.sampler_mut().onsets_wanted = None;
        match f.session_mut().plan_slices(cut, range) {
            Ok(job) => {
                f.planning(job);
                f.notify(Outcome::PlanStarted.into());
            }
            Err(refusal) => f.notify(refusal.into()),
        }
    } else {
        match f.session_mut().export(cut, range) {
            Ok(_) => f.notify(Outcome::ExportStarted.into()),
            Err(refusal) => f.notify(refusal.into()),
        }
    }
}
