//! Marks, labels and loop slots on a track, and the loop library.

use super::*;

impl Session {
    // --- marks ---

    /// The marks in the track at `path`, earliest first, read from the
    /// library when `path` is not the track they were last read for.
    pub fn marks_for(&mut self, path: Option<&PathBuf>) -> &[Mark] {
        if self.marks_for.as_ref() != path {
            self.marks = path
                .and_then(|p| query::marks(&self.conn, &p.to_string_lossy()).ok())
                .unwrap_or_default();
            self.labels = path
                .and_then(|p| query::labels(&self.conn, &p.to_string_lossy()).ok())
                .unwrap_or_default();
            self.marks_for = path.cloned();
        }
        &self.marks
    }

    /// The labels of the marks in the track at `path`: frame and label,
    /// earliest first, read as [`Session::marks_for`] reads the marks.
    pub fn labels_for(&mut self, path: Option<&PathBuf>) -> &[(u64, String)] {
        self.marks_for(path);
        &self.labels
    }

    /// Reads the labels of the marks kept for `path` again, after an edit.
    pub(super) fn relabel(&mut self, path: &Path) {
        self.labels = query::labels(&self.conn, &path.to_string_lossy()).unwrap_or_default();
    }

    /// Labels the mark at `frame` in the playing track, or clears its label
    /// when `label` is blank.
    pub fn label_mark(&mut self, frame: u64, label: &str) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let label = Some(label.trim()).filter(|l| !l.is_empty());
        match query::set_label(&self.conn, &path.to_string_lossy(), frame, label) {
            Ok(true) => {
                self.relabel(&path);
                Outcome::Labelled {
                    at: Duration::from_secs_f64(frame as f64 / rate.max(1) as f64),
                    label: label.map(String::from),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::Mark,
                error: e.to_string(),
            },
        }
    }

    /// Writes the playing track's marks, with their labels, as `format`, to
    /// the samples directory, named after the track. A file written before
    /// is replaced: it is made from the marks, which may have changed.
    pub fn export_marks(&mut self, format: crate::labels::MarkFile) -> Notice {
        let (path, _) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        let marks = self.marks_for(Some(&path)).to_vec();
        if marks.is_empty() {
            return Refusal::NoMarks.into();
        }
        let labelled: Vec<(Duration, Option<String>)> = marks
            .iter()
            .map(|m| {
                let label = self.labels.iter().find(|(f, _)| *f == m.frame);
                (m.time(), label.map(|(_, l)| l.clone()))
            })
            .collect();
        let text = match format {
            crate::labels::MarkFile::Audacity => crate::labels::audacity(&labelled),
            crate::labels::MarkFile::Cue => crate::labels::cue(&path, &labelled),
        };
        let file = self
            .samples
            .join(format!("{}{}", samples::name_for(&path), format.suffix()));
        let written =
            std::fs::create_dir_all(&self.samples).and_then(|()| std::fs::write(&file, text));
        match written {
            Ok(()) => Outcome::MarksExported {
                path: file,
                format,
                marks: marks.len(),
            }
            .into(),
            Err(e) => Notice::Failed {
                task: Task::Export,
                error: format!("{}: {e}", file.display()),
            },
        }
    }

    /// The marks in the track at `path`, earliest first, read afresh: for a
    /// track other than the one [`Session::marks_for`] keeps, as a DJ deck's.
    pub fn marks_of(&self, path: &Path) -> Vec<Mark> {
        query::marks(&self.conn, &path.to_string_lossy()).unwrap_or_default()
    }

    /// The loops saved in the track at `path`, by slot from 1, in source frames.
    pub fn loops_for(&mut self, path: Option<&PathBuf>) -> Loops {
        if self.loops_for.as_ref() != path {
            self.loops = [None; LOOP_SLOTS as usize];
            let rows = path.and_then(|p| query::loops(&self.conn, &p.to_string_lossy()).ok());
            for (slot, start, end) in rows.into_iter().flatten() {
                if let Some(held) = self.loops.get_mut(usize::from(slot).wrapping_sub(1)) {
                    *held = Some((start, end));
                }
            }
            self.loops_for = path.cloned();
        }
        self.loops
    }

    /// Saves `span` as loop `slot`, from 1, of the playing track.
    pub fn save_loop(&mut self, slot: u8, span: (u64, u64)) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.loops_for = None;
        let saved = query::save_loop(&self.conn, &path.to_string_lossy(), slot, span, rate);
        self.reload_loops();
        match saved {
            Ok(()) => Outcome::LoopSaved {
                slot,
                kept: self.has_library_file(),
            }
            .into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// Empties loop `slot` of the playing track.
    pub fn clear_loop(&mut self, slot: u8) -> Notice {
        let (path, _) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.loops_for = None;
        let cleared = query::clear_loop(&self.conn, &path.to_string_lossy(), slot);
        self.reload_loops();
        match cleared {
            Ok(_) => Outcome::LoopCleared { slot }.into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// The loop library as the Loops view lists it.
    pub fn loop_list(&self) -> &[db::loops::Loop] {
        &self.loop_list
    }

    /// The search the Loops view lists, if any.
    pub fn loop_search(&self) -> Option<&str> {
        self.loop_search.as_deref()
    }

    /// Lists the loops `query` matches, as `db::loops::search` reads it, or
    /// every loop for `None`.
    pub fn search_loops(&mut self, query: Option<&str>) {
        self.loop_search = query.map(str::to_owned).filter(|q| !q.trim().is_empty());
        self.reload_loops();
    }

    pub(super) fn reload_loops(&mut self) {
        self.loop_list = match &self.loop_search {
            Some(q) => db::loops::search(&self.conn, q),
            None => db::loops::all(&self.conn),
        }
        .unwrap_or_default();
    }

    /// The loops `query` matches, as `db::loops::search` reads it, or every
    /// loop for a blank one. The Loops view's list stays as it is.
    pub fn find_loops(&self, query: &str) -> Vec<db::loops::Loop> {
        match query.trim() {
            "" => db::loops::all(&self.conn),
            q => db::loops::search(&self.conn, q),
        }
        .unwrap_or_default()
    }

    /// Loop `id` of the library.
    pub fn library_loop(&self, id: i64) -> Option<db::loops::Loop> {
        db::loops::get(&self.conn, id).ok().flatten()
    }

    /// Adds the loop file at `path`, which playr wrote, to the loop library,
    /// with `beats` when its source loop gave them.
    pub fn add_loop_file(&mut self, path: &Path, beats: Option<u32>) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let Some(crate::loops::Found::Loop(mut l)) = crate::loops::read_loop(&path, dir) else {
            return Err(format!("{}: not a readable loop", path.display()));
        };
        l.origin = db::loops::Origin::Tape;
        if let Some(b) = beats {
            l.beats = Some(b);
            l.bpm = Some(crate::loops::bpm(b, l.frames(), l.rate));
            l.bpm_from = Some(db::loops::TempoFrom::User);
        }
        db::loops::upsert_file(&self.conn, &l).map_err(|e| e.to_string())?;
        self.reload_loops();
        Ok(())
    }

    /// Takes loop `id` out of the library, emptying the slot it was; its file
    /// stays. Returns its name.
    pub fn remove_loop(&mut self, id: i64) -> Result<String, String> {
        let l = self.library_loop(id).ok_or("no such loop")?;
        db::loops::remove(&self.conn, id).map_err(|e| e.to_string())?;
        self.loops_for = None;
        self.reload_loops();
        Ok(l.name)
    }

    /// Sets loop `id`'s beats, and its tempo from them. Returns the loop.
    pub fn set_loop_beats(&mut self, id: i64, beats: u32) -> Result<db::loops::Loop, String> {
        db::loops::set_beats(&self.conn, id, beats).map_err(|e| e.to_string())?;
        self.reload_loops();
        self.library_loop(id).ok_or_else(|| "no such loop".into())
    }

    /// Sets loop `id`'s key, or clears it. Returns the loop.
    pub fn set_loop_key(&mut self, id: i64, key: Option<&str>) -> Result<db::loops::Loop, String> {
        db::loops::set_key(&self.conn, id, key).map_err(|e| e.to_string())?;
        self.reload_loops();
        self.library_loop(id).ok_or_else(|| "no such loop".into())
    }

    /// The playing track and how many loops it keeps, before asking to clear
    /// them all.
    pub fn loops_to_clear(&mut self) -> Result<(PathBuf, usize), Refusal> {
        let (path, _) = self.playing_track()?;
        match self.loops_for(Some(&path)).iter().flatten().count() {
            0 => Err(Refusal::NoLoops),
            n => Ok((path, n)),
        }
    }

    /// Empties every loop slot of the track at `path`, the one asked about.
    pub fn clear_loops(&mut self, path: &Path) -> Notice {
        self.loops_for = None;
        let cleared = query::clear_loops(&self.conn, &path.to_string_lossy());
        self.reload_loops();
        match cleared {
            Ok(_) => Outcome::LoopsCleared.into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// Marks `at` in the playing track, or its position now, unless a mark is
    /// already within [`MARK_NEAR`].
    pub fn add_mark(&mut self, at: Option<Duration>) -> Notice {
        self.add_mark_within(at, MARK_NEAR)
    }

    /// As [`Session::add_mark`], refusing a mark within `near` of another, or
    /// on the same frame.
    pub fn add_mark_within(&mut self, at: Option<Duration>, near: Duration) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let at = at.unwrap_or_else(|| self.player.position());
        let mark = Mark::at_time(at, rate);
        if let Some(other) = self
            .marks
            .iter()
            .find(|m| m.frame == mark.frame || m.time().abs_diff(at) < near)
        {
            return Refusal::AlreadyMarked { at: other.time() }.into();
        }
        if let Err(e) = query::add_mark(&self.conn, &path.to_string_lossy(), mark) {
            return Notice::Failed {
                task: Task::Mark,
                error: e.to_string(),
            };
        }
        self.marks.push(mark);
        self.marks.sort_by_key(|m| m.frame);
        Outcome::Marked {
            at,
            kept: self.has_library_file(),
        }
        .into()
    }

    /// The mark within `within` frames of `frame` in the playing track, nearest
    /// first. For a frontend acting on "the mark under the cursor".
    pub fn mark_near(&mut self, frame: u64, within: u64) -> Option<Mark> {
        let path = self.playing_track().ok()?.0;
        self.marks_for(Some(&path));
        self.marks
            .iter()
            .filter(|m| m.frame.abs_diff(frame) <= within)
            .min_by_key(|m| m.frame.abs_diff(frame))
            .copied()
    }

    /// Moves the mark at `from` to `to` in the playing track.
    pub fn move_mark(&mut self, from: u64, to: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let time = |f: u64| Duration::from_secs_f64(f as f64 / rate.max(1) as f64);
        if !self.marks.iter().any(|m| m.frame == from) {
            return Refusal::NoMarkHere.into();
        }
        if let Some(other) = self.marks.iter().find(|m| m.frame == to && m.frame != from) {
            return Refusal::MarkInTheWay { at: other.time() }.into();
        }
        match query::move_mark(&self.conn, &path.to_string_lossy(), from, to) {
            Ok(true) => {
                self.marks_for(None);
                self.marks_for(Some(&path));
                Outcome::MarkMoved {
                    from: time(from),
                    to: time(to),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::MoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Removes the mark at `frame` in the playing track, wherever it sits in
    /// the order marks were made.
    pub fn remove_mark(&mut self, frame: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_mark(&self.conn, &path.to_string_lossy(), frame) {
            Ok(true) => {
                self.marks.retain(|m| m.frame != frame);
                self.relabel(&path);
                Outcome::MarkRemoved {
                    at: Duration::from_secs_f64(frame as f64 / rate.max(1) as f64),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Makes the playing track's marks those at `frames`, in one transaction,
    /// so undo and redo put back every mark or, on an error, none.
    pub fn set_marks(&mut self, frames: &[u64]) -> Result<(), Notice> {
        let (path, rate) = self.playing_track()?;
        let now: Vec<u64> = self
            .marks_for(Some(&path))
            .iter()
            .map(|m| m.frame)
            .collect();
        let remove: Vec<u64> = now
            .iter()
            .copied()
            .filter(|f| !frames.contains(f))
            .collect();
        let add: Vec<Mark> = (frames.iter().copied())
            .filter(|f| !now.contains(f))
            .map(|frame| Mark { frame, rate })
            .collect();
        let key = path.to_string_lossy();
        query::change_marks(&mut self.conn, &key, &remove, &add).map_err(|e| Notice::Failed {
            task: Task::Mark,
            error: e.to_string(),
        })?;
        self.marks.retain(|m| !remove.contains(&m.frame));
        self.marks.extend(add);
        self.marks.sort_by_key(|m| m.frame);
        self.relabel(&path);
        Ok(())
    }

    /// Looks for the onset nearest the mark at `from`, on a job, since it
    /// decodes. Finishes with [`Event::Snapped`].
    pub fn snap_mark(&mut self, from: u64, sensitivity: f32) -> Result<JobId, Refusal> {
        let (track, rate) = self.playing_track()?;
        Ok(self.spawn(move |job| {
            let result = crate::samples::nearest_onset(&track, rate, from, sensitivity);
            Some(Event::Snapped {
                job,
                track,
                from,
                result,
            })
        }))
    }

    /// Removes the mark added most recently to the playing track: marks are a
    /// chain, undone in the order they were made.
    pub fn undo_mark(&mut self) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_last_mark(&self.conn, &path.to_string_lossy()) {
            Ok(Some(mark)) => {
                self.marks.retain(|m| m.frame != mark.frame);
                self.relabel(&path);
                Outcome::MarkRemoved { at: mark.time() }.into()
            }
            Ok(None) => Refusal::NoMarks.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// The playing track and how many marks clearing it would remove, for a
    /// frontend to confirm before [`Session::clear_marks`].
    pub fn marks_to_clear(&mut self) -> Result<(PathBuf, usize), Refusal> {
        let (path, _) = self.playing_track()?;
        match self.marks_for(Some(&path)).len() {
            0 => Err(Refusal::NoMarks),
            n => Ok((path, n)),
        }
    }

    /// Clears the marks of the track at `path`, or does nothing if it has none.
    ///
    /// The path is the one the frontend asked about, since the playing track
    /// can change before the answer.
    pub fn clear_marks(&mut self, path: &Path) -> Option<Notice> {
        match query::clear_marks(&self.conn, &path.to_string_lossy()) {
            Ok(0) => None,
            Ok(_) => {
                if self.marks_for.as_deref() == Some(path) {
                    self.marks.clear();
                    self.labels.clear();
                }
                Some(Outcome::MarksCleared.into())
            }
            Err(e) => Some(Notice::Failed {
                task: Task::ClearMarks,
                error: e.to_string(),
            }),
        }
    }

    /// Seeks to the next mark, or back to the previous one.
    ///
    /// Back skips a mark less than [`MARK_BACK`] behind, as going to the
    /// previous track restarts one rather than leaving it, so stepping back
    /// twice passes two marks.
    pub fn seek_to_mark(&mut self, forward: bool) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        let at = self.player.position();
        let mut times = self.marks_for(Some(&path)).iter().map(Mark::time);
        let target = if forward {
            times.find(|t| *t > at + MARK_NEAR / 2)
        } else {
            times.rev().find(|t| *t + MARK_BACK < at)
        };
        match target {
            Some(t) => {
                self.player.send(Cmd::Seek(t));
                Outcome::AtMark { at: t }.into()
            }
            None if forward => Refusal::NoLaterMark.into(),
            None => Refusal::NoEarlierMark.into(),
        }
    }
}
