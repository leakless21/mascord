//! Pure helpers mirroring [`songbird::tracks::TrackQueue`] `modify_queue` closures.
//! Lets us unit-test shuffle/move semantics without Discord or a live [`songbird::Call`].

use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::VecDeque;

/// Shuffle all entries after the first (keeps “now playing” fixed).
pub fn shuffle_queue_tail_keep_head<T>(q: &mut VecDeque<T>, rng: &mut impl Rng) {
    if q.len() <= 2 {
        return;
    }
    let mut tail: Vec<_> = q.drain(1..).collect();
    tail.shuffle(rng);
    for t in tail {
        q.push_back(t);
    }
}

/// Move the item at `from_i` to `to_i` using adjacent swaps (same behavior as `/move`).
pub fn move_by_adjacent_swaps<T>(q: &mut VecDeque<T>, from_i: usize, to_i: usize) {
    if from_i >= q.len() || to_i >= q.len() || from_i == to_i {
        return;
    }
    if from_i < to_i {
        for i in from_i..to_i {
            q.swap(i, i + 1);
        }
    } else {
        for i in (to_i..from_i).rev() {
            q.swap(i, i + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn shuffle_tail_preserves_head_and_permutation() {
        let mut q = VecDeque::from([10u8, 20, 30, 40]);
        let mut rng = StdRng::seed_from_u64(42);
        shuffle_queue_tail_keep_head(&mut q, &mut rng);
        assert_eq!(q[0], 10);
        let mut got: Vec<_> = q.iter().copied().collect();
        got.sort_unstable();
        assert_eq!(got, vec![10, 20, 30, 40]);
    }

    #[test]
    fn shuffle_tail_noop_when_len_le_2() {
        let mut q = VecDeque::from([1u8, 2]);
        let mut rng = StdRng::seed_from_u64(1);
        shuffle_queue_tail_keep_head(&mut q, &mut rng);
        assert_eq!(Vec::from(q), vec![1, 2]);
    }

    #[test]
    fn move_forward() {
        let mut q = VecDeque::from([1u8, 2, 3, 4, 5]);
        move_by_adjacent_swaps(&mut q, 0, 2);
        assert_eq!(Vec::from(q), vec![2, 3, 1, 4, 5]);
    }

    #[test]
    fn move_backward() {
        let mut q = VecDeque::from([1u8, 2, 3, 4, 5]);
        move_by_adjacent_swaps(&mut q, 4, 1);
        assert_eq!(Vec::from(q), vec![1, 5, 2, 3, 4]);
    }
}
