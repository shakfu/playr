//! Actions on the views' rows: the library, searches, playlists, loops, the cursor, playing and queueing.

use super::*;

/// Shows the library view, unless the DJ view, which lists it too, is showing.
pub(super) fn show_library(f: &mut impl Frontend) {
    if f.view() != View::Dj {
        f.set_view(View::Library);
    }
}

/// Shows the tracks matching `query` in the library view, or the whole library
/// for an empty query, with the cursor on the first.
pub fn search(f: &mut impl Frontend, query: &str) {
    show_library(f);
    let results = (!query.is_empty()).then(|| f.session().search(query));
    let shown = (!query.is_empty()).then(|| Query::Text(query.to_string()));
    f.session_mut().set_shown(shown);
    f.set_results(results);
    let row = (!f.listed().is_empty()).then_some(0);
    f.set_cursor(View::Library, row);
}

/// Shows what `search` finds now in the library view, with the cursor on
/// the first.
pub(super) fn show_search(f: &mut impl Frontend, search: &SavedSearch) {
    if let Query::Sql(statement) = &search.query {
        return run_sql(f, statement.clone(), SqlThen::Show(statement.clone()));
    }
    show_library(f);
    let results = f.session().run_search(search);
    let empty = results.is_empty();
    f.session_mut().set_shown(Some(search.query.clone()));
    f.set_results(Some(results));
    f.set_cursor(View::Library, (!empty).then_some(0));
    if empty {
        f.notify(Message::NoMatches);
    }
}

/// The saved search under the playlists view's cursor. They are listed after
/// the playlists.
pub(super) fn search_under_cursor(f: &impl Frontend) -> Option<SavedSearch> {
    if f.view() != View::Playlists {
        return None;
    }
    let row = f.cursor(View::Playlists)?;
    let i = row.checked_sub(f.session().playlists().len())?;
    f.session().searches().get(i).cloned()
}

/// Puts `playlist`'s tracks in the selection to edit, and shows them.
pub(super) fn edit(f: &mut impl Frontend, playlist: &Playlist) {
    let Some(outcome) = f.session_mut().edit_playlist(playlist.id) else {
        return;
    };
    f.set_view(View::Selection);
    let row = (!f.session().selection().is_empty()).then_some(0);
    f.set_cursor(View::Selection, row);
    f.notify(outcome.into());
}

/// Whether `:save` can save: the queue in its view, else the selection.
pub(super) fn check_save(f: &impl Frontend) -> Result<(), Refusal> {
    match f.view() {
        View::Queue => f.session().check_save_queue(),
        _ => f.session().check_save(),
    }
}

/// Saves the queue in its view, else the selection, as `name`, asking first
/// if that replaces a playlist.
pub fn save_as(f: &mut impl Frontend, name: &str) {
    let queue = f.view() == View::Queue;
    let notice = match queue {
        true => f.session_mut().save_queue(name, false),
        false => f.session_mut().save_selection(name, false),
    };
    match notice {
        Notice::Refused(Refusal::WouldReplace(name)) => {
            f.confirm(Confirm::ReplacePlaylist { name, queue })
        }
        notice => f.notify(notice.into()),
    }
}

/// Renames `from` to `name`; the playlists cursor follows it to its new place.
pub fn rename(f: &mut impl Frontend, from: &Playlist, name: &str) {
    let notice = f.session_mut().rename_playlist(from.id, name);
    if matches!(notice, Notice::Done(_)) {
        // The list is sorted by name, so the renamed playlist may have moved.
        let at = f.session().playlists().iter().position(|p| p.id == from.id);
        f.set_cursor(View::Playlists, at);
    }
    f.notify(notice.into());
}

/// The number of rows `view` lists.
pub(super) fn len(f: &impl Frontend, view: View) -> usize {
    match view {
        View::Library | View::Dj => f.listed().len(),
        View::Selection => f.session().selection().len(),
        View::Playlists => f.session().playlists().len() + f.session().searches().len(),
        View::Queue => f.session().played().len() + f.session().queue_rows().len(),
        View::Loops => f.session().loop_list().len(),
        View::Sampler | View::Tape | View::Mix => 0,
    }
}

/// The Loops view's cursor and the loop under it.
pub(super) fn loop_under_cursor(f: &impl Frontend) -> Option<(usize, playr_core::db::loops::Loop)> {
    let i = f.cursor(View::Loops)?;
    Some((i, f.session().loop_list().get(i)?.clone()))
}

/// Does `action` to the loop under the Loops view's cursor.
pub(super) fn loop_lib(f: &mut impl Frontend, action: LoopAction) {
    let Some((_, l)) = loop_under_cursor(f) else {
        return f.notify(Message::NoLoopUnderCursor);
    };
    let edited = match action {
        LoopAction::Load(voice) => return crate::tape::load_loop(f, l.id, voice),
        LoopAction::Beats(n) => f.session_mut().set_loop_beats(l.id, n),
        LoopAction::Key(k) => f.session_mut().set_loop_key(l.id, k.as_deref()),
    };
    match edited {
        Ok(l) => f.notify(Message::LoopSet(Box::new(l))),
        Err(e) => f.notify(Message::LoopFailed(e)),
    }
}

/// Sets each band to its gain in dB, and says what the tone control is now.
/// Sets the speed that plays the playing track at `bpm`, to the nearest cent,
/// from its analysed tempo.
pub(super) fn tempo(f: &mut impl Frontend, bpm: f32) {
    let path = match f.session().playing_track() {
        Ok((path, _)) => path,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let Some(found) = f.session().bpm(&path) else {
        return f.notify(Refusal::NoTempo.into());
    };
    let cents = (1200.0 * f64::from(bpm / found).log2()).round();
    if cents.abs() > f64::from(playr_core::audio::MAX_CENTS) {
        return f.notify(Message::TempoOutOfReach { bpm, found });
    }
    f.session().send(Cmd::SetCents(cents as i32));
}

/// Sets `bands`, which ends a bypass.
pub(super) fn set_eq(f: &mut impl Frontend, bands: &[(Band, f32)]) {
    f.mix().hold_eq(None);
    for &(band, db) in bands {
        f.session().send(Cmd::SetEq(band, db));
    }
    let gains = f.session().player().eq();
    f.notify(Message::Eq(gains));
}

/// Flattens the EQ and holds its bands, or puts back the bands held. A flat
/// EQ with none held stays flat.
pub(super) fn bypass_eq(f: &mut impl Frontend) {
    let now = f.session().player().eq();
    let flat = now == [0.0; 3];
    match f.mix().eq_held() {
        Some(held) if flat => set_eq(f, &Band::ALL.map(|b| (b, held[b as usize]))),
        _ if flat => set_eq(f, &[]),
        _ => {
            for band in Band::ALL {
                f.session().send(Cmd::SetEq(band, 0.0));
            }
            f.mix().hold_eq(Some(now));
            f.notify(Message::EqBypassed(now));
        }
    }
}

/// Puts the current view's cursor on row `i`, or its last row.
pub(super) fn select(f: &mut impl Frontend, i: usize) {
    let view = f.view();
    if view == View::Sampler {
        return;
    }
    let row = match len(f, view) {
        0 => None,
        n => Some(i.min(n - 1)),
    };
    f.set_cursor(view, row);
}

/// Moves the current view's cursor `rows` rows, stopping at either end.
pub(super) fn move_cursor(f: &mut impl Frontend, rows: i64) {
    let view = f.view();
    let n = len(f, view);
    if n == 0 {
        return;
    }
    let at = f.cursor(view).unwrap_or(0) as i64;
    f.set_cursor(
        view,
        Some(at.saturating_add(rows).clamp(0, n as i64 - 1) as usize),
    );
}

/// The playlist under the cursor, when the playlists view is showing.
pub(super) fn playlist_under_cursor(f: &impl Frontend) -> Option<Playlist> {
    if f.view() != View::Playlists {
        return None;
    }
    let row = f.cursor(View::Playlists)?;
    f.session().playlists().get(row).cloned()
}

/// Plays the list in view from the cursor, or the playlist under it.
/// The library view's track under the cursor, for keeping it there.
pub(super) fn library_track(f: &impl Frontend) -> Option<Track> {
    f.listed().get(f.cursor(View::Library)?).cloned()
}

/// The track under the cursor in the library or the selection.
pub(crate) fn cursor_track(f: &impl Frontend) -> Option<Track> {
    match f.view() {
        View::Library | View::Dj => library_track(f),
        View::Selection => f
            .session()
            .selection()
            .get(f.cursor(View::Selection)?)
            .cloned(),
        _ => None,
    }
}

pub(super) fn activate(f: &mut impl Frontend) {
    match f.view() {
        View::Library | View::Dj => {
            let Some(i) = f.cursor(View::Library) else {
                return;
            };
            let tracks = f.listed().to_vec();
            if tracks.is_empty() {
                return;
            }
            // Search results are a list chosen, so they fill the queue; the
            // library plays on by itself.
            match f.searching() {
                true => {
                    if let Some(replaced) = f.session_mut().play_queued(&tracks, i) {
                        f.notify(replaced.into());
                    }
                }
                false => f.session_mut().play(&tracks, i),
            }
        }
        View::Selection => {
            if let Some(i) = f.cursor(View::Selection) {
                let tracks = f.session().selection().to_vec();
                if let Some(replaced) = f.session_mut().play_queued(&tracks, i) {
                    f.notify(replaced.into());
                }
            }
        }
        View::Playlists => {
            if let Some(pl) = playlist_under_cursor(f) {
                let notice = f.session_mut().play_playlist(pl.id);
                f.notify(notice.into());
            } else if let Some(search) = search_under_cursor(f) {
                show_search(f, &search);
            }
        }
        View::Queue => {
            if let Some(row) = f.cursor(View::Queue) {
                f.session_mut().play_queue_row(row);
            }
        }
        // Loops load from the Tape tab's lanes, or with :load.
        View::Loops | View::Sampler | View::Tape | View::Mix => {}
    }
}

/// Queues the track under the cursor, or the playlist's tracks, and moves
/// the cursor on a row, so a run of tracks takes one key each.
pub(super) fn enqueue(f: &mut impl Frontend, next: bool) {
    let tracks: Vec<Track> = match f.view() {
        View::Library | View::Dj => library_track(f).into_iter().collect(),
        View::Selection => f
            .cursor(View::Selection)
            .and_then(|i| f.session().selection().get(i).cloned())
            .into_iter()
            .collect(),
        View::Playlists => match (playlist_under_cursor(f), search_under_cursor(f)) {
            (Some(pl), _) => f.session().playlist_tracks(pl.id),
            (
                None,
                Some(SavedSearch {
                    query: Query::Sql(statement),
                    ..
                }),
            ) => {
                move_cursor(f, 1);
                return run_sql(f, statement, SqlThen::Enqueue(next));
            }
            (None, Some(search)) => f.session().run_search(&search),
            (None, None) => Vec::new(),
        },
        View::Sampler | View::Queue | View::Tape | View::Mix | View::Loops => Vec::new(),
    };
    if tracks.is_empty() {
        return;
    }
    let outcome = f.session_mut().enqueue(&tracks, next);
    move_cursor(f, 1);
    f.notify(outcome.into());
}

/// In the library, selects the track under the cursor or unselects it; on a
/// playlist, adds its tracks. Either way the cursor moves on a row, so a run
/// of tracks takes one key each.
pub(super) fn add(f: &mut impl Frontend) {
    let outcome = match f.view() {
        View::Library | View::Dj => {
            let Some(i) = f.cursor(View::Library) else {
                return;
            };
            let Some(track) = f.listed().get(i).cloned() else {
                return;
            };
            f.session_mut().toggle_selected(track)
        }
        View::Playlists => {
            let added = match (playlist_under_cursor(f), search_under_cursor(f)) {
                (Some(pl), _) => f.session_mut().add_playlist_to_selection(pl.id),
                (
                    None,
                    Some(SavedSearch {
                        query: Query::Sql(statement),
                        ..
                    }),
                ) => {
                    move_cursor(f, 1);
                    return run_sql(f, statement, SqlThen::Select);
                }
                (None, Some(search)) => {
                    let found = f.session().run_search(&search);
                    f.session_mut().add_all_to_selection(found)
                }
                (None, None) => None,
            };
            let Some(outcome) = added else {
                return;
            };
            outcome
        }
        View::Queue => {
            let Some(i) = f.cursor(View::Queue) else {
                return;
            };
            let Some(track) = f.session().queue_tracks().get(i).cloned() else {
                return;
            };
            f.session_mut().add_to_selection(track)
        }
        View::Selection | View::Sampler | View::Tape | View::Mix | View::Loops => return,
    };
    // Keep the selection's cursor on a track: on the first once a track is
    // added, and within the list when unselecting shortens it.
    let n = f.session().selection().len();
    match outcome {
        Outcome::RemovedFromSelection => {
            let row = f
                .cursor(View::Selection)
                .map(|i| i.min(n.saturating_sub(1)));
            f.set_cursor(View::Selection, if n == 0 { None } else { row });
        }
        Outcome::AddedToSelection if f.cursor(View::Selection).is_none() => {
            f.set_cursor(View::Selection, Some(0));
        }
        _ => {}
    }
    move_cursor(f, 1);
    f.notify(outcome.into());
}
