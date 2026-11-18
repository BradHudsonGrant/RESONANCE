use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How often a provider may be asked, as its terms state it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub requests: u32,
    pub per: Duration,
}

impl RateLimit {
    pub const fn per_second(requests: u32) -> Self {
        Self {
            requests,
            per: Duration::from_secs(1),
        }
    }

    pub const fn per_minute(requests: u32) -> Self {
        Self {
            requests,
            per: Duration::from_secs(60),
        }
    }

    fn interval(self) -> Duration {
        self.per / self.requests.max(1)
    }
}

/// A bucket of one token that refills once per interval. Holding a single token rather than a
/// burst keeps every window of `per` at or under `requests`, which is what the terms ask.
pub struct RateLimiter {
    interval: Duration,
    next_free: Mutex<Option<Instant>>,
}

impl RateLimiter {
    pub fn new(limit: RateLimit) -> Self {
        Self {
            interval: limit.interval(),
            next_free: Mutex::new(None),
        }
    }

    /// Takes the next slot and says how long to wait from `now` before using it.
    pub fn reserve(&self, now: Instant) -> Duration {
        let mut next_free = self
            .next_free
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let slot = match *next_free {
            Some(free) if free > now => free,
            _ => now,
        };
        *next_free = Some(slot + self.interval);
        slot - now
    }

    /// Waits for the next slot.
    pub fn acquire(&self) {
        let wait = self.reserve(Instant::now());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_a_second_are_spaced_a_third_of_a_second_apart() {
        let limiter = RateLimiter::new(RateLimit::per_second(3));
        let start = Instant::now();
        let waits: Vec<_> = (0..4).map(|_| limiter.reserve(start)).collect();
        let third = Duration::from_secs(1) / 3;
        assert_eq!(waits, vec![Duration::ZERO, third, third * 2, third * 3]);
    }

    #[test]
    fn an_idle_limiter_does_not_bank_a_burst() {
        let limiter = RateLimiter::new(RateLimit::per_second(1));
        let start = Instant::now();
        assert_eq!(limiter.reserve(start), Duration::ZERO);
        let later = start + Duration::from_secs(10);
        assert_eq!(limiter.reserve(later), Duration::ZERO);
        assert_eq!(limiter.reserve(later), Duration::from_secs(1));
    }

    #[test]
    fn sixty_a_minute_is_one_a_second() {
        let limiter = RateLimiter::new(RateLimit::per_minute(60));
        let start = Instant::now();
        limiter.reserve(start);
        assert_eq!(limiter.reserve(start), Duration::from_secs(1));
    }
}
