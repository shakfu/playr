//! Work on threads of its own: peaks, slices, exports, scans, analysis and pruning.

use super::*;

impl Session {
    // --- background work ---

    /// Runs `work` on its own thread, which sends its result as an event.
    pub(super) fn spawn(
        &mut self,
        work: impl FnOnce(JobId) -> Option<Event> + Send + 'static,
    ) -> JobId {
        let job = self.next_job;
        self.next_job += 1;
        let events = self.events.clone();
        std::thread::spawn(move || {
            if let Some(event) = work(job) {
                events(event);
            }
        });
        job
    }

    /// Reads the peaks of `track`, stopping any read still running, which
    /// then sends nothing. Finishes with [`Event::Peaks`].
    pub fn read_peaks(&mut self, track: PathBuf) -> JobId {
        self.cancel_peaks();
        let cancel = Arc::new(AtomicBool::new(false));
        self.reading = Some(cancel.clone());
        let cache = self.peaks.clone();
        self.spawn(move |job| {
            // Taken before the read, so a file changed during it reads again.
            let stamp = crate::wave::stamp(&track);
            let held = stamp.and_then(|s| cache.lock().ok()?.get(&track, s));
            let result = match held {
                Some(peaks) => Ok(peaks),
                None => match Peaks::read(&track, &cancel) {
                    Ok(Some(peaks)) => {
                        let peaks = Arc::new(peaks);
                        if let (Some(s), Ok(mut c)) = (stamp, cache.lock()) {
                            c.put(track.clone(), s, peaks.clone());
                        }
                        Ok(peaks)
                    }
                    Ok(None) => return None,
                    Err(e) => Err(e),
                },
            };
            Some(Event::Peaks { job, track, result })
        })
    }

    /// Reads `track`'s peaks into the cache in the background, so a later
    /// [`Session::read_peaks`] of it finds them. Stops the read ahead still
    /// running. Sends nothing.
    pub fn read_ahead(&mut self, track: PathBuf) {
        if let Some(cancel) = self.ahead.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if self.has_peaks(&track) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.ahead = Some(cancel.clone());
        let cache = self.peaks.clone();
        std::thread::spawn(move || {
            let stamp = crate::wave::stamp(&track);
            if let (Ok(Some(peaks)), Some(stamp)) = (Peaks::read(&track, &cancel), stamp) {
                if let Ok(mut c) = cache.lock() {
                    c.put(track, stamp, Arc::new(peaks));
                }
            }
        });
    }

    /// Whether the cache holds `track`'s peaks, read since it last changed.
    pub fn has_peaks(&self, track: &Path) -> bool {
        let Some(stamp) = crate::wave::stamp(track) else {
            return false;
        };
        self.peaks
            .lock()
            .is_ok_and(|mut c| c.get(track, stamp).is_some())
    }

    /// Decodes frames `start..end` of `track`, which counts frames at `rate`,
    /// for a view too close for its peaks. Finishes with [`Event::Detail`].
    pub fn read_detail(&mut self, track: PathBuf, rate: u32, start: u64, end: u64) -> JobId {
        self.spawn(move |job| {
            let result = crate::wave::Detail::read(&track, rate, start, end).map(Arc::new);
            Some(Event::Detail { job, track, result })
        })
    }

    /// Stops the peaks read in progress, if there is one.
    pub fn cancel_peaks(&mut self) {
        if let Some(cancel) = self.reading.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Plans slices of the playing track, `cut`'s way. Finishes with
    /// [`Event::Planned`]; nothing is written.
    pub fn plan_slices(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let job = self.slice_job(cut, range)?;
        Ok(self.plan_job(job))
    }

    /// Plans `job` again with the slice edges and fades set now, as when they
    /// change after it was planned. Finishes with [`Event::Planned`].
    pub fn replan(&mut self, mut job: Job) -> JobId {
        job.edges = self.edges;
        job.fades = self.fades;
        self.plan_job(job)
    }

    /// Whether `job` was planned with the slice edges and fades set now.
    pub fn plans_current(&self, job: &Job) -> bool {
        (job.edges, job.fades) == (self.edges, self.fades)
    }

    pub(super) fn plan_job(&mut self, job: Job) -> JobId {
        let audio = self.onset_audio.clone();
        self.spawn(move |id| {
            let result = samples::plan_with(&job, &audio).map(|spans| Plan {
                job: job.clone(),
                spans,
            });
            Some(Event::Planned {
                job: id,
                track: job.path,
                result,
            })
        })
    }

    /// Writes `plan`'s slices. Finishes with [`Event::Exported`].
    pub fn write_slices(&mut self, plan: Plan) -> JobId {
        self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::write(&plan.job, &plan.spans),
            })
        })
    }

    /// Plans and writes slices of the playing track at once, `cut`'s way.
    /// Finishes with [`Event::Exported`].
    pub fn export(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let work = self.slice_job(cut, range)?;
        Ok(self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::export(&work),
            })
        }))
    }

    /// Converts `export`, or the last export, to `format` with
    /// ConvertWithMoss, into a directory of that name inside the export's.
    /// A relative `export` is under the samples directory. Finishes with
    /// [`Event::Converted`].
    pub fn convert(&mut self, format: String, export: Option<PathBuf>) -> Result<JobId, Refusal> {
        let program = self.convertwithmoss.clone().ok_or(Refusal::ConvertOff)?;
        if !program.is_file() {
            return Err(Refusal::NoConvertWithMoss(program));
        }
        let dir = match export {
            Some(dir) => self.samples.join(dir),
            None => self.exported.clone().ok_or(Refusal::NothingExported)?,
        };
        if !samples::kit_path(&dir).is_file() {
            return Err(Refusal::NoKit(dir));
        }
        Ok(self.spawn(move |job| {
            let result =
                crate::convertwithmoss::convert(&program, &samples::kit_path(&dir), &format);
            Some(Event::Converted { job, result })
        }))
    }

    /// Scans `dir` into the library file, creating it if there is none.
    /// Sends [`Event::ScanProgress`] as it goes and finishes with
    /// [`Event::Scanned`], after which the frontend calls [`Session::scanned`].
    /// The directory is recorded so a later [`Session::rescan`] covers it.
    pub fn scan(&mut self, dir: PathBuf) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !dir.is_dir() {
            return Err(Refusal::NotADirectory(dir));
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_into(&library, &dir, |stats| {
                    if stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: Some(dir),
                result,
            })
        }))
    }

    /// Watches the library's roots, or stops for `false`. A change sends
    /// [`Event::FilesChanged`] once files have been still for [`WATCH_QUIET`],
    /// so a copy of an album is one rescan. Without a library file or roots
    /// nothing is watched. Called again after a scan, which may add a root.
    pub fn watch(&mut self, on: bool) -> Result<(), String> {
        use notify::Watcher;
        self.watcher = None;
        let roots = db::roots(&self.conn).unwrap_or_default();
        if !on || !self.has_library_file() || roots.is_empty() {
            return Ok(());
        }
        let (changed, quiet) = std::sync::mpsc::channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                let relevant = !event.kind.is_access()
                    && event
                        .paths
                        .iter()
                        .any(|p| scan::is_audio(p) || p.extension().is_none());
                if relevant {
                    let _ = changed.send(());
                }
            })
            .map_err(|e| e.to_string())?;
        for root in &roots {
            watcher
                .watch(root, notify::RecursiveMode::Recursive)
                .map_err(|e| format!("{}: {e}", root.display()))?;
        }
        // Ends when the watcher, and with it the sender, is dropped.
        let events = self.events.clone();
        std::thread::Builder::new()
            .name("playr-watch".into())
            .spawn(move || {
                while quiet.recv().is_ok() {
                    loop {
                        match quiet.recv_timeout(WATCH_QUIET) {
                            Ok(()) => continue,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                            Err(_) => return,
                        }
                    }
                    events(Event::FilesChanged);
                }
            })
            .map_err(|e| e.to_string())?;
        self.watcher = Some(watcher);
        Ok(())
    }

    /// Whether [`Session::watch`] is watching.
    pub fn watching(&self) -> bool {
        self.watcher.is_some()
    }

    /// Re-scans every directory previously given to [`Session::scan`] or
    /// `playr scan`. Sends the same progress events as a single scan.
    pub fn rescan(&mut self) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        let roots = db::roots(&self.conn).unwrap_or_default();
        if roots.is_empty() {
            return Err(Refusal::NoRoots);
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_roots(&library, &roots, |stats| {
                    if stats.seen > 0 && stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: None,
                result,
            })
        }))
    }

    /// Analyses the library tracks under `dir`, or every track for `None`,
    /// whose stored analysis is missing or out of date. Sends
    /// [`Event::AnalyzeProgress`] per file and finishes with
    /// [`Event::Analysed`], after which the frontend calls
    /// [`Session::analysed`].
    ///
    /// It runs alongside playback and alongside a scan: it writes only the
    /// `analysis` and `album_loudness` tables, which nothing here caches.
    pub fn analyze(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        if self.analysing.swap(true, Ordering::Relaxed) {
            return Err(Refusal::AnalysisRunning);
        }
        let paths: Vec<PathBuf> = dir.into_iter().collect();
        let (analysing, events) = (self.analysing.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("analysis", || {
                analysis::run_into(
                    &library,
                    &paths,
                    false,
                    analysis::default_workers(),
                    |done, total| events(Event::AnalyzeProgress { job, done, total }),
                )
            });
            analysing.store(false, Ordering::Relaxed);
            Some(Event::Analysed { job, result })
        }))
    }

    /// Takes in a finished analysis: reads the measurements again, for the
    /// columns, and the gains, so tracks analysed just now play at their
    /// measured level.
    pub fn analysed(&mut self) {
        self.measure();
        self.send_gains();
    }

    /// What `playr analyze` measured about `path`, when the row still
    /// describes the file as the library knows it.
    pub fn analysis_of(&self, path: &Path) -> Option<analysis::Analysis> {
        let key = path.to_str()?;
        let (stat, a) = db::analysis::row_of(&self.conn, key).ok().flatten()?;
        let track = self.tracks.iter().find(|t| t.path == key)?;
        analysis::is_current(track, Some(&stat)).then_some(a)
    }

    /// The gains `path` would play at, whether or not ReplayGain is on.
    ///
    /// It reads the whole library's gains, as turning ReplayGain on does, so
    /// it belongs to a dialog rather than to a frame.
    pub fn gains_of(&self, path: &Path) -> crate::gain::Gains {
        db::analysis::gains(&self.conn, &self.tracks)
            .ok()
            .and_then(|mut gains| gains.remove(path))
            .unwrap_or_default()
    }

    /// The beat grid of `path`: set by hand, else from `playr analyze`, at
    /// its tempo times any octave correction.
    pub fn grid(&self, path: &Path) -> Option<analysis::tempo::Grid> {
        let key = path.to_str()?;
        let grid = match db::analysis::grid_edit(&self.conn, key).ok().flatten() {
            Some(g) => g,
            None => db::analysis::grid_of(&self.conn, key).ok().flatten()?,
        };
        let factor = db::analysis::tempo_fix(&self.conn, key).unwrap_or(1.0);
        Some(analysis::tempo::Grid {
            bpm: grid.bpm * f64::from(factor),
            ..grid
        })
    }

    /// Sets `path`'s beat grid by hand, as [`Session::grid`] gives it, or
    /// with `None` goes back to the analysis's. Analysing the track again
    /// keeps it.
    pub fn set_grid(
        &mut self,
        path: &Path,
        grid: Option<analysis::tempo::Grid>,
    ) -> Result<(), String> {
        let key = path.to_str().ok_or("the path is not UTF-8")?;
        let factor = db::analysis::tempo_fix(&self.conn, key).unwrap_or(1.0);
        let stored = grid.map(|g| analysis::tempo::Grid {
            bpm: g.bpm / f64::from(factor),
            ..g
        });
        db::analysis::set_grid_edit(&self.conn, key, stored).map_err(|e| e.to_string())?;
        self.tempo_rev += 1;
        self.measure();
        Ok(())
    }

    /// The DJ hot cues set in `path`: slot, from 1, and time in seconds.
    pub fn hot_cues(&self, path: &Path) -> Vec<(u8, f64)> {
        query::hot_cues(&self.conn, &path.to_string_lossy()).unwrap_or_default()
    }

    /// Sets DJ hot cue `slot`, from 1, of `path` to `at` seconds, or empties it.
    pub fn set_hot_cue(&mut self, path: &Path, slot: u8, at: Option<f64>) -> Result<(), String> {
        query::set_hot_cue(&self.conn, &path.to_string_lossy(), slot, at).map_err(|e| e.to_string())
    }

    /// Corrects the playing track's tempo by an octave, or puts it back. The
    /// correction is kept apart from the analysis, so analysing the track
    /// again keeps it.
    pub fn fix_tempo(&mut self, fix: analysis::TempoFix) -> Notice {
        match self.playing_track() {
            Ok((path, _)) => self.fix_tempo_of(&path, fix),
            Err(refusal) => refusal.into(),
        }
    }

    /// Corrects `path`'s tempo by an octave, as [`Session::fix_tempo`] does
    /// the playing track's.
    pub fn fix_tempo_of(&mut self, path: &Path, fix: analysis::TempoFix) -> Notice {
        use analysis::{TempoFix, MAX_TEMPO_FIX};
        let shown = self
            .bpm(path)
            .or_else(|| self.grid(path).map(|g| g.bpm as f32));
        let (Some(key), Some(shown)) = (path.to_str(), shown) else {
            return Refusal::NoTempoToFix.into();
        };
        let was = db::analysis::tempo_fix(&self.conn, key).unwrap_or(1.0);
        let factor = match fix {
            TempoFix::Double => was * 2.0,
            TempoFix::Halve => was / 2.0,
            TempoFix::Reset => db::analysis::genre_fix(&self.conn, key).unwrap_or(1.0),
        };
        if !(1.0 / MAX_TEMPO_FIX..=MAX_TEMPO_FIX).contains(&factor) {
            return Refusal::TempoFixLimit.into();
        }
        if let Err(e) = db::analysis::set_tempo_fix(&self.conn, key, factor) {
            return Notice::Failed {
                task: Task::FixTempo,
                error: e.to_string(),
            };
        }
        self.tempo_rev += 1;
        self.measure();
        Outcome::TempoFixed {
            bpm: shown / was * factor,
            factor,
        }
        .into()
    }

    /// Counts tempo corrections, so a frontend holding a tempo knows to read
    /// it again.
    pub fn tempo_rev(&self) -> u64 {
        self.tempo_rev
    }

    /// The tempo to show for `path`, from `playr analyze`: its grid's when it
    /// has one, times any correction made by hand.
    pub fn bpm(&self, path: &Path) -> Option<f32> {
        db::analysis::bpm_of(&self.conn, path.to_str()?)
            .ok()
            .flatten()
    }

    /// Takes in a finished scan: reads the library again, from its file if
    /// the session was running without one. Marks added to a library that was
    /// only in memory are not in the file, and are gone.
    pub fn scanned(&mut self) {
        if !self.has_library_file() {
            if let Some(conn) = self.library.as_deref().and_then(|p| db::open(p).ok()) {
                self.conn = conn;
                self.marks_for = None;
                self.marks.clear();
            }
        }
        self.reload();
    }

    /// Starts recording the master: everything the output plays, the player,
    /// the tape and the decks, to a new file under the samples directory.
    /// Returns the file.
    pub fn start_master(&mut self) -> Result<PathBuf, String> {
        if self.master.is_some() {
            return Err("already recording the master".into());
        }
        std::fs::create_dir_all(&self.samples)
            .map_err(|e| format!("cannot create {}: {e}", self.samples.display()))?;
        let dir = samples::unused_dir(&self.samples, "master")?;
        let stem = dir.file_name().unwrap_or_default().to_string_lossy();
        let path = dir.join(format!("{stem}.wav"));
        let rec = self.player.sources().record(&path).inspect_err(|_| {
            _ = std::fs::remove_dir(&dir);
        })?;
        self.master = Some(rec);
        Ok(path)
    }

    /// Asks the master's recording to stop; [`Session::master_done`] has the
    /// result once its file is written. False with none under way.
    pub fn stop_master(&mut self) -> bool {
        self.master.as_ref().inspect(|m| m.stop()).is_some()
    }

    /// The file the master is being recorded to.
    pub fn master_recording(&self) -> Option<&Path> {
        self.master.as_ref().map(|m| m.path())
    }

    /// The master's recording, once its file is written: after a stop, or
    /// when the output closed under it. A file written is added to the library.
    pub fn master_done(&mut self) -> Option<Result<crate::audio::record::Recorded, String>> {
        let done = self.master.as_ref()?.finished()?;
        self.master = None;
        Some(done.and_then(|r| self.add_to_library(&r.path).map(|()| r)))
    }

    /// Adds the audio file at `path` to the library, as a scan would, without
    /// recording its directory as a root.
    pub fn add_to_library(&mut self, path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let track = scan::read_track(&path)
            .ok_or_else(|| format!("{}: not a readable audio file", path.display()))?;
        db::upsert(&self.conn, &track).map_err(|e| e.to_string())?;
        self.reload();
        Ok(())
    }

    /// Whether `dir` can be pruned, or every recorded root when `None`, for a
    /// frontend to refuse before it asks.
    pub fn check_prune(&self, dir: Option<&Path>) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else {
            match dir {
                Some(dir) if !dir.is_dir() => Err(Refusal::NotADirectory(dir.to_path_buf())),
                None if db::roots(&self.conn).ok().is_none_or(|r| r.is_empty()) => {
                    Err(Refusal::NoRoots)
                }
                _ => Ok(()),
            }
        }
    }

    /// Removes the tracks under `dir` whose files are gone, and the marks of
    /// every file under it that is gone; with `None`, every recorded root.
    /// Finishes with [`Event::Pruned`], after which the frontend calls
    /// [`Session::pruned`]. Shares a scan's turn: one of either runs at a time.
    pub fn prune(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        self.check_prune(dir.as_deref())?;
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryFile);
        };
        let dirs = match &dir {
            Some(d) => vec![d.clone()],
            None => db::roots(&self.conn).unwrap_or_default(),
        };
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let scanning = self.scanning.clone();
        Ok(self.spawn(move |job| {
            let result = caught("prune", || {
                let fail = |e: rusqlite::Error| format!("{}: {e}", library.display());
                let conn = db::open(&library).map_err(fail)?;
                let mut removed = crate::db::Pruned::default();
                for d in &dirs {
                    let batch = db::prune_missing(&conn, d).map_err(fail)?;
                    removed.tracks += batch.tracks;
                    removed.marks += batch.marks;
                }
                Ok(removed)
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Pruned { job, dir, result })
        }))
    }

    /// Directories previously scanned into this library.
    pub fn roots(&self) -> Vec<PathBuf> {
        db::roots(&self.conn).unwrap_or_default()
    }

    /// Whether `dir` can be forgotten, for a frontend to refuse before it asks.
    pub fn check_forget(&self, dir: &Path) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else if db::stored_root(&self.conn, dir).ok().flatten().is_none() {
            Err(Refusal::NotARoot(dir.to_path_buf()))
        } else {
            Ok(())
        }
    }

    /// Forgets `dir` as a root and removes the tracks and marks under it,
    /// then reads the library again.
    ///
    /// Done here rather than on a job: it asks the filesystem nothing, so it
    /// is one transaction rather than a walk of every file. It still waits for
    /// a scan, which writes the same rows.
    pub fn forget_root(&mut self, dir: &Path) -> Result<Pruned, Refusal> {
        self.check_forget(dir)?;
        let removed = db::forget_root(&self.conn, dir)
            .ok()
            .flatten()
            .ok_or_else(|| Refusal::NotARoot(dir.to_path_buf()))?;
        self.pruned();
        Ok(removed)
    }

    /// Takes in a finished prune: reads the library and its marks again.
    pub fn pruned(&mut self) {
        self.marks_for = None;
        self.marks.clear();
        self.reload();
    }

    /// Gathers `paths`, files and directories, into tracks to play, with tags
    /// from the library where it has them. Finishes with [`Event::Opened`].
    pub fn open(&mut self, paths: Vec<PathBuf>) -> JobId {
        let library = self.library.clone().filter(|_| self.has_library_file());
        self.spawn(move |job| {
            let conn = library.and_then(|p| db::open(&p).ok());
            let known = |key: &str| {
                conn.as_ref()
                    .and_then(|c| query::by_path(c, key).ok().flatten())
            };
            Some(Event::Opened {
                job,
                playable: scan::playable(&paths, known),
            })
        })
    }

    // --- slices ---

    /// The job for cutting the playing track `cut`'s way, at its marks and
    /// position now, or within `range`.
    pub fn slice_job(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<Job, Refusal> {
        let (path, rate) = self.playing_track()?;
        // The loop follows the range, so a loop on is the range's.
        let loops = cut == Cut::Region && range.is_some() && self.player.status().looping.is_some();
        let marks = self
            .marks_for(Some(&path))
            .iter()
            .map(|m| m.frame)
            .collect();
        Ok(Job {
            path,
            rate,
            marks,
            at: Mark::at_time(self.player.position(), rate).frame,
            cut,
            range,
            samples: self.samples.clone(),
            edges: self.edges,
            fades: self.fades,
            loops,
            ot_file: self.ot_file,
            cuts: None,
        })
    }
}
