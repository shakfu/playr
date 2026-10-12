//! The tape as kept between runs, when `persist` names `tape`: every voice's
//! settings and what it reads, by reference. A write loop that was printed
//! into is kept as its shape alone, and resumes silent. Design in
//! `docs/dev/tape-machine-v2-design.md`, section 7.

use std::fmt::Write as _;
use std::path::PathBuf;

use playr_looper::{Window, VOICES, WRITER};

use super::{
    model_name, Division, FxState, Layout, Source, TapeState, DELAY_MODELS, REVERB_MODELS,
};

/// The longest write loop a saved shape may ask for, in seconds: bounds what
/// a damaged row can make playr allocate.
const LONGEST: usize = 3600;

/// What a voice reads, as kept.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Reads {
    #[default]
    Empty,
    /// The write loop; for voice 4, a silent one of [`SavedTape::shape`].
    Write,
    /// A loop read from a file.
    File(Source),
}

/// The tape's settings, and what each voice reads.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedTape {
    /// The tape's rate, which the windows count frames at.
    pub(super) rate: u32,
    /// Writing is kept off: a resumed tape that wrote at once would print
    /// over the write loop before it was heard.
    pub(super) state: TapeState,
    pub(super) reads: [Reads; VOICES],
    /// The silent write loop's frames, range and channels.
    pub(super) shape: Option<(usize, Window, u16)>,
    pub(super) layout: Layout,
}

impl SavedTape {
    /// How many voices read a loop from a file.
    pub fn files(&self) -> usize {
        self.reads
            .iter()
            .filter(|r| matches!(r, Reads::File(_)))
            .count()
    }

    /// The settings kept.
    pub fn state(&self) -> &TapeState {
        &self.state
    }

    /// What each voice reads.
    pub fn reads(&self) -> &[Reads; VOICES] {
        &self.reads
    }

    /// Whether the tape was playing when kept.
    pub fn playing(&self) -> bool {
        self.state.playing
    }

    /// As text, a row a line.
    pub fn encode(&self) -> String {
        let s = &self.state;
        let mut text = format!("rate {}\nplaying {}\n", self.rate, bit(s.playing));
        let w = s.write_window;
        let _ = writeln!(
            text,
            "write {} {} {} {} {}",
            s.feedback, s.wear, s.thin, w.start, w.end
        );
        if let Some((frames, range, channels)) = self.shape {
            let _ = writeln!(
                text,
                "shape {frames} {} {} {channels}",
                range.start, range.end
            );
        }
        for (i, v) in s.voices.iter().enumerate() {
            let _ = writeln!(
                text,
                "voice {i} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                bit(v.on),
                v.rate,
                v.window.start,
                v.window.end,
                v.level,
                v.pan,
                v.send,
                v.wear,
                v.fade,
                bit(v.ping),
                v.slew,
                v.drive,
                v.high_pass,
                v.low_pass,
                v.bell_hz,
                v.bell_gain,
                v.bell_q,
                bit(v.solo),
                bit(v.mute),
                v.delay,
                v.reverb,
            );
        }
        let fx = &s.fx;
        let _ = writeln!(
            text,
            "fx {} {} {} {} {} {} {} {} {} {} {} {} {}",
            model_name(&DELAY_MODELS, fx.delay_model),
            fx.time,
            fx.sync.map_or("-", Division::name),
            fx.feedback,
            fx.tone,
            fx.wow,
            fx.delay_return,
            model_name(&REVERB_MODELS, fx.reverb_model),
            fx.size,
            fx.decay,
            fx.damp,
            fx.reverb_return,
            fx.print,
        );
        // A lane folded or unfolded by hand, else "-"; then whose drawers are open.
        let folded = self
            .layout
            .folded
            .map(|f| f.map_or("-", |f| if f { "1" } else { "0" }));
        let drawer = self.layout.drawer.map(|d| bit(d).to_string());
        let _ = writeln!(
            text,
            "lanes {} {} {}",
            folded.join(" "),
            drawer.join(" "),
            bit(self.layout.fx)
        );
        for (i, r) in self.reads.iter().enumerate() {
            let _ = match r {
                Reads::Empty => writeln!(text, "reads {i} empty"),
                Reads::Write => writeln!(text, "reads {i} write"),
                // The path is last, as it may hold spaces.
                Reads::File(src) => writeln!(
                    text,
                    "reads {i} file {} {} {} {} {} {}",
                    bit(src.circular),
                    src.rate,
                    src.span.0,
                    src.span.1,
                    src.beats.map_or("-".into(), |b| b.to_string()),
                    src.path.to_string_lossy(),
                ),
            };
        }
        text
    }

    /// From [`SavedTape::encode`]'s text. A row that does not read is left at
    /// its default; text with no rate, or nothing read from a file and no
    /// write loop, is `None`.
    pub fn decode(text: &str) -> Option<SavedTape> {
        let mut rate = None;
        let mut state = TapeState::new(Window::new(0, 0));
        state.voices[0].on = false;
        let mut reads: [Reads; VOICES] = Default::default();
        let mut shape = None;
        let mut layout = Layout::default();
        for line in text.lines() {
            let words: Vec<&str> = line.split(' ').collect();
            match words.as_slice() {
                ["rate", r] => rate = r.parse::<u32>().ok().filter(|r| *r > 0),
                ["playing", b] => state.playing = flag(b).unwrap_or(false),
                ["write", fb, wear, thin, a, b] => {
                    if let (Some(fb), Some(wear), Some(thin), Some(w)) =
                        (num(fb), num(wear), num(thin), window(a, b))
                    {
                        (state.feedback, state.wear, state.thin) = (fb, wear, thin);
                        state.write_window = w;
                    }
                }
                ["shape", frames, a, b, c] => {
                    let frames = frames.parse::<usize>().ok();
                    let channels = c.parse::<u16>().ok().filter(|c| (1..=2).contains(c));
                    if let (Some(f), Some(w), Some(c)) = (frames, window(a, b), channels) {
                        shape = (w.end <= f).then_some((f, w, c));
                    }
                }
                ["voice", i, rest @ ..] => {
                    if let (Some(i), Some(v)) = (index(i), voice(rest)) {
                        state.voices[i] = v;
                    }
                }
                ["fx", rest @ ..] => {
                    if let Some(fx) = fx(rest) {
                        state.fx = fx;
                    }
                }
                // Before the FX lane, rows had no word for it.
                ["lanes", rest @ ..] if [2 * VOICES, 2 * VOICES + 1].contains(&rest.len()) => {
                    let folded: Option<Vec<Option<bool>>> = rest[..VOICES]
                        .iter()
                        .map(|w| match *w {
                            "-" => Some(None),
                            w => flag(w).map(Some),
                        })
                        .collect();
                    let drawer: Option<Vec<bool>> =
                        rest[VOICES..2 * VOICES].iter().map(|w| flag(w)).collect();
                    let fx = rest.get(2 * VOICES).map_or(Some(false), |w| flag(w));
                    if let (Some(f), Some(d), Some(x)) = (folded, drawer, fx) {
                        layout.folded.copy_from_slice(&f);
                        layout.drawer.copy_from_slice(&d);
                        layout.fx = x;
                    }
                }
                ["reads", i, "empty"] => {
                    if let Some(i) = index(i) {
                        reads[i] = Reads::Empty;
                    }
                }
                ["reads", i, "write"] => {
                    if let Some(i) = index(i) {
                        reads[i] = Reads::Write;
                    }
                }
                ["reads", i, "file", circular, r, a, b, beats, path @ ..] => {
                    let path = path.join(" ");
                    let src = (|| {
                        let (start, end) = (a.parse::<u64>().ok()?, b.parse::<u64>().ok()?);
                        let beats = match *beats {
                            "-" => None,
                            b => Some(b.parse::<u32>().ok()?),
                        };
                        let path = PathBuf::from(path);
                        if start >= end || path.as_os_str().is_empty() {
                            return None;
                        }
                        Some(Source {
                            name: playr_core::samples::name_for(&path),
                            path,
                            rate: r.parse::<u32>().ok().filter(|r| *r > 0)?,
                            span: (start, end),
                            circular: flag(circular)?,
                            voice: None,
                            beats,
                        })
                    })();
                    if let (Some(i), Some(src)) = (index(i), src) {
                        reads[i] = Reads::File(src);
                    }
                }
                _ => {}
            }
        }
        let rate = rate?;
        let shape = shape.filter(|(f, ..)| *f <= LONGEST * rate as usize);
        // Voice 4 reads the write loop alone, and a silent one needs a shape.
        if matches!(reads[WRITER], Reads::Write) && shape.is_none() {
            reads[WRITER] = Reads::Empty;
        }
        let write = !matches!(reads[WRITER], Reads::Empty);
        for (i, r) in reads.iter_mut().enumerate() {
            if i != WRITER && *r == Reads::Write && !write {
                *r = Reads::Empty;
            }
        }
        for (i, r) in reads.iter().enumerate() {
            state.own[i] = i != WRITER && matches!(r, Reads::File(_));
            state.empty[i] = *r == Reads::Empty;
        }
        state.write = false;
        let shape = shape.filter(|_| reads[WRITER] == Reads::Write);
        let saved = SavedTape {
            rate,
            state,
            reads,
            shape,
            layout,
        };
        (saved.files() > 0 || saved.shape.is_some()).then_some(saved)
    }
}

fn bit(b: bool) -> u8 {
    u8::from(b)
}

fn flag(word: &str) -> Option<bool> {
    match word {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn num(word: &str) -> Option<f32> {
    word.parse::<f32>().ok().filter(|v| v.is_finite())
}

fn index(word: &str) -> Option<usize> {
    word.parse::<usize>().ok().filter(|i| *i < VOICES)
}

fn window(a: &str, b: &str) -> Option<Window> {
    let (a, b) = (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?);
    (a <= b).then(|| Window::new(a, b))
}

/// A voice's settings, from the words after its index.
fn voice(words: &[&str]) -> Option<super::VoiceState> {
    // Before the effects, rows ended at mute: their sends are 0.
    let (words, sends) = match words.len() {
        19 => (words, ["0", "0"]),
        21 => (&words[..19], [words[19], words[20]]),
        _ => return None,
    };
    let [on, rate, a, b, level, pan, send, wear, fade, ping, slew, drive, hp, lp, bell, gain, q, solo, mute] =
        words
    else {
        return None;
    };
    let [delay, reverb] = sends;
    Some(super::VoiceState {
        on: flag(on)?,
        rate: num(rate)?,
        window: window(a, b)?,
        level: num(level)?,
        pan: num(pan)?,
        send: num(send)?,
        wear: num(wear)?,
        fade: num(fade)?,
        ping: flag(ping)?,
        slew: num(slew)?,
        drive: num(drive)?,
        high_pass: num(hp)?,
        low_pass: num(lp)?,
        bell_hz: num(bell)?,
        bell_gain: num(gain)?,
        bell_q: num(q)?,
        solo: flag(solo)?,
        mute: flag(mute)?,
        delay: num(delay)?,
        reverb: num(reverb)?,
    })
}

/// The model `table` names `word`.
fn named<M: Copy>(table: &[(&str, M)], word: &str) -> Option<M> {
    table.iter().find(|m| m.0 == word).map(|m| m.1)
}

/// The effects' settings, from the words after `fx`.
fn fx(words: &[&str]) -> Option<FxState> {
    let [dm, time, sync, fb, tone, wow, dr, rm, size, decay, damp, rr, print] = words else {
        return None;
    };
    Some(FxState {
        delay_model: named(&DELAY_MODELS, dm)?,
        time: num(time)?,
        sync: match *sync {
            "-" => None,
            w => Some(*Division::ALL.iter().find(|d| d.name() == w)?),
        },
        feedback: num(fb)?,
        tone: num(tone)?,
        wow: num(wow)?,
        delay_return: num(dr)?,
        reverb_model: named(&REVERB_MODELS, rm)?,
        size: num(size)?,
        decay: num(decay)?,
        damp: num(damp)?,
        reverb_return: num(rr)?,
        print: num(print)?,
    })
}
