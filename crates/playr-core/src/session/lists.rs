//! The selection, and saving, renaming and deleting playlists.

use super::*;

impl Session {
    // --- selection ---

    /// Replaces the selection, as when files are given on the command line.
    pub fn set_selection(&mut self, tracks: Vec<Track>) {
        self.selection = tracks;
        self.selection_changed();
    }

    /// Selects `track`, or unselects every copy of it if it is selected.
    pub fn toggle_selected(&mut self, track: Track) -> Outcome {
        let outcome = if self.selection.iter().any(|t| t.path == track.path) {
            self.selection.retain(|t| t.path != track.path);
            Outcome::RemovedFromSelection
        } else {
            self.selection.push(track);
            Outcome::AddedToSelection
        };
        self.selection_changed();
        outcome
    }

    /// Selects `track`, unless it is selected already.
    pub fn add_to_selection(&mut self, track: Track) -> Outcome {
        if self.selection.iter().any(|t| t.path == track.path) {
            return Outcome::AlreadyInSelection;
        }
        self.selection.push(track);
        self.selection_changed();
        Outcome::AddedToSelection
    }

    /// Adds the tracks of playlist `id` that are not selected yet, or nothing
    /// if the playlist has no tracks. Repeats within the playlist stay, so a
    /// playlist that repeats a track on purpose keeps doing so.
    pub fn add_playlist_to_selection(&mut self, id: i64) -> Option<Outcome> {
        let added = self.playlist_tracks(id);
        self.add_all_to_selection(added)
    }

    /// Adds the `added` tracks not selected yet, or nothing if there are
    /// none; repeats within `added` stay.
    pub fn add_all_to_selection(&mut self, added: Vec<Track>) -> Option<Outcome> {
        if added.is_empty() {
            return None;
        }
        let selected: HashSet<&str> = self.selection.iter().map(|t| t.path.as_str()).collect();
        let new: Vec<Track> = added
            .into_iter()
            .filter(|t| !selected.contains(t.path.as_str()))
            .collect();
        if new.is_empty() {
            return Some(Outcome::AlreadyInSelection);
        }
        self.selection.extend(new);
        self.selection_changed();
        Some(Outcome::AddedToSelection)
    }

    /// Selects `listed`, or unselects every copy of each when all are
    /// selected already. Nothing for an empty list.
    pub fn toggle_all_selected(&mut self, listed: Vec<Track>) -> Option<Outcome> {
        let selected: HashSet<&str> = self.selection.iter().map(|t| t.path.as_str()).collect();
        if !listed.iter().all(|t| selected.contains(t.path.as_str())) {
            return self.add_all_to_selection(listed);
        }
        if listed.is_empty() {
            return None;
        }
        let gone: HashSet<&str> = listed.iter().map(|t| t.path.as_str()).collect();
        self.selection.retain(|t| !gone.contains(t.path.as_str()));
        self.selection_changed();
        Some(Outcome::RemovedFromSelection)
    }

    /// Removes the track at `index`, or nothing if there is none.
    pub fn remove_from_selection(&mut self, index: usize) -> Option<Outcome> {
        let track = (index < self.selection.len()).then(|| self.selection.remove(index))?;
        self.selection_changed();
        Some(Outcome::RemovedTrack {
            title: track.display_title(),
        })
    }

    /// Moves the track at `index` by `by` places, returning where it is now,
    /// or nothing if either place is outside the selection.
    pub fn move_in_selection(&mut self, index: usize, by: i64) -> Option<usize> {
        let to = usize::try_from((index as i64).checked_add(by)?).ok()?;
        if index >= self.selection.len() || to >= self.selection.len() {
            return None;
        }
        // Moved, not swapped: the tracks between keep their order.
        let track = self.selection.remove(index);
        self.selection.insert(to, track);
        self.selection_changed();
        Some(to)
    }

    pub fn clear_selection(&mut self) -> Outcome {
        self.selection.clear();
        if self.editing.is_some() {
            self.set_editing(None);
        }
        self.selection_changed();
        Outcome::SelectionCleared
    }

    // --- playlists ---

    /// Whether the selection can be saved as a playlist at all.
    pub fn check_save(&self) -> Result<(), Refusal> {
        if self.selection.is_empty() {
            Err(Refusal::SelectionEmpty)
        } else if !self.has_library_file() {
            // In memory, the playlist would be lost on exit.
            Err(Refusal::NoLibraryFile)
        } else {
            Ok(())
        }
    }

    /// Saves the selection as the playlist `name`. Replacing a playlist of that
    /// name is refused with [`Refusal::WouldReplace`] unless `replace` is set,
    /// so a frontend can ask first.
    pub fn save_selection(&mut self, name: &str, replace: bool) -> Notice {
        // Saving an edit over its playlist is what the edit was for.
        let edited = self.editing().is_some_and(|p| p.name == name.trim());
        let tracks = self.selection.clone();
        let notice = self.save_tracks(name, &tracks, replace || edited);
        if matches!(notice, Notice::Done(_)) && self.editing.is_some() {
            self.set_editing(None);
        }
        notice
    }

    /// As [`check_save`](Self::check_save), for the queue.
    pub fn check_save_queue(&self) -> Result<(), Refusal> {
        if self.played.is_empty() && self.queue_rows().is_empty() {
            Err(Refusal::QueueEmpty)
        } else if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else {
            Ok(())
        }
    }

    /// As [`save_selection`](Self::save_selection), for every row of the
    /// Queue view: played, playing and waiting.
    pub fn save_queue(&mut self, name: &str, replace: bool) -> Notice {
        let tracks = self.queue_tracks();
        self.save_tracks(name, &tracks, replace)
    }

    pub(super) fn save_tracks(&mut self, name: &str, tracks: &[Track], replace: bool) -> Notice {
        let name = name.trim();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if reserved(name) {
            return Refusal::NameReserved(name.to_string()).into();
        }
        if self.searches.iter().any(|s| s.name == name) {
            return Refusal::NameTaken(name.to_string()).into();
        }
        if !replace && self.playlists.iter().any(|p| p.name == name) {
            return Refusal::WouldReplace(name.to_string()).into();
        }
        // A playlist holds only library tracks; files given on the command line
        // are selected without being in the library.
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).filter(|id| *id != 0).collect();
        match query::save_playlist(&mut self.conn, name, &ids) {
            Ok(_) => {
                self.playlists = query::playlists(&self.conn).unwrap_or_default();
                Outcome::Saved {
                    name: name.to_string(),
                    tracks: ids.len(),
                    left_out: tracks.len() - ids.len(),
                }
                .into()
            }
            Err(e) => Notice::Failed {
                task: Task::Save,
                error: e.to_string(),
            },
        }
    }

    /// Renames playlist `id` to `name`, unless the name is empty, unchanged
    /// or taken.
    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Notice {
        let name = name.trim();
        let from = self.playlist_name(id).unwrap_or_default();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if name == from {
            return Refusal::NameUnchanged.into();
        }
        if reserved(name) {
            return Refusal::NameReserved(name.to_string()).into();
        }
        if self.name_taken(name) {
            // Renaming onto it would have to merge or replace two playlists.
            return Refusal::NameTaken(name.to_string()).into();
        }
        if let Err(e) = query::rename_playlist(&self.conn, id, name) {
            return Notice::Failed {
                task: Task::Rename,
                error: e.to_string(),
            };
        }
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Outcome::Renamed {
            from,
            to: name.to_string(),
        }
        .into()
    }

    /// Deletes playlist `id`. Returns nothing if the library refuses.
    pub fn delete_playlist(&mut self, id: i64) -> Option<Notice> {
        let name = self.playlist_name(id)?;
        query::delete_playlist(&self.conn, id).ok()?;
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Some(Outcome::Deleted { name }.into())
    }

    pub(super) fn playlist_name(&self, id: i64) -> Option<String> {
        self.playlists
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
    }
}
