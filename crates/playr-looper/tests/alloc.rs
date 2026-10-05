//! The callback never allocates or frees: a counting allocator, armed on
//! this thread only, wraps `Looper::process`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use playr_looper::{Filter, Loop, Returned, Setting, Window};

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
    let (mut l, mut h) = playr_looper::new(sr);
    let lp = |v: f32| {
        Loop::new(vec![v; 9600 * 2], 2)
            .unwrap()
            .with_range(Window::new(2400, 7200))
            .unwrap()
    };
    h.load(lp(0.3)).unwrap();
    for s in [
        Setting::Play,
        Setting::Write(true),
        Setting::WriteWindow(Window::new(2400, 7200)),
        Setting::Feedback(0.5),
        Setting::Thin(0.4),
        Setting::Wear(0.3),
        Setting::Send(0, 0.8),
        Setting::Drive(0, 0.5),
        Setting::Filter(0, Filter::Band),
        Setting::On(1, true),
        Setting::Rate(1, -1.3),
        Setting::Ping(1, true),
    ] {
        h.set(s).unwrap();
    }
    let mut out = vec![0.0; 512 * 2];
    // Commands queued before each run, applied inside it: a snapshot, a
    // load and a write window that aborts nothing.
    h.snapshot().unwrap();
    h.load(lp(-0.3)).unwrap();
    h.set(Setting::Cutoff(0, 0.2)).unwrap();
    let n = counted(|| {
        for _ in 0..400 {
            l.process(&mut out);
        }
    });
    assert_eq!(n, 0);
    // What it handed back reached the handle.
    let mut back = Vec::new();
    while let Some(r) = h.poll() {
        back.push(matches!(r, Returned::Loop(_)));
    }
    assert!(back.contains(&true), "the replaced loop came back");
}
