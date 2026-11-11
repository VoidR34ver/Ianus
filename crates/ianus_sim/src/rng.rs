//! Deterministic random numbers.
//!
//! xoshiro256** seeded through SplitMix64. Integer-only, no OS entropy, no
//! global state: the same seed gives the same stream on every platform, which
//! is what makes replays and shared bug reports possible.
//!
//! Never use `rand::thread_rng()` or the system clock inside the simulation.

/// A small, fast, seedable generator. `Copy` on purpose: cheap to snapshot
/// into saves and replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rng {
    s: [u64; 4],
}

/// One step of SplitMix64; used to turn a single seed into good state.
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    pub const fn new(seed: u64) -> Self {
        let mut sm = seed;
        let s = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { s }
    }

    /// An independent stream derived from this seed, for one subsystem (so
    /// adding a random call in pathfinding does not shift the dice in combat).
    pub const fn fork(&self, stream: u64) -> Self {
        let mut sm =
            self.s[0] ^ self.s[3].rotate_left(17) ^ stream.wrapping_mul(0xd6e8_feb8_6659_fd93);
        let s = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { s }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `0..n` without modulo bias (Lemire's method). `n == 0`
    /// returns 0.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let mut m = u64::from(self.next_u32()) * u64::from(n);
        let mut low = m as u32;
        if low < n {
            let threshold = n.wrapping_neg() % n;
            while low < threshold {
                m = u64::from(self.next_u32()) * u64::from(n);
                low = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// Uniform in `lo..hi`. Returns `lo` if the range is empty.
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        let span = (i64::from(hi) - i64::from(lo)) as u64;
        // Spans wider than u32 cannot occur: i32 differences fit in u32 + 1.
        let span = span.min(u64::from(u32::MAX)) as u32;
        lo.wrapping_add(self.below(span) as i32)
    }

    /// True with probability `num / den`.
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }

    /// Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    /// Known-answer test. If this changes, every saved replay breaks.
    /// Values cross-checked against an independent implementation of
    /// xoshiro256** seeded with SplitMix64.
    #[test]
    fn known_answer_vector() {
        let mut r = Rng::new(0);
        let got = [r.next_u64(), r.next_u64(), r.next_u64()];
        assert_eq!(
            got,
            [
                0x99ec_5f36_cb75_f2b4,
                0xbf6e_1f78_4956_452a,
                0x1a5f_849d_4933_e6e0
            ]
        );
    }

    #[test]
    fn forks_are_independent_of_each_other() {
        let root = Rng::new(7);
        let mut a = root.fork(1);
        let mut b = root.fork(2);
        assert_ne!(a.next_u64(), b.next_u64());
        // Drawing from one fork must not disturb another.
        let mut b2 = root.fork(2);
        let _ = a.next_u64();
        let mut b_again = root.fork(2);
        assert_eq!(b2.next_u64(), b_again.next_u64());
    }

    #[test]
    fn below_is_in_range_and_roughly_uniform() {
        let mut r = Rng::new(99);
        let mut buckets = [0u32; 6];
        for _ in 0..60_000 {
            let v = r.below(6);
            assert!(v < 6);
            buckets[v as usize] += 1;
        }
        // Expected 10_000 each; allow a generous 5% band.
        for (i, &c) in buckets.iter().enumerate() {
            assert!((9_500..=10_500).contains(&c), "bucket {i} has {c}");
        }
    }

    #[test]
    fn below_zero_and_empty_ranges_are_safe() {
        let mut r = Rng::new(1);
        assert_eq!(r.below(0), 0);
        assert_eq!(r.range_i32(5, 5), 5);
        assert_eq!(r.range_i32(5, 3), 5);
    }

    #[test]
    fn range_covers_negative_and_extreme_bounds() {
        let mut r = Rng::new(3);
        for _ in 0..1000 {
            let v = r.range_i32(-10, 10);
            assert!((-10..10).contains(&v));
        }
        let v = r.range_i32(i32::MIN, i32::MAX);
        assert!(v < i32::MAX);
    }

    #[test]
    fn shuffle_is_a_permutation_and_repeatable() {
        let mut a: Vec<u32> = (0..50).collect();
        let mut b = a.clone();
        Rng::new(5).shuffle(&mut a);
        Rng::new(5).shuffle(&mut b);
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(a, (0..50).collect::<Vec<_>>());
    }
}
