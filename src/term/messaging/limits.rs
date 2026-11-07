//! Flood and loop protection: a sliding-window rate limit per sender/target pair and a cap on
//! the length of a conversation (messages between the same two agents with no long pause).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const RATE_MAX: usize = 10;
pub const RATE_WINDOW: Duration = Duration::from_secs(60);
pub const CONVERSATION_MAX: u8 = 10;
pub const CONVERSATION_GAP: Duration = Duration::from_secs(120);

/// At most `RATE_MAX` accepted messages per `RATE_WINDOW`.
#[derive(Debug, Default)]
pub struct RateLimiter {
    sent: VecDeque<Instant>,
}

impl RateLimiter {
    fn prune(&mut self, now: Instant) {
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= RATE_WINDOW)
        {
            self.sent.pop_front();
        }
    }

    /// `Ok(())` if one more message may be sent now, else `Err(seconds until it may)`.
    pub fn check(&mut self, now: Instant) -> Result<(), u64> {
        self.prune(now);
        if self.sent.len() < RATE_MAX {
            return Ok(());
        }
        let oldest = *self.sent.front().expect("non-empty when full");
        let wait = RATE_WINDOW.saturating_sub(now.duration_since(oldest));
        Err(wait.as_secs() + u64::from(wait.subsec_nanos() > 0))
    }

    pub fn record(&mut self, now: Instant) {
        self.prune(now);
        self.sent.push_back(now);
    }

    pub fn is_idle(&self, now: Instant) -> bool {
        self.sent
            .back()
            .is_none_or(|t| now.duration_since(*t) >= RATE_WINDOW)
    }
}

#[derive(Debug, Clone, Copy)]
struct Convo {
    count: u8,
    last: Instant,
}

/// Conversations keyed by the unordered pair of terminals.
#[derive(Debug, Default)]
pub struct Conversations {
    pairs: HashMap<(usize, usize), Convo>,
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

impl Conversations {
    /// May one more message go between `a` and `b` now?
    pub fn allowed(&self, a: usize, b: usize, now: Instant) -> bool {
        match self.pairs.get(&key(a, b)) {
            Some(c) if now.duration_since(c.last) < CONVERSATION_GAP => c.count < CONVERSATION_MAX,
            _ => true,
        }
    }

    /// Count an accepted message between `a` and `b`.
    pub fn record(&mut self, a: usize, b: usize, now: Instant) {
        let c = self.pairs.entry(key(a, b)).or_insert(Convo {
            count: 0,
            last: now,
        });
        if now.duration_since(c.last) >= CONVERSATION_GAP {
            c.count = 0;
        }
        c.count = c.count.saturating_add(1);
        c.last = now;
    }

    /// A message from the user starts a fresh conversation for every pair with `terminal`.
    pub fn reset_terminal(&mut self, terminal: usize) {
        self.pairs
            .retain(|(a, b), _| *a != terminal && *b != terminal);
    }

    pub fn prune(&mut self, now: Instant) {
        self.pairs
            .retain(|_, c| now.duration_since(c.last) < CONVERSATION_GAP);
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_allows_ten_then_refuses_with_retry() {
        let mut r = RateLimiter::default();
        let t0 = Instant::now();
        for i in 0..10 {
            assert!(r.check(t0 + Duration::from_secs(i)).is_ok());
            r.record(t0 + Duration::from_secs(i));
        }
        let at = t0 + Duration::from_secs(10);
        assert_eq!(r.check(at), Err(50));
        // the oldest leaves the window at t0 + 60 s
        assert!(r.check(t0 + Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn rate_limiter_goes_idle() {
        let mut r = RateLimiter::default();
        let t0 = Instant::now();
        r.record(t0);
        assert!(!r.is_idle(t0 + Duration::from_secs(30)));
        assert!(r.is_idle(t0 + Duration::from_secs(61)));
    }

    #[test]
    fn eleventh_message_of_a_conversation_is_refused() {
        let mut c = Conversations::default();
        let t0 = Instant::now();
        for i in 0..10 {
            let t = t0 + Duration::from_secs(i);
            assert!(c.allowed(1, 2, t), "message {i}");
            c.record(
                if i % 2 == 0 { 1 } else { 2 },
                if i % 2 == 0 { 2 } else { 1 },
                t,
            );
        }
        assert!(
            !c.allowed(2, 1, t0 + Duration::from_secs(11)),
            "unordered pair"
        );
    }

    #[test]
    fn a_long_pause_starts_a_new_conversation() {
        let mut c = Conversations::default();
        let t0 = Instant::now();
        for i in 0..10 {
            c.record(1, 2, t0 + Duration::from_secs(i));
        }
        assert!(!c.allowed(1, 2, t0 + Duration::from_secs(20)));
        assert!(c.allowed(1, 2, t0 + Duration::from_secs(9 + 121)));
        c.record(1, 2, t0 + Duration::from_secs(9 + 121));
        assert!(c.allowed(1, 2, t0 + Duration::from_secs(9 + 122)));
    }

    #[test]
    fn user_message_resets_every_pair_of_that_terminal() {
        let mut c = Conversations::default();
        let t0 = Instant::now();
        for _ in 0..10 {
            c.record(1, 2, t0);
            c.record(3, 4, t0);
        }
        c.reset_terminal(1);
        assert!(c.allowed(1, 2, t0));
        assert!(!c.allowed(3, 4, t0));
    }

    #[test]
    fn prune_drops_idle_pairs() {
        let mut c = Conversations::default();
        let t0 = Instant::now();
        for p in 0..50 {
            c.record(p, p + 100, t0);
        }
        assert_eq!(c.len(), 50);
        c.prune(t0 + Duration::from_secs(121));
        assert_eq!(c.len(), 0);
    }
}
