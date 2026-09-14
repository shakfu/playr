//! Play order in each mode, without an engine.

use playr_core::audio::order::{Mode, Order};

/// The next `n` tracks from `first`, following natural track ends.
fn run(order: &mut Order, first: usize, n: usize) -> Vec<usize> {
    let mut out = vec![first];
    let mut i = first;
    for _ in 1..n {
        match order.successor(i, false) {
            Some(next) => {
                out.push(next);
                i = next;
            }
            None => break,
        }
    }
    out
}

#[test]
fn modes_cycle_forward_and_back() {
    let mut m = Mode::Normal;
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(m);
        m = m.next();
    }
    assert_eq!(
        seen,
        [Mode::Normal, Mode::Shuffle, Mode::Repeat, Mode::RepeatOne]
    );
    assert_eq!(m, Mode::Normal, "the cycle does not wrap");
    assert_eq!(Mode::Normal.prev(), Mode::RepeatOne);
    for m in seen {
        assert_eq!(m.next().prev(), m);
    }
}

#[test]
fn normal_plays_in_order_and_stops() {
    let mut order = Order::new(4, Mode::Normal, 1, 7);
    assert_eq!(run(&mut order, 1, 10), [1, 2, 3]);
    assert_eq!(order.predecessor(1), Some(0));
    assert_eq!(order.predecessor(0), None);
}

#[test]
fn repeat_wraps_both_ways() {
    let mut order = Order::new(3, Mode::Repeat, 0, 7);
    assert_eq!(run(&mut order, 1, 5), [1, 2, 0, 1, 2]);
    assert_eq!(order.predecessor(0), Some(2));
}

#[test]
fn repeat_one_replays_unless_skipping() {
    let mut order = Order::new(3, Mode::RepeatOne, 2, 7);
    assert_eq!(run(&mut order, 2, 4), [2, 2, 2, 2]);
    assert_eq!(
        order.successor(2, true),
        Some(0),
        "skipping did not move on"
    );
    assert_eq!(order.predecessor(0), Some(2));
}

#[test]
fn shuffle_plays_each_track_once_per_pass_starting_where_asked() {
    for seed in 1..50 {
        let len = 7;
        let mut order = Order::new(len, Mode::Shuffle, 4, seed);
        let played = run(&mut order, 4, len * 3);
        assert_eq!(
            played[0], 4,
            "seed {seed}: did not start at the chosen track"
        );
        for pass in played.chunks(len) {
            let mut sorted = pass.to_vec();
            sorted.sort();
            assert_eq!(
                sorted,
                (0..len).collect::<Vec<_>>(),
                "seed {seed}: pass {pass:?}"
            );
        }
        for w in played.windows(2) {
            assert_ne!(w[0], w[1], "seed {seed}: a track played twice in a row");
        }
    }
}

#[test]
fn shuffle_differs_between_seeds() {
    let orders: std::collections::HashSet<Vec<usize>> = (1..20)
        .map(|seed| run(&mut Order::new(8, Mode::Shuffle, 0, seed), 0, 8))
        .collect();
    assert!(
        orders.len() > 10,
        "only {} distinct orders from 19 seeds",
        orders.len()
    );
}

#[test]
fn shuffle_steps_back_through_the_pass() {
    let mut order = Order::new(5, Mode::Shuffle, 3, 11);
    let played = run(&mut order, 3, 5);
    for w in played.windows(2) {
        assert_eq!(order.predecessor(w[1]), Some(w[0]));
    }
    assert_eq!(
        order.predecessor(3),
        None,
        "the first track of the pass has a predecessor"
    );
}

#[test]
fn switching_mode_keeps_the_current_track() {
    let mut order = Order::new(6, Mode::Normal, 0, 5);
    order.set_mode(Mode::Shuffle, 2);
    let played = run(&mut order, 2, 6);
    let mut sorted = played.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [0, 1, 2, 3, 4, 5],
        "the rest did not follow the current track"
    );

    order.set_mode(Mode::Normal, 4);
    assert_eq!(order.successor(4, false), Some(5));
}

#[test]
fn appended_tracks_join_the_order() {
    let mut order = Order::new(3, Mode::Normal, 0, 3);
    order.extend(5);
    assert_eq!(run(&mut order, 0, 10), [0, 1, 2, 3, 4]);

    let mut order = Order::new(3, Mode::Shuffle, 0, 3);
    let first_pass = run(&mut order, 0, 3);
    order.extend(5);
    // The pass carries on into the appended tracks before reshuffling.
    let last = *first_pass.last().unwrap();
    let more = run(&mut order, last, 3);
    let mut appended = more[1..].to_vec();
    appended.sort();
    assert_eq!(appended, [3, 4]);
}

#[test]
fn an_empty_or_single_list_does_not_panic() {
    let mut empty = Order::new(0, Mode::Shuffle, 0, 1);
    assert_eq!(empty.successor(0, false), None);
    assert_eq!(empty.predecessor(0), None);
    let mut one = Order::new(1, Mode::Shuffle, 0, 1);
    assert_eq!(run(&mut one, 0, 3), [0, 0, 0]);
}
