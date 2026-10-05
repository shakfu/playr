//! The callback never allocates or frees: a counting allocator, armed on
//! this thread only, wraps `Engine::process`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use playr_dj::{Band, Grid, Returned, Setting, Side, Track};

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    if ARMED.with(Cell::get) {
        COUNT.with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        count();
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// Allocations and frees on this thread while `f` runs.
fn counted(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    COUNT.with(Cell::get)
}

#[test]
fn the_callback_neither_allocates_nor_frees() {
    assert!(counted(|| drop(vec![0u8; 8])) > 0, "the counter counts");
    let sr = 48_000;
    let (mut e, mut h) = playr_dj::new(sr);
    let track = |v: f32| {
        let grid = Grid::new(120.0, 0.0).unwrap();
        Track::new(vec![v; 48_000 * 2], 2, Some(grid))
            .unwrap()
            .with_hot_cues([Some(1000.0), None, None, None])
    };
    h.load(Side::A, track(0.3)).unwrap();
    h.load(Side::B, track(0.2)).unwrap();
    for s in [
        Setting::Play(Side::A),
        Setting::Play(Side::B),
        Setting::Sync(Side::B, true),
        Setting::Eq(Side::A, Band::Low, 4.0),
        Setting::Filter(Side::B, -0.6),
        Setting::CueBus(Some(Side::B)),
        Setting::Quantize(true),
    ] {
        h.set(s).unwrap();
    }
    let mut out = vec![0.0; 512 * 2];
    // Applied inside the run: a loop, a hot cue, a pause and a load.
    h.set(Setting::Loop(Side::A, Some(2.0))).unwrap();
    h.set(Setting::HotCue(Side::A, 0)).unwrap();
    h.set(Setting::Pause(Side::B)).unwrap();
    h.load(Side::B, track(-0.2)).unwrap();
    let n = counted(|| {
        for _ in 0..400 {
            e.process(&mut out);
        }
    });
    assert_eq!(n, 0);
    let mut replaced = 0;
    while let Some(r) = h.poll() {
        replaced += matches!(r, Returned::Replaced(..)) as usize;
    }
    assert!(replaced >= 3, "{replaced}");
}
