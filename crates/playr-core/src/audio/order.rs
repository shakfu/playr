//! Play order: which track follows which, in each playback mode.
//!
//! The list itself never changes order. Shuffle keeps a permutation of its
//! indices beside it, so the list on screen stays as it was and shuffle can be
//! turned off without losing the original order.

/// How playback moves through the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// In list order, stopping at the end.
    #[default]
    Normal,
    /// Every track once per pass in random order, reshuffling at each pass's end.
    Shuffle,
    /// In list order, starting again after the last track.
    Repeat,
    /// The current track, over and over.
    RepeatOne,
}

impl Mode {
    const CYCLE: [Mode; 4] = [Mode::Normal, Mode::Shuffle, Mode::Repeat, Mode::RepeatOne];

    /// Each mode's name in commands and settings.
    pub const NAMES: [(&'static str, Mode); 4] = [
        ("normal", Mode::Normal),
        ("shuffle", Mode::Shuffle),
        ("repeat", Mode::Repeat),
        ("repeat-one", Mode::RepeatOne),
    ];

    fn position(self) -> usize {
        Self::CYCLE.iter().position(|m| *m == self).unwrap_or(0)
    }

    /// The mode after this one, wrapping.
    pub fn next(self) -> Mode {
        Self::CYCLE[(self.position() + 1) % Self::CYCLE.len()]
    }

    /// The mode before this one, wrapping.
    pub fn prev(self) -> Mode {
        Self::CYCLE[(self.position() + Self::CYCLE.len() - 1) % Self::CYCLE.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Shuffle => "shuffle",
            Mode::Repeat => "repeat",
            Mode::RepeatOne => "repeat one",
        }
    }
}

/// The order a list of `len` tracks plays in.
pub struct Order {
    mode: Mode,
    /// List indices in play order. List order unless shuffling.
    seq: Vec<usize>,
    /// `pos[i]` is where index `i` sits in `seq`.
    pos: Vec<usize>,
    /// xorshift64* state. Never zero.
    rng: u64,
}

impl Order {
    /// The order for a list of `len` tracks that starts at `first`.
    pub fn new(len: usize, mode: Mode, first: usize, seed: u64) -> Self {
        let mut order = Order {
            mode,
            seq: Vec::new(),
            pos: Vec::new(),
            rng: mix(seed),
        };
        order.arrange(len, first);
        order
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Switches mode with `current` playing. A new shuffle starts from it, so
    /// the rest of the list plays after it; leaving shuffle continues in list
    /// order from it.
    pub fn set_mode(&mut self, mode: Mode, current: usize) {
        self.mode = mode;
        self.arrange(self.seq.len(), current);
    }

    /// The track to play after `i`, or `None` at the end of the list.
    ///
    /// `skipping` is set for an explicit move to the next track and for stepping
    /// over one that will not play. Repeat-one then moves on rather than
    /// replaying the same track.
    pub fn successor(&mut self, i: usize, skipping: bool) -> Option<usize> {
        let len = self.seq.len();
        if i >= len {
            return None;
        }
        match self.mode {
            Mode::Normal => (i + 1 < len).then_some(i + 1),
            Mode::RepeatOne if !skipping => Some(i),
            Mode::Repeat | Mode::RepeatOne => Some((i + 1) % len),
            Mode::Shuffle => {
                let p = self.pos[i] + 1;
                if p < len {
                    return Some(self.seq[p]);
                }
                // A new pass, which must not open with the track that closed the last.
                self.shuffle_from(0);
                if len > 1 && self.seq[0] == i {
                    let j = 1 + self.below(len - 1);
                    self.seq.swap(0, j);
                }
                self.index_positions();
                Some(self.seq[0])
            }
        }
    }

    /// The track to play before `i`, or `None` at the start. In shuffle that
    /// is the one played before it in this pass.
    pub fn predecessor(&self, i: usize) -> Option<usize> {
        let len = self.seq.len();
        if i >= len {
            return None;
        }
        match self.mode {
            Mode::Normal => i.checked_sub(1),
            Mode::Repeat | Mode::RepeatOne => Some((i + len - 1) % len),
            Mode::Shuffle => self.pos[i].checked_sub(1).map(|p| self.seq[p]),
        }
    }

    /// Takes in tracks appended to the list, which now holds `len`. When
    /// shuffling they follow the rest of this pass, in random order.
    pub fn extend(&mut self, len: usize) {
        let old = self.seq.len();
        if len <= old {
            return;
        }
        self.seq.extend(old..len);
        if self.mode == Mode::Shuffle {
            self.shuffle_from(old);
        }
        self.index_positions();
    }

    fn arrange(&mut self, len: usize, first: usize) {
        self.seq = (0..len).collect();
        if self.mode == Mode::Shuffle && len > 0 {
            self.seq.swap(0, first.min(len - 1));
            self.shuffle_from(1);
        }
        self.index_positions();
    }

    /// Fisher-Yates over `seq[start..]`.
    fn shuffle_from(&mut self, start: usize) {
        for i in (start + 1..self.seq.len()).rev() {
            let j = start + self.below(i - start + 1);
            self.seq.swap(i, j);
        }
    }

    fn index_positions(&mut self) {
        self.pos = vec![0; self.seq.len()];
        for (p, &i) in self.seq.iter().enumerate() {
            self.pos[i] = p;
        }
    }

    /// A number below `n`. The modulo bias is below 1 in 2^40 for any list
    /// this player will hold.
    fn below(&mut self, n: usize) -> usize {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) % n as u64) as usize
    }
}

/// SplitMix64 of `seed`, never zero. Using the seed directly would make small or
/// adjacent seeds start xorshift from nearly the same state.
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).max(1)
}

/// A seed that differs between runs, from the standard library's hasher keys.
pub fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}
