//! The player's list, the queue, the draft and the state kept between sessions.

use super::*;

impl Session {
    // --- playback ---

    // The player holds one list: the list played, with the tracks the
    // listener queued spliced in after the track playing. `Status::queued`
    // tells them apart. A queued track leaves the list once it has played.

    /// Plays `tracks` from `index`, as a list that goes on by itself, as the
    /// library does. Tracks waiting in the queue are kept and play next.
    pub fn play(&mut self, tracks: &[Track], index: usize) {
        let status = self.player.status();
        let waiting: Vec<PathBuf> = waiting(&status).map(|i| status.queue[i].clone()).collect();
        let index = index.min(tracks.len().saturating_sub(1));
        let paths = |ts: &[Track]| {
            ts.iter()
                .map(|t| PathBuf::from(&t.path))
                .collect::<Vec<_>>()
        };
        let (head, tail) = tracks.split_at((index + 1).min(tracks.len()));
        let mut list = paths(head);
        let mut queued = vec![false; list.len()];
        queued.extend(vec![true; waiting.len()]);
        list.extend(waiting);
        queued.extend(vec![false; tail.len()]);
        list.extend(paths(tail));
        self.player.send(Cmd::PlayWith(list, queued, index));
    }

    /// Replaces the queue with `tracks` from `index` and plays them, over the
    /// list playing, which resumes after them. Says how many tracks were
    /// waiting, when some were.
    pub fn play_queued(&mut self, tracks: &[Track], index: usize) -> Option<Outcome> {
        let status = self.player.status();
        let dropped = waiting(&status).count();
        let new: Vec<PathBuf> = tracks
            .iter()
            .skip(index)
            .map(|t| PathBuf::from(&t.path))
            .collect();
        let listed = |i: &usize| !status.queued.get(*i).copied().unwrap_or(false);
        let played = |range: std::ops::Range<usize>| -> Vec<PathBuf> {
            range
                .filter(listed)
                .map(|i| status.queue[i].clone())
                .collect()
        };
        let len = status.queue.len();
        let (before, after) = match len {
            0 => (Vec::new(), Vec::new()),
            _ => (played(0..status.index + 1), played(status.index + 1..len)),
        };
        let start = before.len();
        let mut queued = vec![false; before.len()];
        queued.extend(vec![true; new.len()]);
        queued.extend(vec![false; after.len()]);
        let list: Vec<PathBuf> = before.into_iter().chain(new).chain(after).collect();
        self.player.send(Cmd::PlayWith(list, queued, start));
        (dropped > 0).then_some(Outcome::QueueReplaced { tracks: dropped })
    }

    /// Queues `tracks`: after the tracks waiting, or before them when `next`.
    /// The first tracks queued over a list playing interrupt it; with
    /// nothing playing, they play at once.
    pub fn enqueue(&mut self, tracks: &[Track], next: bool) -> Outcome {
        let outcome = Outcome::Queued {
            tracks: tracks.len(),
            next,
        };
        let status = self.player.status();
        if self.stopped(&status) || status.queue.is_empty() {
            let _ = self.play_queued(tracks, 0);
            return outcome;
        }
        let last_waiting = waiting(&status).last();
        let playing_queued = status.queued.get(status.index).copied().unwrap_or(false);
        let at = match (next, last_waiting) {
            (false, Some(last)) => last + 1,
            _ => status.index + 1,
        };
        let paths = tracks.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Insert(at, paths));
        if !next && last_waiting.is_none() && !playing_queued {
            self.player.send(Cmd::Next);
        }
        outcome
    }

    /// The rows of the queue, as indices into the player's list: the track
    /// playing, if it was queued, then the queued tracks waiting.
    pub fn queue_rows(&self) -> Vec<usize> {
        let status = self.player.status();
        let playing =
            !self.stopped(&status) && status.queued.get(status.index).copied().unwrap_or(false);
        playing
            .then_some(status.index)
            .into_iter()
            .chain(waiting(&status))
            .collect()
    }

    /// Queued tracks that have played, oldest first: the Queue view's first
    /// rows, above [`queue_rows`](Self::queue_rows).
    pub fn played(&self) -> &[Track] {
        &self.played
    }

    /// Every row of the Queue view, played first, for saving as a playlist.
    pub fn queue_tracks(&self) -> Vec<Track> {
        let list = self.player.queue();
        let live = self
            .queue_rows()
            .into_iter()
            .map(|i| self.track_for(&list[i]));
        self.played.iter().cloned().chain(live).collect()
    }

    /// The library's track at `path`, or a bare one for a file outside it.
    pub(super) fn track_for(&self, path: &Path) -> Track {
        match self.tracks.iter().find(|t| Path::new(&t.path) == path) {
            Some(t) => t.clone(),
            None => Track {
                path: path.to_string_lossy().into_owned(),
                ..Default::default()
            },
        }
    }

    /// Takes Queue view row `row` out, and says which track it was. Taking
    /// out the track playing plays the next.
    pub fn dequeue(&mut self, row: usize) -> Option<Outcome> {
        if row < self.played.len() {
            let title = self.played.remove(row).display_title();
            return Some(Outcome::RemovedTrack { title });
        }
        let index = *self.queue_rows().get(row - self.played.len())?;
        let path = self.player.queue().get(index)?.clone();
        let title = self.track_for(&path).display_title();
        self.player.send(Cmd::Remove(index));
        Some(Outcome::RemovedTrack { title })
    }

    /// Moves Queue view row `row` by `by` places, returning where it is now,
    /// or nothing if the move leaves the played rows or the waiting rows it
    /// started in. The track playing keeps its place.
    pub fn move_in_queue(&mut self, row: usize, by: i64) -> Option<usize> {
        let to = usize::try_from((row as i64).checked_add(by)?).ok()?;
        let h = self.played.len();
        if row < h {
            if to >= h {
                return None;
            }
            let track = self.played.remove(row);
            self.played.insert(to, track);
            return Some(to);
        }
        let (row, to) = (row - h, to.checked_sub(h)?);
        let rows = self.queue_rows();
        let status = self.player.status();
        let stopped = self.stopped(&status);
        let first = rows.iter().position(|&i| i > status.index || stopped)?;
        if row < first || to < first || row >= rows.len() || to >= rows.len() {
            return None;
        }
        self.player.send(Cmd::Move(rows[row], rows[to]));
        Some(to + h)
    }

    /// Plays Queue view row `row`: a waiting track at once, or a played one
    /// again, which then leaves the played rows until it has played.
    pub fn play_queue_row(&mut self, row: usize) {
        let h = self.played.len();
        if row >= h {
            if let Some(&index) = self.queue_rows().get(row - h) {
                self.player.send(Cmd::Jump(index));
            }
            return;
        }
        let path = PathBuf::from(&self.played.remove(row).path);
        let status = self.player.status();
        match status.queue.is_empty() {
            true => self.player.send(Cmd::PlayWith(vec![path], vec![true], 0)),
            false => {
                self.player.send(Cmd::Insert(status.index + 1, vec![path]));
                self.player.send(Cmd::Jump(status.index + 1));
            }
        }
    }

    /// Empties the queue: the tracks waiting and those played go, and the
    /// track playing plays on as part of the list, no longer queued.
    pub fn clear_queue(&mut self) -> Outcome {
        let rows = self.queue_rows();
        let status = self.player.status();
        let waiting: Vec<usize> = waiting(&status).collect();
        for &i in waiting.iter().rev() {
            self.player.send(Cmd::Remove(i));
        }
        // Not taken out, which would cut it off: it plays on as part of the
        // list, and leaves the queue's rows.
        let playing = rows.first().filter(|&&i| i == status.index);
        if let Some(&i) = playing {
            self.player.send(Cmd::Unqueue(i));
        }
        let played = std::mem::take(&mut self.played).len();
        Outcome::QueueCleared {
            tracks: waiting.len() + played + usize::from(playing.is_some()),
        }
    }

    /// Whether playback is stopped, and not about to start: right after a
    /// play is sent the engine has yet to leave the stopped state.
    pub(super) fn stopped(&self, status: &crate::audio::Status) -> bool {
        status.state == State::Stopped && self.player.caught_up()
    }

    /// Moves the queued tracks that have played out of the list and into
    /// [`played`](Self::played), once playback has moved past them.
    pub fn drop_played(&mut self) {
        let status = self.player.status();
        let played: Vec<usize> = (0..status.index.min(status.queued.len()))
            .filter(|&i| status.queued[i])
            .collect();
        for &i in &played {
            let track = self.track_for(&status.queue[i]);
            self.played.push(track);
        }
        for &i in played.iter().rev() {
            self.player.send(Cmd::Remove(i));
        }
    }

    /// Plays `path` from `at`, then the waiting tracks of `queue`, without
    /// touching the selection. For taking up where playr left off; the list
    /// that was playing is not kept, so playback stops after the queue.
    pub fn resume(&mut self, path: PathBuf, at: Duration, queue: SavedQueue) {
        self.played = queue.played.iter().map(|p| self.track_for(p)).collect();
        let mut list = vec![path];
        list.extend(queue.waiting);
        let mut queued = vec![true; list.len()];
        queued[0] = queue.playing;
        self.player.send(Cmd::PlayWith(list, queued, 0));
        self.player.send(Cmd::Seek(at));
    }

    /// What playr was playing when it last closed, if the file is still
    /// there, with the queue stored beside it, less files that have gone.
    ///
    /// A file that has gone is not offered: the question would be about a
    /// track that cannot play, and answering yes would do nothing.
    pub fn resumable(&self) -> Option<(PathBuf, Duration, SavedQueue)> {
        let (path, at) = db::resume(&self.conn).ok().flatten()?;
        let mut queue = match self.keep_queue {
            true => db::resume_queue(&self.conn).unwrap_or_default(),
            false => SavedQueue::default(),
        };
        queue.played.retain(|p| p.is_file());
        queue.waiting.retain(|p| p.is_file());
        path.is_file().then_some((path, at, queue))
    }

    /// Remembers `path` and `at` as where to take up next time, with the queue.
    /// Records a play of `path` in the library, and in the rows that show it.
    pub fn record_play(&mut self, path: &Path) {
        let (path, at) = (path.to_string_lossy(), db::now());
        if db::record_play(&self.conn, &path, at).is_err() {
            return;
        }
        let rows = self.tracks.iter_mut();
        for t in rows.chain(&mut self.selection).chain(&mut self.played) {
            if t.path == path {
                t.played_at = Some(at);
                t.plays += 1;
            }
        }
        self.revision += 1;
    }

    pub fn remember(&self, path: &Path, at: Duration) {
        let _ = db::set_resume(&self.conn, path, at);
        self.remember_queue();
    }

    /// Chooses whether the queue is stored for the next start. It is unless
    /// this says not; a queue stored before is then forgotten.
    pub fn keep_queue(&mut self, keep: bool) {
        self.keep_queue = keep;
        if !keep {
            let _ = db::set_resume_queue(&self.conn, &SavedQueue::default(), Path::new(""));
        }
    }

    // The selection starts empty each session and is written, as it changes,
    // to the playlist `DRAFT`. A draft an earlier session left is settled, as
    // `draft` says, before the first write replaces it.

    /// The playlist the selection holds to edit, if it still exists.
    pub fn editing(&self) -> Option<&Playlist> {
        let id = self.editing?;
        self.playlists.iter().find(|p| p.id == id)
    }

    /// Records the playlist being edited, in the library too, so an edit the
    /// draft holds can go on in a later session.
    pub(super) fn set_editing(&mut self, id: Option<i64>) {
        self.editing = id;
        self.set_state(EDITING, &id.map_or(String::new(), |id| id.to_string()));
    }

    /// Replaces the selection with playlist `id`'s tracks to edit them;
    /// saving under its name then replaces it without asking. The draft
    /// itself is taken up as an old draft appended; nothing, once settled.
    pub fn edit_playlist(&mut self, id: i64) -> Option<Outcome> {
        let name = self.playlist_name(id)?;
        if name == DRAFT {
            // Settled, the draft already is the selection.
            if self.draft_settled {
                return None;
            }
            self.selection.clear();
            self.settle_draft(DraftChoice::Append);
            return Some(Outcome::DraftAppended);
        }
        self.selection = self.playlist_tracks(id);
        self.set_editing(Some(id));
        self.selection_changed();
        Some(Outcome::Editing { name })
    }

    /// Chooses what happens to a draft an earlier session left.
    pub fn set_draft(&mut self, draft: Draft) {
        self.draft = draft;
    }

    /// The draft playlist, if there is one.
    pub(super) fn draft_playlist(&self) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.name == DRAFT)
    }

    /// Writes the selection as the draft once the old draft is settled, or
    /// settles it first as `draft` says, or waits on the listener's answer.
    pub(super) fn selection_changed(&mut self) {
        if self.draft == Draft::Off || !self.has_library_file() {
            return;
        }
        if !self.draft_settled {
            let old = self.draft_playlist().map_or(0, |p| p.len);
            match (old, self.draft) {
                (0, _) | (_, Draft::Overwrite) => self.set_editing(self.editing),
                (_, Draft::Append) => self.append_old_draft(),
                _ => {
                    self.draft_question = true;
                    return;
                }
            }
            self.draft_settled = true;
        }
        self.write_draft();
    }

    /// Puts the old draft's tracks before the selection's, leaving out
    /// those the selection already holds. An edit the old draft held goes
    /// on, unless one has begun since.
    pub(super) fn append_old_draft(&mut self) {
        let Some(id) = self.draft_playlist().map(|p| p.id) else {
            return;
        };
        let editing = self.state(EDITING).and_then(|id| id.parse().ok());
        self.set_editing(self.editing.or(editing));
        let new = std::mem::take(&mut self.selection);
        self.selection = self.playlist_tracks(id);
        let old: HashSet<String> = self.selection.iter().map(|t| t.path.clone()).collect();
        self.selection
            .extend(new.into_iter().filter(|t| !old.contains(&t.path)));
    }

    /// Makes the draft playlist the selection's library tracks, or removes it
    /// when there are none.
    pub(super) fn write_draft(&mut self) {
        let ids: Vec<i64> = (self.selection.iter().map(|t| t.id))
            .filter(|id| *id != 0)
            .collect();
        let _ = match (ids.is_empty(), self.draft_playlist().map(|p| p.id)) {
            (true, Some(id)) => query::delete_playlist(&self.conn, id),
            (true, None) => Ok(()),
            (false, _) => query::save_playlist(&mut self.conn, DRAFT, &ids).map(|_| ()),
        };
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
    }

    /// How many tracks the old draft holds, once, when a change waits on the
    /// listener: they answer with [`settle_draft`](Self::settle_draft).
    /// Unanswered, the question comes again with the next change.
    pub fn take_draft_question(&mut self) -> Option<usize> {
        if !std::mem::take(&mut self.draft_question) || self.draft_settled {
            return None;
        }
        self.draft_playlist().map(|p| p.len as usize)
    }

    /// Settles the old draft as the listener chose, then writes the selection
    /// as the draft. Saving under a name that is refused leaves it unsettled,
    /// and asks again.
    pub fn settle_draft(&mut self, choice: DraftChoice) -> Notice {
        let notice = match choice {
            DraftChoice::Overwrite => {
                self.set_editing(self.editing);
                Outcome::DraftOverwritten.into()
            }
            DraftChoice::Append => {
                self.append_old_draft();
                Outcome::DraftAppended.into()
            }
            DraftChoice::SaveAs(name) => {
                let Some(id) = self.draft_playlist().map(|p| p.id) else {
                    return Refusal::NoPlaylistNamed(DRAFT.into()).into();
                };
                let notice = self.rename_playlist(id, &name);
                if !matches!(notice, Notice::Done(_)) {
                    self.draft_question = true;
                    return notice;
                }
                self.set_editing(self.editing);
                notice
            }
        };
        self.draft_settled = true;
        self.write_draft();
        notice
    }

    /// Remembers the queue, to offer with the track playing next time, if
    /// it is kept.
    pub fn remember_queue(&self) {
        if !self.keep_queue {
            return;
        }
        let status = self.player.status();
        let rows = self.queue_rows();
        let playing = rows.first() == Some(&status.index) && !self.stopped(&status);
        let queue = SavedQueue {
            played: self.played.iter().map(|t| PathBuf::from(&t.path)).collect(),
            playing,
            waiting: rows[usize::from(playing)..]
                .iter()
                .map(|&i| status.queue[i].clone())
                .collect(),
        };
        let current = status.current().map_or(Path::new(""), |p| p.as_path());
        let _ = db::set_resume_queue(&self.conn, &queue, current);
    }

    /// The session value remembered under `key`; none without a library file.
    pub fn state(&self, key: &str) -> Option<String> {
        db::state(&self.conn, key).ok().flatten()
    }

    /// Remembers a session value under `key`, if there is a library file.
    pub fn set_state(&self, key: &str, value: &str) {
        if self.has_library_file() {
            let _ = db::set_state(&self.conn, key, value);
        }
    }

    /// Forgets where to take up, after a refused offer or a stop.
    pub fn forget_resume(&self) {
        let _ = db::clear_resume(&self.conn);
    }

    /// Sets the sleep timer to stop playback `after` from now, or turns it
    /// off with `None`.
    pub fn sleep_in(&mut self, after: Option<Duration>) {
        self.sleep_at = after.and_then(|d| Instant::now().checked_add(d));
    }

    /// Time left on the sleep timer, if set.
    pub fn sleep_left(&self) -> Option<Duration> {
        self.sleep_at
            .map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// Stops playback once the sleep timer has run out; says whether it did.
    pub fn sleep_due(&mut self) -> bool {
        if self.sleep_left() != Some(Duration::ZERO) {
            return false;
        }
        self.sleep_at = None;
        self.player.send(Cmd::Stop);
        true
    }

    /// Sends a playback command: pause, next, seek, speed and so on.
    pub fn send(&self, cmd: Cmd) {
        self.player.send(cmd);
    }

    /// Changes the volume by `delta`, from 0 to 1.
    pub fn volume_by(&self, delta: f32) {
        // The player's own value, not a frame-old copy, so quick presses all count.
        let v = (self.player.volume() + delta).clamp(0.0, 1.0);
        self.player.send(Cmd::SetVolume(v));
    }

    /// Which ReplayGain applies, from the audible position on.
    pub fn set_replaygain(&mut self, replaygain: ReplayGain) -> Notice {
        let loading = self.replaygain == ReplayGain::Off;
        self.replaygain = replaygain;
        if loading {
            self.send_gains();
        }
        self.player.send(Cmd::SetReplayGain(replaygain));
        Outcome::ReplayGain(replaygain).into()
    }

    pub fn replaygain(&self) -> ReplayGain {
        self.replaygain
    }

    pub fn set_mode(&self, mode: Mode) -> Notice {
        self.player.send(Cmd::SetMode(mode));
        Outcome::Mode(mode).into()
    }

    /// Moves to the next playback mode, or the previous one.
    pub fn cycle_mode(&self, forward: bool) -> Notice {
        let current = self.player.mode();
        self.set_mode(if forward {
            current.next()
        } else {
            current.prev()
        })
    }

    /// Plays playlist `id` from its first track.
    pub fn play_playlist(&mut self, id: i64) -> Notice {
        let Some(name) = self.playlist_name(id) else {
            return Refusal::PlaylistEmpty.into();
        };
        let tracks = self.playlist_tracks(id);
        if tracks.is_empty() {
            return Refusal::PlaylistEmpty.into();
        }
        match self.play_queued(&tracks, 0) {
            Some(replaced) => replaced.into(),
            None => Outcome::PlayingPlaylist { name }.into(),
        }
    }

    /// Plays the one playlist named `name`, matched as `playr playlist` does.
    pub fn play_playlist_named(&mut self, name: &str) -> Notice {
        let name = name.trim();
        match query::find_playlist(&self.playlists, name) {
            Some(pl) => self.play_playlist(pl.id),
            None => Refusal::NoPlaylistNamed(name.to_string()).into(),
        }
    }

    /// The playing track's path and source rate, read from the player now.
    pub fn playing_track(&self) -> Result<(PathBuf, u32), Refusal> {
        let status = self.player.status();
        match (status.state, status.current(), status.source) {
            (State::Playing | State::Paused, Some(path), Some(source)) => {
                Ok((path.clone(), source.rate))
            }
            _ => Err(Refusal::NothingPlaying),
        }
    }
}
