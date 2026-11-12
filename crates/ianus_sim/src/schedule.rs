//! Which objects update on which tick.
//!
//! Every object has a tick rate, `ob_tick`: it updates once every `ob_tick`
//! ticks. `1` is every tick, `60` is once a second, `0` is never (a wall).
//!
//! The point is that an object which is not due costs **nothing**: it is not
//! looked at, not even to decide to skip it. Objects are filed by rate into
//! "lanes"; a lane with rate `n` has `n` buckets, and on tick `t` only bucket
//! `t % n` is walked. Objects with the same rate are dealt round-robin across
//! the buckets, so a thousand objects at rate 10 become a hundred per tick
//! instead of a thousand every tenth tick.
//!
//! Deterministic: which bucket an object lands in depends only on the order
//! objects were inserted, and due objects come out in a fixed order.

/// Longest allowed interval (one hour at 60 ticks per second). Keeps a typo
/// like `ob_tick = 4000000000` from allocating four billion buckets.
pub const MAX_OB_TICK: u32 = 216_000;

#[derive(Clone, Debug, Default)]
struct Lane {
    every: u32,
    /// `buckets[phase]` holds the ids due when `tick % every == phase`.
    buckets: Vec<Vec<u32>>,
    /// Next bucket to deal into.
    next: u32,
}

/// Files object ids by tick rate and hands back the ones due each tick.
#[derive(Clone, Debug, Default)]
pub struct Scheduler {
    lanes: Vec<Lane>,
    scheduled: usize,
    never: usize,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything, keeping allocations for reuse.
    pub fn clear(&mut self) {
        for lane in &mut self.lanes {
            for bucket in &mut lane.buckets {
                bucket.clear();
            }
            lane.next = 0;
        }
        self.scheduled = 0;
        self.never = 0;
    }

    /// Schedule `id` to update every `ob_tick` ticks. `0` means never. Values
    /// above [`MAX_OB_TICK`] are clamped to it.
    pub fn insert(&mut self, id: u32, ob_tick: u32) {
        if ob_tick == 0 {
            self.never += 1;
            return;
        }
        let every = ob_tick.min(MAX_OB_TICK);
        let lane = match self.lanes.iter().position(|l| l.every == every) {
            Some(i) => &mut self.lanes[i],
            None => {
                // Keep lanes sorted by rate so the order objects come out in
                // does not depend on which rate happened to be seen first.
                let at = self.lanes.partition_point(|l| l.every < every);
                self.lanes.insert(
                    at,
                    Lane {
                        every,
                        buckets: vec![Vec::new(); every as usize],
                        next: 0,
                    },
                );
                &mut self.lanes[at]
            }
        };
        let phase = lane.next;
        lane.buckets[phase as usize].push(id);
        lane.next = (phase + 1) % every;
        self.scheduled += 1;
    }

    /// Call `f(id, elapsed)` for every object due on `tick`. `elapsed` is how
    /// many ticks have passed since that object's last update, so its logic
    /// can do that much work in one go. An object's first update counts from
    /// tick 0, so it gets `tick + 1`, not the full interval; from then on it is
    /// the interval. Over any run, the elapsed values add up to the ticks run.
    #[inline]
    pub fn due(&self, tick: u64, mut f: impl FnMut(u32, u32)) {
        for lane in &self.lanes {
            let phase = (tick % u64::from(lane.every)) as usize;
            // First time round (tick below the interval) the object has only
            // existed for `tick + 1` ticks.
            let elapsed = if tick < u64::from(lane.every) {
                tick as u32 + 1
            } else {
                lane.every
            };
            for &id in &lane.buckets[phase] {
                f(id, elapsed);
            }
        }
    }

    /// How many objects are due on `tick`, without visiting them.
    pub fn due_count(&self, tick: u64) -> usize {
        self.lanes
            .iter()
            .map(|l| l.buckets[(tick % u64::from(l.every)) as usize].len())
            .sum()
    }

    /// Objects that update at some rate.
    pub fn scheduled(&self) -> usize {
        self.scheduled
    }

    /// Objects with `ob_tick = 0`, which never update.
    pub fn never(&self) -> usize {
        self.never
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn due_ids(s: &Scheduler, tick: u64) -> Vec<u32> {
        let mut v = Vec::new();
        s.due(tick, |id, _| v.push(id));
        v
    }

    #[test]
    fn rate_one_updates_every_tick() {
        let mut s = Scheduler::new();
        s.insert(7, 1);
        for t in 0..50 {
            assert_eq!(due_ids(&s, t), vec![7]);
        }
    }

    #[test]
    fn rate_zero_never_updates_and_is_counted() {
        let mut s = Scheduler::new();
        s.insert(1, 0);
        s.insert(2, 0);
        for t in 0..100 {
            assert!(due_ids(&s, t).is_empty());
        }
        assert_eq!((s.scheduled(), s.never()), (0, 2));
    }

    #[test]
    fn every_object_updates_exactly_once_per_interval() {
        let mut s = Scheduler::new();
        let rates = [1u32, 2, 3, 7, 10, 60];
        let mut id = 0;
        for &r in &rates {
            for _ in 0..25 {
                s.insert(id, r);
                id += 1;
            }
        }
        // Over 420 ticks (a multiple of every rate) each object must come up
        // exactly 420 / rate times, and always `rate` ticks apart.
        let span = 420u64;
        let mut last = vec![None::<u64>; id as usize];
        let mut count = vec![0u64; id as usize];
        for t in 0..span {
            s.due(t, |i, every| {
                if let Some(prev) = last[i as usize] {
                    assert_eq!(t - prev, u64::from(every), "object {i} irregular");
                }
                last[i as usize] = Some(t);
                count[i as usize] += 1;
            });
        }
        for (i, &c) in count.iter().enumerate() {
            let rate = rates[i / 25];
            assert_eq!(c, span / u64::from(rate), "object {i} at rate {rate}");
        }
    }

    #[test]
    fn elapsed_ticks_add_up_to_exactly_the_ticks_that_ran() {
        // Logic that does "elapsed ticks of work" per update must have done
        // exactly one tick of work per tick, from the very first update on.
        let mut s = Scheduler::new();
        for id in 0..10 {
            s.insert(id, 4);
        }
        s.insert(10, 1);
        s.insert(11, 60);
        let mut work = [0u64; 12];
        let mut last = [0u64; 12];
        for t in 0..240 {
            s.due(t, |id, elapsed| {
                work[id as usize] += u64::from(elapsed);
                last[id as usize] = t;
            });
        }
        for id in 0..12 {
            assert_eq!(work[id], last[id] + 1, "object {id}");
        }
    }

    #[test]
    fn same_rate_objects_are_spread_evenly_across_ticks() {
        let mut s = Scheduler::new();
        for id in 0..1000 {
            s.insert(id, 10);
        }
        for t in 0..40 {
            assert_eq!(s.due_count(t), 100, "tick {t}");
        }
    }

    #[test]
    fn due_count_matches_what_due_visits() {
        let mut s = Scheduler::new();
        for id in 0..333 {
            s.insert(id, 1 + id % 9);
        }
        for t in 0..100 {
            assert_eq!(s.due_count(t), due_ids(&s, t).len());
        }
    }

    #[test]
    fn order_is_the_same_whatever_order_rates_were_first_seen() {
        let mut a = Scheduler::new();
        a.insert(1, 5);
        a.insert(2, 2);
        let mut b = Scheduler::new();
        b.insert(2, 2);
        b.insert(1, 5);
        // Both objects are in bucket 0 of their lane, so both are due at t=0;
        // the faster lane always comes first.
        assert_eq!(due_ids(&a, 0), vec![2, 1]);
        assert_eq!(due_ids(&b, 0), vec![2, 1]);
    }

    #[test]
    fn absurd_rates_are_clamped_not_allocated() {
        let mut s = Scheduler::new();
        s.insert(1, u32::MAX);
        let mut seen = 0;
        s.due(0, |_, elapsed| {
            assert_eq!(elapsed, 1, "first update: one tick has existed");
            seen += 1;
        });
        s.due(u64::from(MAX_OB_TICK), |_, elapsed| {
            assert_eq!(elapsed, MAX_OB_TICK, "clamped to the cap, not 4 billion");
            seen += 1;
        });
        assert_eq!(seen, 2);
    }

    #[test]
    fn clear_empties_and_restarts_the_dealing() {
        let mut s = Scheduler::new();
        for id in 0..10 {
            s.insert(id, 3);
        }
        s.clear();
        assert_eq!((s.scheduled(), s.never()), (0, 0));
        assert!(due_ids(&s, 0).is_empty());
        s.insert(42, 3);
        assert_eq!(due_ids(&s, 0), vec![42], "dealing restarts at bucket 0");
    }

    #[test]
    fn works_at_very_large_tick_numbers() {
        let mut s = Scheduler::new();
        s.insert(1, 7);
        let t = u64::MAX - (u64::MAX % 7);
        assert_eq!(due_ids(&s, t), vec![1]);
        assert!(due_ids(&s, t + 1).is_empty());
    }
}
