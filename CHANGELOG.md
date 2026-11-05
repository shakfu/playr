# Changelog

Notable changes to playr. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- An `audit` workflow checks `Cargo.lock` against the RustSec advisories weekly and whenever a manifest or the lock changes, failing on a vulnerability; `make audit` runs the same. Dependabot proposes Cargo and workflow-action updates weekly.

### Changed

- The number keys follow tab order: `6` opens Loops, `7` Tape, `8` DJ and `9` Mix, where Loops was `9` and the rest one lower. Loops is the sixth tab, so in the terminal `6` refused with "no Tape view here" and in the window each number past `5` opened the tab after its position. Renumbering over moving Loops to the end of the tabs, which would still leave the terminal's sixth tab on `9`.

- Five of the largest source files are split into submodules, with no change to behaviour or the library API: the playback engine moves from `audio/mod.rs` to `audio/engine.rs`, its crossfades and loops to `audio/engine/blend.rs`; `Session`'s methods to `session/{playback,lists,marks,jobs}.rs`; `dispatch` to `dispatch/{rows,editing}.rs`; the `:tape`, `:dj` and `:mix` parsers to `command/decks.rs`; and the tape's frontend actions and controls to `tape/{actions,controls}.rs`.

- A malformed Opus stream with no channel layout cannot reach an `unwrap` in the decoder's setup: the layout is checked once and reused. `playr-server` no longer panics on a poisoned lock around the state sent to pages, whose value each write replaces whole.

- Releases 0.1.0 to 0.11.0 move from `CHANGELOG.md` to `CHANGELOG-archive.md`.

### Fixed

- Four tests failed on the Windows runner. Two in `tests/cli.rs` expected the settings path as `playr/settings.toml`, but playr prints it with Windows' `\`. Two in `dedupe.rs` used library paths like `/x/a.flac`, which have no drive and so are relative on Windows; they now get `C:` there, as in `paths.rs`.

## [0.20.0]

### Added

- The tape has two LFOs and four modulation slots. An LFO is a sine, triangle, square or sample-and-hold, free from 0.01 to 20 Hz or synced to a part of voice 1's window, in phase with its head. A slot routes one to a voice's rate, start, pan, level, send, EQ or effect sends, or to the delay's time, feedback and tone, the reverb's damp, or Print FX, by a depth from -1 to 1. `:tape lfo a|b ...` and `:tape mod N ...` set them, and Mod in the FX lane's header shows them in place of the effects. They are kept with the tape. Values move every 32 frames on ramps after the settings', so a setting's own smoothing holds, and a tape with no routes plays as before, sample for sample. Gains dip from their setting rather than rising above it. Two LFOs and a slot matrix over a depth per field, which the lanes have no room for. Design in "Modulation" in `docs/dev/looper-engine.md`. Library API: `playr_looper::modulation`, `Setting::LfoShape`, `Setting::LfoSpeed`, `Setting::Route`, `TapeAction::Lfo`, `TapeAction::Route`, `tape::LfoSetting`, `TapeState::lfos`, `TapeState::routes`.

- `V` or `:pick` picks rows in the library, selection and queue, and `a`, `e`, `E`, remove and move then act on every picked row instead of the cursor row; `:pick N..M`, `:pick all` and `:pick none` pick in bulk, and in the window ctrl-click (command-click) picks a row and shift-click a range from the cursor. Picked rows show `*` in the terminal and on the page, and as chosen rows in the window. Picks are kept by track path, so sorting keeps them; a new search drops the library's. "Picked", not "selected", since the selection is a list of its own. Moving picked rows moves none when the first cannot go as far, so their spacing holds. Library API: `Action::Pick`, `action::Pick`, `dispatch::Picks`, `Frontend::picks`, `Frontend::picks_mut`, `Message::Picked`, `Message::RemovedPicked`.

- `:device` while a track plays says `bit-perfect` when the device receives the file's samples exactly, or names what changes them: a device other than an ALSA `hw:` one, resampling, ReplayGain, volume below full, the EQ, fades, the tape or decks on the same output, or an output format narrower than the file. Library API: `output::inexact`, `Status::output_format`, `Status::fading`, `Message::Output::inexact`.

- A track tagged drum and bass, jungle, footwork, juke, breakcore, hardcore, gabber, speedcore or neurofunk shows twice a halved tempo reading, as `:bpm x2` would: 174, not 87. The tempo prior reads any pulse above 170 BPM at half. It applies only with no BPM tag, a reading under 100 and the level above recorded, and is worked out from the tag when read, so retagging needs no new analysis. A genre rule over accent modelling, which would need labelled tracks to calibrate. `:bpm reset` returns to the doubled level. Library API: `analysis::FAST_GENRES`, `analysis::genre_fix`.

- `playr-server --tui` shows the terminal's interface on the server's playback, so the terminal and the page control one player; the instance lock kept `playr` from starting beside the server. The terminal's loop owns the model and runs the page's and OSC's requests between frames, over a second `Model` in another process, which the lock exists to prevent. They share one view: a page request moves the terminal off a view the page lacks, such as the sampler. Library API: `owner::handle`, `owner::drain`, `App::run_with`, `App::with_model`, `Message::Serving`.

- The `added`, `played` and `plays` columns sort and search the library by the day a scan first found a track, the day it last played, and how often it has. `added:30d..`, `played:..1y` and `added:2026-10-01` take UTC days or times counted back from now, and `plays:0` finds tracks never played. A play counts at half the track or 4 minutes, as Last.fm counts one, over counting at the start, which would count every skip; a loop's passes count once. Tracks already in the library have no `added` day: the file's mtime would date the copy, not the addition. Plays are a `plays` table keyed by path, as marks are, and `:sql`'s `library` view has the three columns. Library API: `Column::Added`, `Column::Played`, `Column::Plays`, `Column::date`, `Track::added_at`, `Track::played_at`, `Track::plays`, `db::record_play`, `db::now`, `Session::record_play`, `columns::civil`, `columns::days_from_civil`.

- The output device in use is shown. `playr devices` marks it `>`, beside the default's `*`, and warns when the `device` setting or `--device` names none listed. Every frontend reports it as a message, `output: ID (name)`, at startup and on `:device` (Playback > Output device in the window). Before, nothing said which device a `device` setting resolved to. Through PipeWire, the ALSA device names only PipeWire, so `:device` also names the default sink from `wpctl inspect`, over linking libpipewire for one string. Startup skips it, so a missing or stuck `wpctl` cannot delay the first frame. Library API: `Player::device`, `output::info`, `output::place`, `output::pipewire_sink`, `output::sink_description`, `Message::Output`, `Action::ShowDevice`.

- `:tape fx` folds or opens the FX lane, `:tape N fold` lane N, and `:tape N more` its drawer. Before, only the window's chevrons and More did. Library API: `TapeAction::Fold`, `TapeAction::Drawer`.

- The Loops view has an ID column, the ID `:tape N loop ID` takes. Library API: `loops::HEADINGS` and `loops::cells` have 9 cells.

- `playr dedupe` lists byte-identical copies of tracks, and `--apply` keeps one row of each, moving the others' playlist places to it. Only rows go: the files stay, and their paths are excluded from later scans until `--restore`. Name-only matches, as `is:duplicate` finds, are left alone, since they may be different masters. Library API: `db::dedupe`.

- `:messages` lists every message since start, newest first; in the window it opens the Messages window. A message too long for the terminal's bottom line ends in `...`. The line shares its width with the indicators, so at 100 columns a ConvertWithMoss warning lost its last words with no sign it had. Library API: `Action::ShowMessages`, `Presentation::MessageList`, `Input::Messages`.

- `playr --man` prints a man page and `playr --completions SHELL` a completion script for bash, zsh, fish, elvish or PowerShell, both generated from the parser, so they cannot drift from `--help`. The macOS and Linux archives hold `playr.1` and the bash, zsh and fish scripts.

- `[server]` in `settings.toml` takes playr-server's `listen`, `open`, `hosts`, `osc` and `osc_reply`, so a systemd unit need not carry them. A flag wins over its setting. Library API: `config::ServerSettings`, `Config::server`, `config::is_host_port`, `flags::Flags`.

- `--osc-reply` takes a host name, as `ipad.local:9001`, resolved at startup to an IPv4 address where the name has one, since a `.local` name's IPv6 address is often link-local and needs a scope. It took an IP address only, so the tablet needed a fixed one. Library API: `osc::resolve`.

- `--device none`, or `device = "none"`, plays to no device, in real time. The page's browser tests use it, and run in CI since; they needed an audio device, which runners lack. `none` over ALSA's `null`, which is a real device ID. Library API: `output::Null`, `output::NONE`.

- The page moves a selection or queue row by dragging it, as the window does: a mouse from anywhere on the row, touch by a `::` grip, so a finger elsewhere still scrolls the list. The row menu's Move up and Move down remain.

- The page shows the `columns` setting, `[server] columns`, and `:columns`, on rows wider than a phone. It read them and showed a fixed four. A phone keeps its two-line rows of title, artist, album and time, which no column list fits at that width, and a tablet leaves out album, album artist, genre and file, as it left out the album.

- `watch = true` rescans 2 s after files under the library's directories stop changing, in all three programs, so a copied album is one rescan. A watched rescan asks nothing about missing tracks and prunes them only with `auto_prune`; nobody asked for it, and a stray key would answer the question. Off by default: Linux caps inotify watches, one per directory. Library API: `Session::watch`, `Session::watching`, `Event::FilesChanged`, `session::WATCH_QUIET`, `Task::Watch`, `Settings::watch`.

- The Sampler view reads the next track's waveform ahead while one plays, in normal and repeat modes, so a change of track shows its waveform at once; a first visit decoded for 0.35 to 2.7 s on one machine. Only the next track: the waveform cache holds about six, so reading a whole selection ahead would push out what was just read. Library API: `Session::read_ahead`, `Session::has_peaks`.

- `:filter album` and `:filter artist` search for the row's album or artist, or the playing track's, as a search the box shows and can change; Same album and Same artist in the window's row menu. A first step toward grouping, which needs a view of its own in three frontends. Library API: `Action::Filter`, `FilterBy`, `Refusal::NoTag`.

- `:toggle all` selects every track the library view lists, such as search results, or unselects them all when all are selected; Select or unselect all above the window's results. Library API: `Action::ToggleAll`, `Session::toggle_all_selected`.

- `:crossfade 2`, or `crossfade = 2`, overlaps each track's last 2 s with the next one's first, on equal-power curves after each side's ReplayGain; Playback > Crossfade over 2 s in the window. The next track is decoded alongside and mixed before conversion, so the one stream and ring stay. A next track at another sample rate or channel count is converted to the stream's, and stays converted to its end, over opening a new stream, which leaves a gap. The two mix after conversion, so the incoming track's converter carries on alone once the outgoing one ends. At a join it replaces `:fade`. Library API: `Cmd::SetCrossfade`, `Action::Crossfade`, `Message::Crossfade`, `Settings::crossfade`.

- Surround Opus decodes, up to 7.1, through libopus's multistream decoder. Its channels are reordered from Vorbis order to WAVE order, as the other decoders give them, so a stereo device plays front left and right; in Vorbis order the second channel is the centre. It was refused at open.

- CAF, MKV and WebM files are indexed with their tags and, for MKV and WebM, their length. lofty reads none of the three. A CAF's `info` chunk is read by playr, since Symphonia skips it. Matroska's tags come through Symphonia, read by their raw `TARGET@NAME` keys: ffmpeg writes no target, so every tag reads as the album's, which Symphonia's own mapping turns into album artist or drops. The length is the segment's, where Matroska keeps it, over decoding each file at scan time. The next scan reads such files already in the library again, once. Library API: `AudioStream::tags`, `decode::Tags`.

### Changed

- A release tag publishes `playr` and its five library crates to crates.io from the release workflow, after the GitHub release, with `cargo publish --workspace`. It needs the `CARGO_REGISTRY_TOKEN` secret. `playr-gui` and `playr-server` stay unpublished.

- macOS keeps the library and settings in `~/Library/Application Support/playr`, and Windows the library in `%LOCALAPPDATA%\playr` and settings in `%APPDATA%\playr`. Linux keeps the XDG directories, and an `XDG_DATA_HOME` or `XDG_CONFIG_HOME` still wins everywhere. A file in the old `~/.local/share/playr` or `~/.config/playr` is used while the new place holds none, over moving it, which a failed move could lose. Library API: `paths`.

- The window bundles egui's Ubuntu Light and Hack and a 1 KB cut of its icon font, not its two emoji fonts, 726 KB of the binary for six symbols. Emoji in tags now draw as boxes: no system fallback here draws colour glyphs. The icon cut keeps egui's submenu arrow, U+23F5, which painting the transport's symbols would not have covered; `crates/playr-gui/assets/fonts/README.md` says how it was made.

- `playr loops scan DIR` refuses a directory holding tracks in the audio library, and says how many, unless `--move` is given. It took them out unasked, with their playlist places and marks, so a mistyped directory lost them. Library API: `loops::scan_dir` takes `move_tracks`; `Refused::HoldsTracks`.

### Fixed

- 16-, 24- and 32-bit integer output plays an integer source exactly. The conversion scaled by the format's positive maximum, 32,767 for 16-bit, while decoders divide by 32,768, and truncated, so most samples came out one step towards zero. It scales by the power of two and rounds, clipping only positive full scale. Library API: `output::to_i16`, `output::to_i24`, `output::to_i32`.

- MKV and WebM files show the title ffmpeg writes, or else the first track's name. ffmpeg puts it in the segment's Info, and a stream's in its track entry, which Symphonia 0.6 does not expose, so such files showed their file names. playr reads both from the EBML up to the first Cluster. The next scan rereads existing Matroska rows once.

- `:convert opxy` of an export at 48 or 96 kHz writes frame counts that match its samples. ConvertWithMoss 20.3.0 resamples OP-XY samples to 44.1 kHz but keeps the source rate's `framecount`, `sample.end` and loop points in `patch.json`: 24,000 for a file of 22,050 frames. playr rewrites them from each WAV, scaling the positions by the same ratio, over resampling before conversion, which would decode and write every slice a second time.

- A scan repairs the relative rows a playr 0.1.0 scan stored. A rescan added absolute duplicates beside them, and prune never matched them. A row merges into the one absolute row whose path ends with its own, keeping its playlist places, or takes the absolute path when its file lies under exactly one root; an ambiguous row is kept.

- `playr prune` and `playr loops prune` remove nothing under a directory that reads empty, and say why: that is how an unmounted drive's mount point reads. Pruning a root whose drive was unmounted removed every track under it, with its marks and playlist places. A file counts as gone only when its directory is there, or when the nearest directory above it that is there holds something, so a drive unmounted below a root keeps its tracks too. `playr scan` names an empty root rather than suggesting prune. Library API: `db::empty_dir`.

- `playr scan`, `playr prune` and `playr loops prune` exit 1 when a directory given is not one. Scan and prune reported it and exited 0. `playr loops prune` did not check, and removed every loop under a directory that was not there.

- Output into a closed pipe, as `playr search a | head` leaves, ends playr with exit 0. It panicked.

- A gapless change to a track at another sample rate, while the tape or the decks held the output, reopened the stream at the new track's rate. The track is now resampled to the held rate, and the stream stays open.

- The delay bus keeps echoes longer than 1 s. It was skipped, and its line cleared, after 1 s with no input and its output below -120 dB, which the gap before a 1.9 s echo is. It now waits 1 s past the longest delay, 2 s.

- The delay's Tone is capped at 0.45 of the sample rate. Faust's low-pass is unstable above Nyquist, so at 16 to 32 kHz a Tone near its 20 kHz top could blow the delay up.

- A voice prints into a write loop of another channel count: mono reaches both channels of a stereo loop, stereo is summed into a mono one. Mono printed into the left channel only, and stereo into mono dropped the right channel.

- A bounce until stopped's WAV is removed from the temp directory when its load is replaced by another. It stayed. Each bounce has its own file, and a second bounce is refused with `RecorderBusy` until the first is loaded; both had one file per process.

- A WAV loop whose `data` size is 0 or 0xFFFFFFFF, as streaming writers leave it, reads to the end of the file, and a size past the end is cut to it. Such loops were indexed with no length, or 4 GB of one.

- A time over a week, as `:seek 1e30`, is refused. Such times panicked in `:seek`, `:mark`, `:range`, `:tape window` and every other command taking a time. Huge cursor, queue and selection moves stop at the ends, and a sleep timer past the clock is not set, where each overflowed.

- In the window, a key closes the Roots and Info lists and does nothing else, as in the terminal. It also acted: `d` in Playlists asked to delete the playlist.

## [0.19.0]

### Added

- Bounce, beside Record on the Tape tab, and `:tape bounce [long|off]`: the mix as heard, after level, pan, mute and solo and with the delay and reverb, recorded into voice 4's loop. Write records the sends, which come before level and pan, so soloing voice 4 with Write on could not reproduce what was heard. One pass writes the write window at feedback 0, fading in and crossfading into where it began. Until stopped records up to 5 minutes and replaces voice 4's loop with a stereo loop that long, so voices of other lengths are caught whole; it records through Record's writer thread to a temporary WAV and loads that, over holding a 115 MB buffer in the callback, so it cannot run while Record does. Library API: `Setting::Bounce`, `Tape::bouncing`, `Status::bouncing`, `TapeAction::Bounce`, `Bounce`, `LONG_BOUNCE`, `TapeMessage::Bounced`, `NothingBounced`, `RecorderBusy`, `Deck::bouncing`.

- A delay and a reverb on the tape, each a send bus shared by the four voices. The sends are taken after Level, Mute and Solo, so a muted voice has no echo; the write head's Send stays before them. Print FX records the returns into the write loop. The controls are in a foldable FX lane above voice 4, the sends in each voice's drawer. Each effect has several models to compare by ear before the ones that sound worse are removed: delay Clean, Ping-pong and Tape (wow, flutter, saturation); reverb Freeverb, Zita, JP and Plate. Delay Sync sets the time to a part of voice 1's window, since most loops carry no tempo. Clean and Freeverb are ported from softcut-fx; the rest are Faust, compiled to Rust by `make faust` and committed, so builds do not need Faust. Faust code runs a frame at a time, measured at under 0.14% of a core over block processing. A bus that hears nothing for 1 s after its tail falls below -120 dB is skipped, so a tape without sends plays bit-exact. Measurements and Faust workarounds are in `docs/dev/fx-buses.md`. Library API: `playr_dsp::fx`, `playr_dsp::faust`; `Setting::DelaySend`, `ReverbSend`, `Fx`, `PrintFx`; `TapeAction::Fx`, `Control::Fx`, `VoiceSetting::Delay`, `Reverb`, `Layout::fx`, `Idle::NoFxSend`, `Idle::Model`; `Confirm::ResumeTape` holds a `Box<SavedTape>`.

  ```
  :tape 1 delay 0.5    :tape delay model tape    :tape delay sync 3/16    :tape reverb decay 4    :tape print-fx 0.5
  ```

- A delay and a reverb on the DJ decks, the tape's, shared by both decks. Each channel strip sends to them, and FX by the crossfader opens their fields. A deck sends as it reaches the main mix, after its fader, the crossfader and mute, over before the crossfader: a deck crossfaded out but cued, fader up, would otherwise reach the main mix through the reverb. A fader cut leaves the tail. The returns skip the headphone cue. Delay Sync is a part of a bar of 4 beats of the deck heard most, and follows its tempo. The buses moved into `playr-dsp` so both engines share them. Library API: `playr_dsp::fx::Buses`, `BusSetting`; `playr_dj::Setting::DelaySend`, `ReverbSend`, `Fx`; `DjAction::DelaySend`, `ReverbSend`, `Fx`, `DjState::sends`, `fx`, `Decks::fx_idle`; `playr_app::fx` holds `FxSetting`, `FxState`, `Division` and `FxField`.

  ```
  :dj a delay 0.5    :dj delay model tape    :dj delay sync 1/4    :dj reverb decay 3
  ```

- A fourth tape voice, whose loop is the write loop: the tape is a four-track looper, and with Write on, the write head records every voice's send into voice 4's loop, with feedback, whatever loop each voice reads. Voice 4 plays it back at its own rate; voices 1 to 3 can play it too by picking voice 4's loop, or `:tape N share`. A loop loaded into voice 4 is written over, and Reset restores it; with voice 4 empty, Write makes it silent, the length of the first loop in voices 1 to 3. Its lane is on top, with Write and a band for the write window, and a second row of fields for the write head. The waveforms take the height the controls leave, so four lanes fit 800 by 592. This replaces voice 1's loop being the one written, and the hold that kept writing off while any voice read a loop of its own: with each lane on its own loop, writing was always held. A fourth sinc voice costs about a third more CPU (scaled from `docs/dev/aliasing.md`, not measured). Library API: `VOICES` is 4; `WRITER`, `TAPE`, `Handle::blank_tape`. `Tape::write_held` and `Idle::Held` are gone.

- Clear and Eject on each tape lane, and `:tape N clear|eject`. Clear silences the write loop and keeps its length, windows and heads, so the write head can print into it again; it is on voice 4 and on lanes reading voice 4's loop, and Reset restores it. A voice's own loop is refused with `TapeMessage::OwnLoop`: nothing writes it, so a clear could only lose it. Eject empties the voice, and once every voice is empty the tape goes. A clear swaps in a silent loop built off the audio thread rather than zeroing in the callback, which for a minutes-long loop would overrun it. Library API: `Cmd::Clear`, `Cmd::Unload`, `Tape::clear`, `Tape::unload`, `Handle::clear`, `Handle::eject`, `Handle::unload`, `Loop::empty`; `TapeAction::Clear`, `Eject`, `TapeMessage::Empty`, `Emptied`.

- Tape lanes fold and have drawers. A folded lane shows its header and a strip of its waveform; an empty lane is folded until a loop is loaded, unless unfolded by hand. Under each open lane, one row holds Window, Rate, Level, Pan and Send; **More** opens a drawer with Slew, Wear, Drive, Filter, Type and Xfade, and on voice 4 the write head's fields. This frees room for the per-lane EQ and effect sends to come: every lane showed all 11 fields, which left no room in the smallest window. The layout is kept with the tape. Library API: `tape::Layout`, `Deck::layout`, `set_layout`, `Model::set_tape_layout`.

- `persist = ["tape"]` keeps the tape between runs: each voice's settings and the file its loop was read from, in the library. At the next start playr asks to take the tape up again, after the player's question; on yes it reads each loop again and puts back the settings, with Write off and the heads at their windows' starts. A file that is gone leaves its voice empty and is named. The write loop is kept by reference while it is as loaded; once written into or cleared it comes back silent, its length, as its audio is not kept: the voices that printed it can print it again, and Save loop keeps a print. Library API: `Persist::Tape`, `SavedTape`, `Reads`, `Deck::saved`, `tape::resume`, `Confirm::ResumeTape`, `TapeMessage::Resumed`.

- Mute on each tape lane, **M**, and `:tape N mute on|off`. A muted voice is not heard and its lane dims; its head and send go on, so it can still print into the write loop. Turning a voice on now starts its head at its window's start, or its end in reverse, rather than where it stopped. Library API: `Setting::Mute`, `VoiceSetting::Mute`, `VoiceState::mute`, `Control::Mute`, `Idle::Muted`.

- A loop list in each tape lane. It searches the loop library as typed, as `/` does in the Loops view, and loads the loop picked into that voice; `:tape N loop ID` does the same. Each row, and the lane under its loop's name, shows where the loop lies in its file, `1:02.3-1:06.1`: slots are named after their track, so the name alone did not tell them apart. Voices 1 to 3 can pick voice 4's loop instead. Library API: `Session::find_loops`, `TapeAction::Share`, `TapeAction::Loop`, `TapeState::empty`, `Deck::voice_span`, `loops::span_text`, `loops::span_of`.

- A loop library, apart from the tracks. A loop is a span of a file: a precut loop file, the loop its `smpl` chunk names, or a track's loop slot. `playr loops scan DIR` indexes a directory of loops; `playr loops roots`, `rm` and `prune` manage the directories. Each loop has tempo, beats, key and tags, from a BPM tag, a WAV `acid` chunk, the file name, or else the length, guessed and shown as `~120.0`. A file the `acid` chunk marks one-shot is left out. The Loops view, `9` in both frontends, lists and searches them (`bpm:118..124`, `key:Am`), with From and To, where each lies in its file; `:load N` loads one into voice N, `:beats N` and `:key K` correct it, `:remove` drops it and keeps the file. A reference into the file over a copy: a slot keeps the track either side for crossfades, and no audio is duplicated. Apple Loops metadata is not read; it is in `TODO.md`. Design in `docs/dev/loop-library.md`. Library API: `db::loops`, `loops` in `playr-core`; `Session::loop_list`, `search_loops`, `library_loop`, `add_loop_file`, `remove_loop`, `set_loop_beats`, `set_loop_key`; `View::Loops`, `Action::LoopLib`, `LoopAction`, `playr_app::loops`.

  ```
  playr loops scan ~/samples/loops    :view loops    /bpm:120 drums    :beats 8
  ```

- A loop per tape voice. Each voice holds a loop of its own, the write loop, or nothing; voices 2 to 4 start empty, and Load range loads into voice 1 alone. Each lane loads its own (see the loop list above), not the Loops view, so a load never lands in a voice the user did not pick. Rates and starts are set by hand. Only the voices a load changes fade, over 5 ms. A loop cut to loop takes its pre-roll and post-roll from its own other end, so its wraps crossfade into the loop's start rather than silence. Library API: `Tape::load_voice`, `source`, `has_own`, `write_held`, and `Tape::load` returning the voices' own loops; `Cmd::LoadVoice`, `Handle::load_voice`, `has_own`, `Status::voice_columns`, `Error::Voice`; `Deck::voice_extent`, `TapeState::own`, `Idle::Held`, `TapeMessage::LoadedVoice` and `NoSuchLoop`, `tape::load_loop`.

- `:fade SECONDS`, or `fade` in `settings.toml`, fades each track in over its first stretch and out over its last, 0 to 10 s; `:fade off`, the default, plays tracks as they are, gapless. The fade goes by the track's position, so a seek into its last seconds is faded too, and a track whose length is estimated short plays its tail unfaded rather than silent. Loops and auditions keep their own edges. The window has Fade tracks over 2 s and No fades under Playback. This is the TODO's first step: no overlap, one decoder. Library API: `Cmd::SetFade`, `Action::Fade`, `Message::Fade`, `Settings::fade`, `settings::MAX_FADE_SECS`.

- Mark labels and export. `:label TEXT` labels the selected mark, `:label -` clears it, and `:label` opens the command line on its label; the window offers Label... on a mark's menu and draws each label beside its mark. A label moves with its mark. `:mark-export audacity` writes the playing track's marks as an Audacity label track, `samples/NAME-labels.txt`; `:mark-export cue` writes a cue sheet, `samples/NAME.cue`, a track at each mark titled by its label. Both are under Sampler, Edit too. The library's `marks` table gains a `label` column, added on open; the schema version stays, so an older playr still opens the library. `:sql`'s `marks` view has `label`. Audacity labels and cue sheets over SFZ: they carry the marks as they are, while slices already reach samplers through the export's `.sfz`. Library API: `labels` module with `MarkFile`, `audacity` and `cue`; `Session::label_mark`, `labels_for` and `export_marks`; `query::labels` and `set_label`; `Action::Label`, `StartLabel` and `ExportMarks`; `Outcome::Labelled` and `MarksExported`; `Snapshot::labels`, `Layout::labels`, `with_labels` and `label_at`.

  ```
  :label big break    :label -    :mark-export audacity    :mark-export cue
  ```

- Fine varispeed. `:speed +50c` moves the speed in cents, a hundredth of a semitone, and `:speed 1.25` takes semitones to a hundredth. `:tempo 128` on an analysed track sets the speed that plays it at 128 BPM, to the nearest cent: 112 cents from 120 BPM, which plays at 128.01. A semitone is 5.9%, so whole steps moved 120 BPM to 113.3 or 127.1 with nothing between. The resampler already took any ratio. The window's speed slider moves in cents and sets the speed where a drag stops, since each change seeks the player again. The status bar shows `(+1.12 st)`. OSC's `/playr/speed` stays in whole semitones, so a TouchOSC layout keeps working. Library API: `Action::SpeedBy` and `SetSpeed` count cents; `Cmd::CentsBy` and `SetCents` replace `SpeedBy` and `SetSpeed`, `Status::cents` replaces `semitones`, `speed_at` and `MAX_CENTS` replace `speed_for` and `MAX_SEMITONES`; `Action::Tempo`, `Message::TempoOutOfReach`, `message::semitones`.

  ```
  :speed +50c        :speed =-1.5        :tempo 128
  ```

- `:preview`, or `v`, in the library, queue or selection shows the row's waveform in the sampler without playing it, with the track's marks; the window offers it as Preview in sampler on a row's menu. While it shows, the sampler refuses what acts on the playing track, such as seeking, marks, the range, loops, slicing and auditions, and says to play the track to edit it. `:preview` in the sampler, or End preview, shows the playing track again, as does playing the track previewed. View only over making the sampler act on the previewed track: every sampler action names the playing track, and a click that seeks one track while showing another would be ambiguous. `:pre` no longer names `:prev`; `:prev` in full still does. Library API: `Action::Preview`, `Sampler::preview` and `shown`, `sampler::Preview`, `Layout::previewed` and `Layout::playing`, `Message::Previewing`, `PreviewEnded`, `NothingToPreview`, `NotPreviewing` and `PreviewOnly`.

- `:eq bypass`, or `t`, plays the player's EQ flat and keeps its bands; again, it puts them back. Setting a band ends the bypass. The EQ popup has a Bypass button. Every other playback control had a key; a key bound to `:eq flat` would lose the bands on a stray press. Library API: `Action::BypassEq`, `Message::EqBypassed`, `Mix::eq_held` and `hold_eq`.

- The Mix tab's headphones strip has a meter, as the other strips do: the cued deck's peak after the headphone fader, held for 1.5 s. Library API: `Mixer::take_cue_peak` and `Status::take_cue_peak` in `playr-dj`; `Snapshot::meters` gains a fifth entry, the headphones.

- Each program shows its version while running. The keys help title of all three reads `Keys in the library view (playr 0.19.0)`. `playr-server` prints `playr-server 0.19.0: ADDRESS` on start, and `GET /config` returns it. The window's Help, About playr shows it with the author and links to the repository, its releases and its issues. Before, only `--version` gave it, which says nothing about a process already running. Library API: `playr_app::VERSION`.

### Fixed

- The window's status line keeps its row when empty, and a click on it, or View > Messages, opens every message since start, newest last, the last 500 kept. A refused action, such as Write with voice 1 empty, gave its reason on a line that took no room until then and went after 4 s, so the control looked dead. One place for messages over a second line beside the Tape tab's buttons: the history covers a message missed in any view. Library API: `Model::log`, `LOG_LEN`.

- Write with voice 4 empty takes the write loop's length from the first of voices 1 to 3 holding a loop. It required voice 1, and refused with only voices 2 or 3 loaded.

- The window draws library text in scripts its fonts lack, such as Chinese, Japanese, Korean, Arabic or Thai, where before it drew boxes. When the library, queue, selection or playlists change, the characters egui cannot draw are looked up in the system's fonts, and each font that covers some of them is added. A library without such characters loads none. On macOS and Windows each script has a list of files, smallest first, and only the scripts present are read; Linux asks `fc-match`. The window holds what it loads: on macOS 8 MB for Japanese and 15 MB for Korean. System fonts over bundled ones: bundling CJK alone would add 10 MB or more to each binary. Only the macOS list is tested.

- `[` and `]` centre the sampler's view on the range end they select, turning Fit on if it was off. Before, with Fit off, they selected the end but the view stayed on the playhead, so they seemed to do nothing. A drag from an end now selects it with `:select-edge`, which leaves the view and Fit alone, so the waveform does not jump under the pointer. Library API: `Action::SelectEdge`.

### Changed

- Each tape voice's filter is now an EQ: a high-pass, a low-pass and a peaking bell with gain and Q, in series after Drive, all in Hz and dB, in the lane's drawer. The one filter could cut highs or lows, not both, so overlapping loops could not be carved to a band, and its 0-to-1 cutoff gave no frequency to aim at. With the drawer open, the lane's header draws the EQ's response curve, computed from the filters' transfer functions rather than measured, so it is exact and costs no audio. Commands: `:tape V hp HZ|off`, `lp HZ|off`, `bell HZ`, `bell-gain DB`, `bell-q Q`; `:tape V filter` is gone and says what replaced it. Library API: `Setting::HighPass`, `LowPass`, `BellHz`, `BellGain`, `BellQ`, `EqShape`, `eq_db`, `FILTER_HZ`, `BELL_DB`, `BELL_Q`, `Svf::set_q`, `Idle::Flat`; `Setting::Cutoff`, `Setting::Filter` and `Filter` are gone.

  ```
  :tape 2 hp 120    :tape 2 lp 6000    :tape 2 bell 800    :tape 2 bell-gain -6    :tape 2 bell-q 4
  ```

- Backspace takes a loop out of the Loops view, as `d` and delete do, and as it does in the queue and the selection. A Mac's delete key sends backspace.

- The Tape tab has a lane per voice: its loop, window, head and crossfades, with its controls in one row under it, numbered from the bottom with voice 4 on top. Before, one waveform showed the selected voice's loop, so a voice on its own loop was seen only by selecting it. Sliders became number fields, which fit 800 points beside every loop. Design in `docs/dev/tape-lanes.md`. Library API: `Deck::voice_name`.

- The loop slots, F1 to F8, move from the `loops` table to the loop library's `loop_lib` when the library opens. The schema version stays, so an older playr still opens the library, but sees no slots; a slot it saves moves on the next open.

- The tape's Save loop adds the loop to the loop library, not the tracks. A mix recording still goes to the tracks.

- The Sampler menu's Loops submenu is now Loop slots, apart from the Loops view. The window's search field is 180 points wide, from 240, so the tabs still fit 800 points.

- The DJ decks' and the tape's read heads interpolate with a 32-tap Kaiser-windowed sinc, stretched by the read rate above 1, in place of the 4-point cubic Hermite. The cubic's images reached -26 dB at 10 kHz at any rate but 1, and at the tape's x4 everything above 5.5 kHz folded back at full level; the sinc keeps every spur 83 dB down or more. Its top octave rolls off a little more: 18 kHz is 4 dB down. It costs more: three tape voices at x4 take 9.8% of a core where the cubic took 0.6% (one machine). The cubic stays: `interp = "hermite"` in `settings.toml`, `:interp`, `i` in the Tape, DJ and Mix views, or Heads in the Mix tab switch both decks and the tape at once, for comparison by ear. A whole frame at rate 1 is played exactly under either. Measurements in `docs/dev/aliasing.md`. Library API: `playr_dsp::interp` with `Interp` and `Kernel`; `Setting::Interp` in `playr-dj` and `playr-looper`; `Action::Interp`, `Message::Interp`, `Mix::interp` and `set_interp`, `Config::interp`.

- A track picked for a playing deck, with strict off, crossfades in over 1 s once read. Before, the deck faded out over 5 ms, then played silence while the new track decoded and resampled, and only then started it. The old track plays on during the read and fades out from where it is, equal-power, since the two tracks are unrelated; the deck holds both until the fade ends, and the old one goes back off the audio thread. A pick made during the fade waits as the deck's next track. Library API: `Cmd::Replace`, `Handle::replace`, `Returned::Fading` and `Faded`, `REPLACE_MS` in `playr-dj`.

- `:convert` splits an export past a format's zone limit into presets that each fit, as AudioHit splits `.ot` files, where ConvertWithMoss kept the first 24 slices for the OP-XY and dropped the rest. 40 slices give `amen-1` with 24 and `amen-2` with 16, each on keys from C1; the message says "as 2 presets". Split over refused: a long export still reaches the device whole. The OP-XY's 24 is the only limit known. Library API: `convertwithmoss::LIMITS` and `limit`, `Converted::parts`, `Outcome::Converted::parts`.

- The sampler keeps the waveforms of recent tracks in memory, up to 100 MB, dropping the least recently used, so returning to a track draws at once rather than decoding it again. A read took 0.35 s for a 4-minute FLAC, 0.6 s for the same as AAC, 0.8 s at 96 kHz, and 2.7 s for a 36-minute AAC mix (one 32-core machine, release build). A 4-minute track at 44.1 kHz holds 17 MB, so about six fit; the newest is kept whatever its size, since the view holds it anyway. A file whose modification time or size changed is read again. In memory over a file cache, which would need a format, invalidation and cleanup for a handful of working tracks. Library API: `wave::Cache`, `wave::stamp`, `Peaks::bytes`, `Spectrogram::bytes`, `session::PEAKS_CACHE`.

- An edited plan survives a change of track. A plan with a slice moved or joined is kept with its undo and redo, and comes back when its track plays again; it is lost at quit. Before, the track change dropped the plan and cleared undo, so starts set by hand were gone. Keeping each plan over confirming first: a prompt would interrupt moving through the list, and no edit is lost either way. Library API: `Sampler::kept`, `sampler::Kept`.

- The window's Slice drop-down reads Edited once a planned slice is moved or joined, where it kept the name of the method that made the plan. Choosing that method, or moving its count or sensitivity, plans again over the edits, and the drop-down now says so before it happens.

- The spectrogram's transform scales with the sample rate: 2048 points to 48 kHz, 4096 to 96 kHz, 8192 above. At a fixed 2048, a bin was 94 Hz at 192 kHz, so bass on hi-res files blurred into one row; now it is 21.5 to 23.4 Hz at every rate, at about five times the transform work per frame on a 192 kHz file (N log N). `playr analyze` already sized its transform this way. Library API: `spectrum::fft_size` replaces `spectrum::FFT`.

- The tape's Take hands over on one frame: the output callback pauses the player on the frame the tape starts. Before, the tape started on the looper's next block and the player paused when the engine next ran, so the two could land a callback apart (inference, not measured). From inside the range the handover is set 100 ms ahead of the player, so the callback sees it before that frame plays. Library API: `Player::hand_over`, `handover` and `cancel_handover`, `bus::Handover`, `Source::start`, `Looper::play` in `playr-looper`.

- The README shows how to start `playr-gui` detached from the terminal: `open -a playr` on macOS, `&!` in zsh, `& disown` in bash. A plain `&` dies when the terminal closes. The binary does not detach itself, so the terminal, `cargo run` and debuggers still see its exit status and output.

## [0.18.0]

### Added

- `:mix rec` records the master, everything playr plays, before the headphone cue is routed, to a 32-bit float stereo WAV under the samples directory, and adds it to the library once stopped. Overs are kept. The model holds a meter for each strip after its fader, and one for the measured master. Library API: `Sources::record`, `record::{MasterRecording, Recorded}`, `Session::start_master`, `stop_master`, `master_recording` and `master_done`, `Player::take_master_peak`, `MixAction::Record`, `Snapshot::meters`, `Mix::fader_gain`.

- `:tape take`, or Take in the Tape tab, loads the sampler's range and hands the player over to the tape, as a deck's Take does: from the player's position inside the range, or at the range's start once the player reaches it. Past the range it is refused. Library API: `TapeAction::Take`, `Setting::Head` in `playr-looper`.

- A mixer. The master volume now scales the tape as well as the player and the decks, and each of the three has a fader and a mute. The DJ headphone cue has its own fader, which the master leaves alone; before, the master scaled it. The player's volume ramps across each audio callback rather than stepping at its start. The decks' level meter now reads the main mix before the master, as the player's and the tape's do. The tape's recording ignores the faders. The window's Mix tab has a strip each for the player, the tape, the decks, the headphones and the master: a fader, its level in dB, a meter after the fader and a mute, with Record and the law on the master's. The transport's EQ button opens a popup, titled "player EQ", in place of its dialog. The server plays only the player, so its page gains a mute beside the volume, and OSC `/playr/mute`. Library API: `playr_app::mix`, `Action::Mix`, `Frontend::mix`, `Model::mixer`, `Message::Mix` and `FaderLaw`, `Persist::Mix`; `Handle::set_volume` and `Cmd::Volume` in `playr-looper`; `Setting::Headphones` and `Mixer::take_peak` in `playr-dj`. The design is in `docs/dev/mixer.md`.

  ```
  :mix                        :mix tape 80          :mix decks -10
  :mix player mute [on|off]   :mix headphones 70    :mix law [db|cubic]
  ```

- `:mark-slices`, or Sampler, Slice, Mark slice starts, marks each planned slice's start and keeps the marks there are. A plan is lost when the track changes; marks are kept in the library, and `:slice marks` plans them again. One undo takes them all back. Library API: `Action::MarkSlices`.

- `:in TIME` and `:out TIME` set one end of the range at a time rather than the playhead; `:zoom N` zooms to step N. Library API: `Action::RangeIn` and `Action::RangeOut` take an `Option<Duration>`, `Zoom::To`.

- Two DJ decks. Each plays a library track at its own varispeed rate (fader ranges +/-8, 16 and 50%, a held nudge of 4%), with a CDJ-style cue. Sync follows the other deck's tempo at half, the same or double, moves into phase with it, and holds it there with a rate trim of up to 5%; quantize starts play, cue, hot cues, jumps and seeks in phase. The mixer has a trim, a 3-band isolator EQ with kills, a one-knob filter, a mute and a channel fader per deck, a crossfader with a constant-power and a sharp curve, and a headphone cue, split across one stereo output or on channels 3 and 4 of a 4-channel device. Each track's beat grid comes from `playr analyze`, a track never analysed is analysed on load, and grid edits are kept with the track. The decks play on their own output stream, as the tape does, and pause the player. The volume slider is their master volume. Take, or `:dj a take`, moves the track the player is playing onto a deck and continues it from the player's position, at its speed and ReplayGain, and the player pauses. The terminal has the commands and the window a DJ tab; the server has neither. On click tracks the beats stay within 1 frame of each other 5 minutes after sync.

  ```
  :dj a load            the cursor row onto deck A
  :dj a take            the player's track, from where it is
  :dj a play|pause|cue  :dj a cue down|up     :dj b sync [off]
  :dj b rate 2.5        :dj a range 8|16|50   :dj b nudge +|-|off
  :dj a gain G          :dj a level L         :dj xfade X
  :dj quantize on|off   :dj cue a|b|off
  :dj a grid x2|/2|<|>|reset                  :dj a grid offset MS
  :dj a tap
  :dj a eq low|mid|high DB                    :dj a kill low|mid|high on|off
  :dj a filter K        :dj a hot N [clear]   :dj a jump BEATS
  :dj a loop BEATS|off  :dj cue-out split|3-4 :dj curve smooth|sharp
  :dj a seek TIME|PCT%  :dj a mute on|off     :dj a unqueue
  :dj a mark next|prev  :dj strict on|off     :dj xfade a|b|centre
  ```

  Each deck has four hot cues, kept with the track, beat jumps, loops of 1/4 to 32 beats, and the sampler's marks to jump between. A track picked for a playing deck replaces it: the deck fades out, takes it and plays it. With strict on, for a set where a mis-click must not cut into the mix, it waits as the deck's next track until the deck stops. The window's DJ tab draws each deck's waveform around the playhead with its grid and marks, and an overview a click or drag seeks on; it lists the library under the decks, following the search, with A and B on each row. A, Centre and B glide the crossfader there over 400 ms.

  The looper and the decks share their ramps and filters in the new crate `playr-dsp`. Library API: the new crates `playr-dj` and `playr-dsp`; `Action::Dj`, `playr_app::dj`, `Frontend::dj`, `Model::decks` and `set_decks`, `command::side_name`; `Session::set_grid`, `fix_tempo_of`, `hot_cues`, `set_hot_cue` and `marks_of`; `db::analysis::grid_edit` and `set_grid_edit`, `db::query::hot_cues` and `set_hot_cue`; in `playr-dj`, `Track::with_hot_cues`, `Setting::PlayFrom` and `device::render`; `Decks::device_events`; `Svf::reset` in `playr-dsp`. If the device goes away, the decks close and say so; a load opens them again. `docs/guide-dj.md` explains the DJ tab; the design, and where the build departs from it, is in `docs/dev/dj-engine.md`.

- A tape looper over the sampler's range. Three voices read one loop, each at its own rate (-4 to 4, negative in reverse), over its own window, with its own level, pan, crossfade, rate slew, drive, filter, and a low-pass on what it sends; a voice can turn at its window's edges instead of wrapping, and can be soloed. A write head records the sends back into the loop with feedback, a darkening low-pass and a thinning high-pass of its own, so each pass changes the loop. The load keeps up to a second of the track either side of the range as pre-roll and post-roll, so a crossfade fades into the audio that followed the window and the loop repeats exactly one window long. While the write window is the range, the write head writes the rolls as the range's continuation, so they keep the loop's level and carry its sends; frozen rolls played the track at full level at every wrap once feedback had faded the loop. On a stereo loop, pan folds the far channel into the near one rather than dropping it. The loop or the mix saves as a 32-bit float WAV under `samples/<track>-tape/` and is added to the library. It plays on its own output stream, on the player's device, and pauses the player. The terminal has the commands; the window adds a Tape tab with the waveform, the windows and heads, the selected voice's window dragged on the waveform by its edges or whole, and a strip of controls per voice and one for the write head. The server has neither. Library API: the new crate `playr-looper`, with `Loop::with_range`, `Setting` (including `VoiceWear`, `Ping`, `Slew`, `Drive`, `Cutoff`, `Filter`, `Solo` and `Thin`), `Filter`, `crossfade`, `fade_frames`, `DEFAULT_SLEW_MS` and `MAX_SLEW_MS`; `Action::Tape`, `playr_app::tape` with `no_effect`, `message::idle_text`, `Frontend::tape`, `Model::deck`, `Session::add_to_library`; `samples::read_frames`, `unused_dir` and `name_for` are public; `Handle::recording`, `Deck::device_events`. If the device goes away, the tape closes, finishing a recording under way; a load opens it again.

  ```
  :tape load [N]        the range, or loop slot N, of the playing track
  :tape play|stop|reset|save|rec
  :tape 2 rate -0.5     :tape 3 window 25% 75%     :tape 2 send 0.6
  :tape 2 wear 0.4
  :tape 1 ping on       :tape 1 slew 500           :tape 3 drive 0.25
  :tape 1 filter 0.4    :tape 1 filter hp          :tape 2 solo on
  :tape write on        :tape feedback 0.85        :tape wear 0.3
  :tape thin 0.2
  ```

  The design, and where the build departs from it, is in `docs/dev/looper-engine.md`.

- `:slice beats N` cuts the region every N beats, 0.125 to 64, or every bar of 4/4 without N, at the track's tempo. A fraction cuts within the beat: 0.5 gives eighth notes. The cuts fall in phase with the first mark inside the region, else its start, and a cut within half a beat of either end, or half a slice when slices are shorter than a beat, is left out, so a range a few frames longer than 2 bars cuts as 2 bars rather than 2 and a sliver. A track with no tempo yet is analysed first and cut when that finishes; one with no clear pulse is then refused. The window's Slice drop-down offers it as Beats. Library API: `Cut::Beats`, `samples::beat_points`, `Slicing::Beats`, `command::DEFAULT_BEATS`, `MIN_BEATS` and `MAX_BEATS`, `Refusal::NoTempo`, `Outcome::FindingTempo`.

- `playr analyze` finds a beat grid: the tempo to 0.01 BPM and where the first beat falls. Where the estimate reads under 80 BPM and records the double, as a house track at 62 BPM with 124 recorded, the grid takes the faster level: at half the tempo it tests every other beat, and on that track it settled 0.6 of a beat off the kick. Above 80 the estimate stands; taking the double there put hip hop read at 95 at 190. The tempo the status bar and the tempo column show, sort by and `:slice beats` cuts at is the grid's where there is one; `bpm:` finds it as well as the estimate. On synthetic click tracks the grid holds within 2.6 ms of the beats, and within 2.3 ms after 5 minutes; `docs/dev/analyze.md` has what it does on a real library. Library API: `tempo::Grid`, `Tempo::grid`, `Tempo::estimate`, `analysis::HALVED_BELOW`, `Analysis::grid`, `db::analysis::grid_of`, `Session::grid`.

- `:bpm x2` and `:bpm /2` correct the playing track's tempo by an octave, up to two either way, and `:bpm reset` puts it back; the window has them under Sampler, Tempo. The tempo shown, sorted, searched and cut at follows. The correction is kept in a table of its own, not in the track's analysis, whose row is replaced when the track is analysed again; pruning a gone file or forgetting its root removes it. The server's page cannot make one, as it cannot use the sampler. Library API: `TempoFix`, `Session::fix_tempo`, `Session::tempo_rev`, `db::analysis::tempo_fix` and `set_tempo_fix`.

- A range cut whole while it loops gets a `smpl` chunk in its WAV files, the slice file and `sliced/NAME.wav`, looping the whole file. Before, only `samples.json` and the `.sfz` file held the loop, and samplers that load a bare WAV read neither. The root note is middle C, as for a lone sample; the kit's key 36 is a drum-pad convention. Library API: `sliced::smpl_chunk`, `sliced::append_chunk`.

- `dj_knee` and `tape_knee` in `settings.toml` set, in dBFS, where the DJ master's and the tape write head's soft clips start to bend peaks: -0.9 and -6 by default. Library API: `Setting::Knee` in `playr-dj` and `playr-looper`, `playr_dsp::soft_clip` and `flush`.

### Changed

- The window's Tape, DJ and Mix tabs are views, with keys of their own under `[keys.tape]`, `[keys.dj]` and `[keys.mix]`. `6`, `7` and `8`, or `:view tape`, `dj` and `mix`, open them, and Tab steps through them after the sampler. Before, they were panels over another view: no key opened them, and every key acted in the view underneath, so `space` paused the player in the Tape tab. In the Tape view `space` plays the tape; the DJ view keeps the library's rows, cursor and search; the defaults are in `keys.toml`. The terminal and the web page have no screen for them, so they refuse them and Tab passes over them. Library API: `View::Tape`, `Dj`, `Mix`, `View::TERMINAL` and `step`, which replaces `next` and `prev`; `Frontend::views`, `Model::show_views`.

- The tape and the DJ decks play on the player's output stream, summed with it, instead of opening streams of their own. They no longer need a second stream, which a device held exclusively, such as an ALSA `hw:` device, refused; this is not yet tried on such a device. While the tape or a deck is loaded, a track at another rate is resampled to the stream's rather than reopening it, which would cut the others off; the player alone still plays each track at its own rate. Starting a deck no longer pauses the player; the tape's Play and both Takes still do. The headphone cue's routing takes in everything playing, so in Split the player is in the left ear with the main mix. `:dj cue-out 3-4` reopens the stream with 4 channels where the device has them. Library API: `playr_core::audio::bus` (`Source`, `Cue`, `Bus`, `BusControl`), `Player::sources`, `Sources`, `Attachment`; `Backend::start` and `output::render` take the bus; `Engine::process_buses` and `Mixer::process_buses` in `playr-dj`; `playr_looper::device` and `playr_dj::device` are gone. The design is phase 3 in `docs/dev/mixer.md`.

- Volume faders follow a decibel law: each 1% of travel is 0.6 dB, so 50% is -30 dB, not -6 dB. Linear amplitude spent the top half of the slider on the first 6 dB. `fader = "cubic"` in `settings.toml` uses the position cubed instead, as PulseAudio does, and `:mix law` switches between the two for the session. The setting `master` sets the master's position. `volume`, which set a gain, is still read and plays at the same level; a file setting both is an error. A remembered volume converts once to a position at the same level. Under `db`, one press of `-` from the top is now 3 dB, not 0.4 dB.

- The window's Range starts here and Range ends here set one end, as `i` and `o` do. They set both before, putting the other end at the track's start or end, so the menu and the keys left different ranges. The zoom slider performs `:zoom N`, so it goes through the same path as every other control. Library API: `DraftAnswer::of_key`.

- Library API: planned slices are edited in core, by `Plan::move_start`, `Plan::join` and `Plan::index_of`, so an edited plan and one made again from it follow one rule. `samples::spans_from` is no longer public. `Session::revision` counts changes to the tracks, their order and their measurements. `playr_server::web::check` returns the action it checked, which `Model::run_checked` performs.

- After a seek, and on a new output stream, the device starts once 100 ms of audio is buffered, rather than on the first frames. A device started on a few frames underran when the engine woke late for the next. A seek now starts up to 100 ms later.

- The guides are `docs/guide-*.md`: the sampler and server guides under their new names, and new ones for the DJ tab, the tape and the library. Release archives and the Linux `playr` package ship them all, with the cheatsheet and the diagrams they link to; before, only the server guide shipped.

- Opus is on by default, in `cargo build` and `cargo install` as in the release archives, so building needs cmake. `--no-default-features` leaves it out. `make test` and `make clippy` run once, with Opus, not once with and once without.

- The tests that ask the machine for its audio devices, and the smoke test that plays the default one, run only with `PLAYR_DEVICE_TESTS` set, as `make test-devices` and CI set it. Listing devices takes about 14 s a test on macOS. Tests build `playr-core`, the DSP crates and the FFT and decoding crates under them at `opt-level` 1, which takes the analysis tests from 25 s to 3.3 s. Optimising every dependency took them to 1.3 s, but a cold test build from about 5 to 27 minutes.

- `Session::grid` gives the grid a DJ deck uses: one set by hand, else the analysis's, at its tempo times any `:bpm` correction. The tempo shown, sorted and searched takes a hand-set grid's tempo first.

- The analyser is version 5, for the grid, so `playr analyze` measures every track again. Until it does, tempos are not shown or searched, as on the move to version 2.

- playr-gui's minimum height is 592 points, up from 504, so the Tape tab's strips fit without scrolling.

- Undo brings back an edited plan that was discarded or replaced by a new cut, such as one from the window's Sensitivity slider. Before, the starts set by hand were lost with no way back. Making or replacing an unedited plan is still not a step: undo would put back a cut a later one replaced.

### Fixed

- On a phone, the server's page scrolled sideways. With the Queue tab, the four tabs needed 387px of a 374px row; they now shrink, and a long count is cut off. The selection's toolbar did not wrap, so its three buttons widened the page to 546px, and a confirm dialog opened centred on that width with Yes off screen. `make page-test` had failed since the server's token: the fixture took the address printed into a pipe, whose token is `...`. It now reads the token file, and checks each view's width.

- Digits typed into a number field in the window, such as the sampler's slice count, ran the keys bound to them instead, which show views and sections. Only the search, command and name fields held the keys; any field being typed into now does.

- `:analyze PATH` found no tracks when the library held them by a path through a symbolic link, such as macOS's `/var`: it compared the stored paths only with the resolved one. It now matches either.

- `#` pressed twice on a slice whose start is a mark moved the mark too. The older snap's result, which should have been dropped, was applied to the mark there.

- Editing slices cut at marks without a range used the region around the playhead as their bounds. The first slice could not move below the region's start, and a plan made again, as on `:slice-edges`, ended the last slice at the region's end: inverted, so the export wrote fewer slices and reported no error. Slices out of order are now refused when planned or written.

- With `zero` slice edges, a slice start moved by hand stayed where it was put, off a crossing, until the plan was made again. It now moves to the nearest crossing at once. An equal or onset plan edited by hand also ended its last slice unsnapped when made again; it now ends where the unedited plan did.

- In the window, dragging a range end past the other swapped the ends, leaving the wrong one selected. It now stops a frame short, as `<` `>` do.

- In the window, a drag released while another view showed, or after the track changed, carried on: with Scrub on, a pointer passing over the waveform played it.

- `playr-server`: a client sending a request a byte at a time held a connection for as long as it kept sending, since only each read was timed. 32 such clients, without the token, locked out every page. The whole request now has 10 s, and pages left open take at most 24 of the 32 connections, so commands still get through.

- `playr-server`: a command, key or row the server answered with 503 after 5 s still ran once the model got to it, so a retry ran it twice. It is now either run and answered, or never run.

- Analysis results did not reach the columns until the library was read again, as by a scan: the loudness, peak and BPM columns stayed empty, and a sort by them was wrong. The web page also kept its rows after a sort or an analysis changed them in place.

- Undo or redo that failed to write a mark, as on a locked library, left the marks half put back, and the step could not be redone. The marks now change in one transaction, all or none. Library API: `Session::set_marks`.

- `playr-server` prints the token only to a terminal. Under systemd its output goes to the journal, which other users in the `adm` or `systemd-journal` groups can read; there it names the token's file instead. Event streams send `X-Accel-Buffering: no`, so nginx passes events on as they come. `docs/server-guide.md` says a proxy must pass `Host` through.

## [0.17.0]

### Added

- Undo and redo for the sampler: `u` and `r`, or `:undo` and `:redo` in any view. They cover the playing track's marks, range and planned slices, keep the last 100 changes, and restore what was selected. Without them a stray key lost an edit, and a mark's place had to be found by ear again. Library API: `Action::Undo`, `Action::Redo`.

- Planned slices can be edited before they are written: select one with `,` `.` or a click on its start, move its start with `<` `>`, `#` or a drag, and join it to the slice before with backspace. An onset or equal cut rarely puts every edge where it belongs, and before, the only fix was another cut. Edits survive `:slice-edges`. Library API: `Job::cuts`.

### Changed

- The sampler edits a selected mark, range end or slice instead of the mark under a cursor. The cursor was a second, silent position that went stale: after its mark was removed or the track changed, the mark keys could act on another track's mark. A selection belongs to its track and goes with its item. `docs/dev/mark-selection.md` has the design.

  | key | does |
  |-|-|
  | `{` `}` | select the previous or next mark and play it; outside the sampler, seek to it |
  | `,` `.` | in the sampler, select and play the previous or next planned slice, as `n` `p` did; elsewhere, seek to a mark, as before |
  | `[` `]` | select a range end |
  | `<` `>` | move the selection |
  | `#` | move the selection to the nearest rise |
  | backspace | remove the selection; with nothing selected, clear the range |
  | `D` | select nothing |
  | `i` `o` | set the range's start or end, as `<` `>` did |

  `esc` only discards planned slices now; backspace clears the range. `n` `p` skip tracks in the sampler again. The cursor's commands are gone; `:move`, `:move-to`, `:onset`, `:remove`, `:select` and `:deselect` replace them. Library API: `Action::SelectMarkAt`, `SelectSliceAt`, `Deselect`, `MoveSelected`, `MoveSelectedTo`, `SnapSelected` and `RemoveSelected` replace the cursor and mark actions; `Sampler::selected` replaces `Sampler::cursor`.

- The window's Save loop button reads Save range while nothing loops. It saves the range's ends, not audio, and needs no loop running.

## [0.16.0]

### Added

- The window's sampler shows the whole track in a strip above the waveform, with the stretch in view framed; a click or drag there seeks, and the view follows. A zoom slider sits between the `-` and `+` buttons, and a time axis runs along the waveform's top.

- `:convert FORMAT EXPORT` converts an earlier export, named by its directory under `samples`, which Tab completes, or by a path; without EXPORT it converts the last one written, as before, which a restart forgets. The window has Sampler, Slice, Convert an export to, which picks the directory in a dialog. A directory with no `.sfz` kit, such as an export from before 0.15.0, is refused before ConvertWithMoss runs; it used to fail only once the job ran. Library API: `Action::Convert` takes the export, `Session::convert` too, `Session::samples_dir`, `Session::exports`, `Refusal::NoKit`; `command::completions` and `CommandLine::complete` take a function listing the exports.

- `:convert` offers nine more formats in Tab completion and the window's Convert to. For hardware: `mpc` (MPC keygroups), `sp404mk2`, `opxy`, `deluge`, `emulti` (Elektron Tonverk) and `s2400`. For software samplers reported not to load SFZ: `exs24` (Logic's Sampler), `sxt` (Reason's NN-XT) and `nki` (Kontakt 1, for Kontakt 6 and later). Each was converted from a kit laid out as playr writes one and keeps every slice and its loop, except the MPC's: ConvertWithMoss makes each slice one-shot, which ignores the loop. Polyend Tracker is left out, as it keeps only a kit's first slice, and Maschine, which fails without an option `:convert` cannot pass. `docs/dev/hardware_samplers.md` has the results.

- A Scrub button in the sampler: while it is on, a drag across the waveform plays 40 ms from wherever the pointer moves, a grain at a time, the last one even if the pointer then holds still, then loops the range on release. Loop alone already looped a dragged range, but only on release and silently during the drag. Each moment is a seek, so it may stutter on formats slow to seek (untested). `:scrub TIME` plays one moment. Library API: `Action::Scrub`, `sampler::SCRUB`, `sampler::tick_step`, `sampler::fmt_tick`.

- `playr --print-settings` prints the default settings and keys as one `settings.toml`. Before, the defaults were only in the source tree, which an installed binary lacks. It prints even when the user's settings file is broken.

### Changed

- `:convert` names what ConvertWithMoss left out. Converting 40 slices to `opxy` keeps 24, the most the OP-XY plays; the message said only "converted to", and the loss was in ConvertWithMoss's output, which playr read only on failure. It reports a loss as a plain line among its progress and exits 0, so playr takes the lines that say something was dropped, skipped, ignored, truncated or discarded, the words its messages use, and any error output, and shows them first: `ConvertWithMoss: The preset has 40 regions but the device plays at most 24, the rest is dropped. converted to ~/Music/playr/samples/tones-2/opxy`. Library API: `convertwithmoss::convert` returns `Converted`, with the warnings; `Outcome::Converted` carries them.

- playr-gui's minimum height is 504 points, up from 480, so the waveform keeps 200 points with the overview above it. The sampler's Track info button reads `info`; `i` looked like `|`.

### Fixed

- Loop could pause the audio it was meant to play. `l` started from a pause sent a toggle when the published status said paused, but a command sent just before it, such as `:audition` or a scrub's moment, could already have started playback in the engine; the toggle then paused it, leaving the loop set and silent. Loop and From start now send `Cmd::Resume`, which plays from any state and never pauses, so the engine decides from its own state. Library API: `Cmd::Resume`.

- The README said `settings.toml` and `keys.toml` joined in file order make a valid settings file. The window refused it with `unknown setting: theme`: `keys.toml`'s top-level `theme` fell under `settings.toml`'s `[gui]` table. `--print-settings` puts `theme` first.

## [0.15.0]

### Added

- `:convert FORMAT` turns the slices last written into another sampler's format, by running [ConvertWithMoss](https://github.com/git-moss/ConvertWithMoss): `:convert sf2` writes `amen/sf2` inside the export. Each export now holds an `.sfz` file that puts its slices on keys from C1, which samplers that read SFZ load as it is and ConvertWithMoss takes as its source. playr writes SFZ and leaves the other formats to ConvertWithMoss, which already keeps over 40 current, over writing each itself. It is an extension, the first: it runs a program that is not playr's, so it is off until `convert-with-moss.enable = true` under `[extensions]` in `settings.toml`, and until then `:convert` is left out of `:help`, Tab completion, the window's menu and the command line's errors, and no prefix finds it. ConvertWithMoss is run from its installer's path or `convert-with-moss.path`, never from `PATH`; while it is not there, `:convert` is refused with that path and the window's entry is disabled. The window has it under Sampler, Slice, Convert to; the server's page may not run it. Library API: `convertwithmoss::convert`, `convertwithmoss::FORMATS`, `samples::kit_path`, `Session::convert`, `Session::convert_enabled`, `Session::can_convert`, `Session::exported`, `settings::ConvertWithMoss`, `Command::extension`, `command::parse_typed`, `Event::Converted`, `Action::Convert`. `docs/dev/hardware_samplers.md` has the formats tried.

- Each export also holds its audio as one file with the slice points in it, for samplers that slice a file instead of loading a kit: `sliced/amen.wav`, with a cue point at each slice's start in a `cue ` chunk, and, with `slice_ot_file = true` in `settings.toml`, `sliced/amen.ot` beside it for the Octatrack, for up to 64 slices. The `.ot` file is off by default, as few users have an Octatrack. The `.ot` file is byte for byte what ot_utils 0.1.5 writes for the same slices, and ffprobe, sox and Python's `wave` read the WAV; neither has been loaded on a device. A cue point holds a frame, as common writers store it; one reference says bytes, and `docs/dev/hardware_samplers.md` records the question. Library API: `sliced::cue_chunk`, `sliced::append_cue`, `sliced::ot_file`, `samples::sliced_dir`.

### Changed

- playr-gui opens 800 points wide, down from 1100, and the sampler's controls take two rows under the waveform instead of five: they were redesigned to fit this width rather than given more room. At 800 x 480 the waveform is 210 points high, up from 116. Zoom, Track info and a display drop-down sit right of the file name. Snap, Fit and Loop are buttons that stay pressed while on. A Slice drop-down replaces the four slice buttons: choosing a method plans with it, None discards, and it shows a plan made with `:slice` too. The controls that review a plan show only while one waits. Saved loops show as bands under the waveform, where a click loops one; Save loop fills the first empty slot. A right-click on the waveform marks or sets a range end there, and snaps or deletes a mark under it. The transport's row of mark buttons is gone; Playback already listed them. A Sampler menu replaces the Slice menu and lists every sampler action with its key. `docs/dev/ui-refactor.md` has the design.

- `l` and `:loop` with no range set the range to the region around the playhead and loop it, as `:audition` plays the region; they refused before. A range with one end set still refuses.

### Fixed

- The window had no control that selects, moves, snaps or deletes a mark, or moves the cursor; only keys did. `controls::MARK_BAR` listed them for the parity test, which reads the tables, but no view drew it. They are under Sampler, Marks.

- A loop kept playing after leaving the sampler, where `:loop` was refused, so only a stop or a new track ended it. `:loop off` now works in every view; starting a loop stays in the sampler, where the marks are shown.

- The playing row could jump back for a moment after rows above it were removed, as when played tracks leave the queue. `Player::send` shifts the status index at once, but the engine published its own index, from before it took the removal, over it. The engine now publishes its index only once it has taken every command sent.

## [0.14.0]

### Fixed

- `:clear`, `c` and Clear queue did nothing when the only queued track left was the one playing or paused: clearing kept it, so as not to cut it off, then counted nothing to clear and said the queue was empty. It now leaves the queue too and plays on as part of the list, which continues after it as it would after the queue. Library API: `Cmd::Unqueue`.

### Added

- `|` in the sampler returns a fitted view to the whole range after `[` or `]` centred it on one end, as `:fit on` does. It took `f` twice.

- The queue is kept between runs, unless `keep_queue = false`. It is a setting of its own rather than a `persist` name, since a user's `persist` list replaces the default and would turn it off unseen. The queue is stored in the library at each change, in a new `resume_queue` table, and offered with the track playing at the next start: "take up amen.flac again at 1:35, with its queue of 5 tracks?". The list that was playing is not stored, so playback stops once the queue has played. Library API: `db::SavedQueue`, `db::set_resume_queue`, `db::resume_queue`, `Session::remember_queue`, `Session::keep_queue`, `Settings::keep_queue`; `Session::resume` and `Session::resumable` take and return the queue, and `Confirm::Resume` has a `queue` field.

- `:save` in the Queue view saves every queue row as a playlist, played, playing and waiting, so tracks can be queued, heard, pruned and kept. The window and the page have a Save as playlist button above the queue. Library API: `Session::save_queue`, `Session::check_save_queue`, `Session::queue_tracks`, `Refusal::QueueEmpty`; `Confirm::ReplacePlaylist` is a struct variant with a `queue` field.

- The selection is saved, as it changes, to a playlist named `draft`, and starts empty each run. A draft restored silently into the selection could end up in a new playlist unnoticed; as a playlist, it is visible and can be played, edited or deleted. The first change in a run that finds an earlier draft asks whether to overwrite it, append it, or save it under a name; `draft = "overwrite"`, `"append"` or `"off"` sets the answer. The name `draft` is reserved. Library API: `settings::Draft`, `Settings::draft`, `session::DRAFT`, `DraftChoice`, `Session::set_draft`, `Session::take_draft_question`, `Session::settle_draft`, `Refusal::NameReserved`, `Outcome::DraftOverwritten`, `Outcome::DraftAppended`, `Input::Draft`, `DraftAnswer`, `Model::answer_draft`, `Model::save_title`.

- `:save-search [NAME]` keeps the search shown, with its sort, as a list that stays current: it runs again each time it is opened. The Playlists view lists saved searches after the playlists; enter shows the results in the library view, `a` selects them, `e` and `E` queue them, `o` reopens the search and `d` deletes it. Names are shared with playlists, so `:playlist NAME` stays unambiguous. Renaming one in place is not yet possible. Library API: `query::Query`, `query::SavedSearch`, `query::searches`, `query::save_search`, `query::delete_search`, `Session::searches`, `Session::set_shown`, `Session::shown`, `Session::save_search`, `Session::run_search`, `Session::delete_search`, `Session::add_all_to_selection`, `Refusal::NoSearch`, `Outcome::SearchSaved`, `Action::StartSaveSearch`, `Action::SaveSearch`, `Prompt::SaveSearch`, `Confirm::ReplaceSearch`, `Confirm::DeleteSearch`; `Prompt::Search` carries the text to start from.

- `:sql SELECT path ...` lists the tracks a statement names, as search results, keeping its `ORDER BY`, and `:save-search` keeps it as a list that stays current. Statements read the views `library`, `playlists` and `marks`, named as the search fields are, not the tables, so a saved one outlasts schema changes. The page may send one, so it runs on a read-only connection whose authorizer allows only `SELECT`, reads through those views and a list of functions, refusing writes, `PRAGMA` and `ATTACH`, which could read any file on the host; it is stopped after 2 s, refused past 100,000 rows, and no value may exceed 1 MB. It runs on its own thread, as a scan does, so the interface keeps responding; a later statement replaces one still running. rusqlite gains its `hooks` and `limits` features. Library API: `db::sql`, `Session::sql`, `Session::sql_in_background`, `Session::tracks_at`, `Event::Sql`, `Refusal::Sql`, `Action::Sql`, `SqlThen`, `dispatch::sql_done`, `Frontend::sql_started`; `Prompt::Command` carries the text to start from.

- `:edit`, on `o` in the Playlists view, replaces the selection with the playlist's tracks to edit, asking first if it held any. The selection's title names the playlist, `s` offers its name, and saving under it replaces the playlist without the usual question; saving or clearing ends the edit. Adding a playlist to the selection to edit it mixed in any tracks already selected. The playlist being edited is remembered with the draft, so appending an old draft, or `:edit` on the draft, takes the edit up again. Library API: `Session::edit_playlist`, `Session::editing`, `Outcome::Editing`, `Action::EditPlaylist`, `Confirm::EditPlaylist`.

- `backspace` and `delete` take a track out in the Queue and Selection views, as `d` does.

- `:add`, on `a`, in the Queue view adds the track to the selection. A track could move from the selection to the queue, but not back. Library API: `Session::add_to_selection`.

- `:stop after` stops once the track playing ends, and `:stop in TIME` is a sleep timer; `:stop in off` cancels it. The engine checks the flag where it picks the next track, as `after_queue = "stop"` does, so the next track is never started and cut. The status bar shows either while set, and the window's Playback menu and the page's menu have both. Library API: `Cmd::StopAfter`, `Status::stop_after`, `Session::sleep_in`, `Session::sleep_left`, `Session::sleep_due`, `Snapshot::sleep`, `Action::StopAfter`, `Action::StopIn`, `message::stopping`.

- `history` in `persist` keeps the last 100 `:` command lines between runs. Library API: `Persist::History`, `History::new`.

- The empty library view names `:scan DIR`, which fills it without leaving playr.

### Changed

- Library API changes that break callers: `Confirm::ReplacePlaylist` is a struct variant and `Confirm::Resume` has a `queue` field; `Prompt::Search` and `Prompt::Command` carry the text to start from; `Session::resume` takes the queue and `Session::resumable` returns it; `Frontend` requires `sql_started`.

- The queue's commands take the selection's names: `:remove`, `:move` and `:clear`. The old names, `:dequeue`, `:reorder` and `:queue-clear`, remain as aliases, so key bindings written for them still work. A command two views share names both when refused elsewhere, and `:map` no longer suggests one view for it.

- Queued tracks that have played stay in the Queue view, dimmed, above the track playing; they used to leave it, so a queue could not be saved once heard. Tracks skipped by enter on a later row count as played, where they were dropped. Enter on a played row plays it again, `:move` moves a track among the played rows or among the waiting ones, and `:clear` empties both. The session holds them, not the player's list, so repeat and shuffle do not replay them. Library API: `Session::played`, `Session::play_queue_row`, `Model::queue_played`.

## [0.13.0]

### Added

- The sampler's region line gives the region's or range's peak, loudness in LUFS, and stereo correlation. A region under 400 ms shows its RMS in place of LUFS. They come from the decode that draws the waveform, which now also keeps a momentary loudness reading every 100 ms and each bucket's side energy. That makes a 4-minute track's peaks about 2.6 MB larger, and reading them 44% slower: 273 ms against 190 ms in a release build. Library API: `wave::Stats`, `Peaks::stats`, `meter::block_frames`, `Histogram::add`.

  Correlation is sum(2LR) / sum(L^2 + R^2), not Pearson's. It needs one more value a bucket where Pearson's needs two, and the two agree when the channels are equally loud. Numbers on a chosen range replace a live scope view that was tried and dropped. Its waveform trace and spectrum repeated what the sampler shows, and a stereo image mattered less than a number for deciding where to cut.

- A three-band EQ: `:eq bass|mid|treble N|=N|+N|-N`, and `:eq flat`, from -12 to 12 dB. Bass is a shelf below 100 Hz, mid a wide peak at 1 kHz, treble a shelf above 10 kHz. In the window, an EQ button beside Mode opens a dialog with a slider a band and Flat; the page takes the command. The EQ starts flat and is not saved. Library API: `audio::eq`, `Cmd::SetEq`, `Player::eq`, `Shared::eq`, `Biquad::process`; `render` takes an `Eq`; `Action::SetEq`, `EqBy`, `FlatEq`; `Message::Eq`; `Snapshot::eq`.

  A boost raises its band and nothing else, so at full volume a large one can clip a loud track; the volume applies after the EQ, so turning it down makes room. Lowering the whole level by the largest boost prevents that, but a boost then turned everything else down, and raising a band made the track quieter. A limiter changes the sound when it acts. The EQ runs in the output callback, before the meter, not in the decoder, which runs up to 2 s ahead of what is heard. With every band at 0 it passes samples unchanged.

- `persist = ["eq", "volume", ...]` remembers session values between runs: `eq`, `volume`, `mode`, `replaygain`, `theme`, `columns` and `sort`, none by default. They are stored in the library, in a new `state` table, and win over the setting of the same name while listed; `columns` and `sort` are kept for each program. `settings.toml` stays the file the user writes and playr never changes, so the values are state in the library rather than keys written back to it. A change is stored once it has held still for 500 ms, and when playr quits or its window closes. Library API: `settings::Persist`, `Settings::persist`, `db::state`, `db::set_state`, `Session::state`, `Session::set_state`, `persist`, `Config::program`, `model::SAVE_AFTER`.

- `playr export NAME [FILE]` writes a playlist as M3U8, and `playr import FILE...` saves M3U files as playlists. Playlists lived only in `library.db`, so a lost library lost them, and no other player could read them. Import leaves out tracks not in the library and lists them, where scanning their folders first would add to the library without asking. It refuses a name already taken rather than replace that playlist. On Windows, where the library stores canonical `\\?\` paths, export writes them in the plain form other players read. Library API: `m3u::write`, `m3u::paths`, `m3u::name`, `m3u::resolve`, `m3u::plain`.

- Every column is a search field, by the name `:columns` and `:sort` take: `year:1955..1965`, `loudness:..-14`, `time:5:00..`, `genre:jazz`, `path:/backup/`. `tempo:` is `bpm:`, and `albumartist:` stays beside `album_artist:`. `genre`, read at scan since 0.1.0, is now a column. Library API: `Column::Genre`, `db::analysis::current`.

- `is:` searches the findings of `playr analyze`: `damaged`, `unreadable`, `padded`, `no-checksum`, `wrong-length`, `lossy`, `upsampled`, and `duplicate` and `unanalysed`. Findings were visible only in `--report` and `:info`, one track at a time. `is:duplicate` is judged across the whole library whatever else the search holds, so `is:duplicate path:/backup/` lists the copies under one folder.

- A queue of tracks you choose, played over the library. Enter in the library plays on through it as before, and queues nothing. `:enqueue`, on `e`, queues the track or playlist under the cursor: the first queued over the library plays at once, and the rest wait. `:enqueue next`, on `E`, puts a track first among those waiting; `:enqueue all`, on `A`, queues every search result or the selection. Enter on search results, a playlist or the selection replaces the queue with that list from the track chosen. Once the queue has played, the library resumes at the track after the one it interrupted, or stops with `after_queue = "stop"`. A fifth view, Queue, on `2`, lists the track playing if it was queued and the tracks waiting; enter jumps, `:dequeue` on `d` takes a track out, `:reorder +N|-N` on `J` and `K` moves one, and `:queue-clear` on `c` empties it. The window and the page have the view, row menus and a Clear queue button. Library API: `Cmd::PlayWith`, `Cmd::Insert`, `Cmd::Remove`, `Cmd::Move`, `Cmd::SetAfterQueue`, `AfterQueue`, `Status::queued`, `Order::insert`, `Order::remove`, `Order::relabel`, `order::moved`, `Session::play_queued`, `Session::enqueue`, `Session::queue_rows`, `Session::dequeue`, `Session::move_in_queue`, `Session::clear_queue`, `Session::drop_played`, `Outcome::Queued`, `Outcome::QueueReplaced`, `Outcome::QueueCleared`, `Action::Enqueue`, `Action::EnqueueAll`, `Action::ClearQueue`, `View::Queue`, `Cursors::queue`, `Frontend::searching`, `Model::queue`.

  One list holds both: the player plays the library with queued tracks spliced in after the track playing, each marked as queued, and a queued track leaves the list once played. Two lists would need the engine's gapless hand-over and play order to span them. Enter in the library used to make every library track look queued, and `e` appended after the whole library.

- `:prev-view`, on shift-tab, switches to the previous view, as tab switches to the next. The web page steps back from the library to the playlists, skipping the views it does not have.

### Changed

- Mark commands are named noun first, as `:loop N save` and `:roots rm` are: `:mark-undo`, `:mark-clear`, `:mark-next`, `:mark-prev`, `:mark-pick`, `:mark-nudge`, `:mark-move`, `:mark-snap` and `:mark-rm`, for `:unmark`, `:delmarks`, `:next-mark`, `:prev-mark`, `:pick`, `:nudge-mark`, `:move-mark`, `:snap-mark` and `:del-mark`. `:clear-search` is `:search-clear`, and `:next-view` and `:prev-view` are `:view next` and `:view prev`. `delmarks` and `del-mark` differed by a hyphen and a letter, and one cleared every mark. The old names still work, so bindings written for them keep working; help and completion list only the new.

### Fixed

- `:volume`, `:speed` and `:eq` read a number by one rule: a sign changes the value, no sign sets it, and `=` sets it too, so `=-3` sets a negative one. `:volume =60` was refused.

- `:sort` splits keys on spaces as well as commas, as `:columns` does, with `desc` or `asc` turning the key before it. `:sort tempo title` was read as one unknown column.

- In the window, a key a binding takes no longer also moves keyboard focus. Tab switched the view and focused the first control; shift-tab focused a control of the view it left.

## [0.12.0]

### Added

- Saved loops: up to 8 a track, kept in the library beside its marks. `:loop N`, on F1 to F8 in the sampler and numbered buttons in the window, saves the range to an empty slot and recalls a full one, making it the range and looping it; `:loop N save`, on shift-F1 to F8 or shift-click, saves over one, `:loop N clear` empties it, and `:loops clear`, or Clear loops in the window, empties them all after asking. One command both saving and recalling, by whether the slot is full, over separate save and load commands: a loop is set up once and recalled many times, so the common case takes one key. Pruning and forgetting a root remove a track's loops with its marks. The table needs no schema version, as `roots` did not. Library API: `db::query::loops`, `save_loop`, `clear_loop`, `clear_loops`; `Session::loops_for`, `save_loop`, `clear_loop`, `loops_to_clear`, `clear_loops`, `LOOP_SLOTS`, `Loops`; `Outcome::LoopSaved`, `LoopCleared`, `LoopsCleared`; `Refusal::NoLoops`; `Task::Loop`; `Confirm::ClearLoops`; `Action::ClearLoops`; `Snapshot::loops`; `Action::LoopSlot`, `SlotOp`; `Message::LoopRecalled`, `NoRangeToSave`.

### Changed

- The window's transport buttons show media symbols (U+23EE, U+23F8, U+23F9, U+23ED); each tooltip names the action and its key, and `transport_text_buttons = true` in `[gui]` shows words as before. Play and From start, a bar beside a play triangle, are painted: U+23F5 is small beside the other glyphs, and no font egui ships has U+29D0 or U+23FD. The window can no longer be narrowed below 800 by 480 points. The ReplayGain menu moved to Playback, ReplayGain, and the gain applied shows beside the format. The marks buttons now have a row of their own.

  At the default 1100 points, the ReplayGain menu was cut off: the row needed 1270 and never wrapped. egui wraps a row only before a widget wider than the space left, and a slider or combo box asks for exactly the space left.

- In the window's sampler, 10-point gaps separate the row of zoom and display controls, the rows that set the range, and the rows that slice it.

- The window's sampler waveform takes the height its controls leave: 36 points more at any window height. A fixed allowance for the controls, larger than they needed, left the rest empty above the transport.

- Subcommands that use no settings, such as `scan` and `search --json`, now read `settings.toml` and print its errors as warnings, then run. Before, a bad file went unnoticed until playr next played. Warn over fail, so a typo in a setting a script never uses does not break the script, and `playr devices` still works to fix `device`.

- In the window, a range's edge shows it can be dragged: the pointer turns to a left-right arrow over it. The reach grows from 5 to 8 points, for marks too, and a drag on an edge picks that edge, so `{` and `}` go on with it. Dragging an edge was already possible, but nothing showed it.

### Fixed

- A Windows path in double quotes in `settings.toml`, as `samples = "C:\Music"`, now gets an error saying to use single quotes or forward slashes. Before, it got toml's message about escapes, or, if every backslash was a valid escape, as in `"C:\new"`, "samples must be an absolute path". A path with a control character in it is now refused, so a half-escaped `"C:\\Music\temp"` is not used with a tab in it.

- A drag on the window's waveform moved what it dragged by as much as the view moved under it. The view moves while a drag is under way when it follows the playhead, and, with Fit range on, when the drag picks the other end to centre on: dragging the start after the end put the start a range's length outside the old range. The view now holds still until the drag ends.

- `playr-server` now refuses a `server.token` that other users can read or write, and says to `chmod 600` it. Only a token it created was made 0600; one copied or restored with wider permissions was used as found. Refuse over repairing the mode: a changed mode may mean others have already read the token.

- The README gave Rust 1.89 for building everything; `playr-gui` needs 1.95.

Releases 0.1.0 to 0.11.0 are in [CHANGELOG-archive.md](CHANGELOG-archive.md).
