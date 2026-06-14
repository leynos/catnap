//! Monotonic clock abstractions used by the sleep runner.

use std::time::{Duration, Instant};

const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// A timestamp from a monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicTimestamp(Duration);

impl MonotonicTimestamp {
    /// Create a timestamp from a duration since an arbitrary monotonic epoch.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::MonotonicTimestamp;
    ///
    /// let stamp = MonotonicTimestamp::from_elapsed(Duration::from_secs(2));
    /// assert_eq!(stamp.duration_since(stamp), Duration::ZERO);
    /// ```
    #[must_use]
    pub const fn from_elapsed(elapsed: Duration) -> Self { Self(elapsed) }

    /// Return the elapsed duration between this timestamp and an earlier one.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::MonotonicTimestamp;
    ///
    /// let start = MonotonicTimestamp::from_elapsed(Duration::from_secs(3));
    /// let end = MonotonicTimestamp::from_elapsed(Duration::from_secs(8));
    /// assert_eq!(end.duration_since(start), Duration::from_secs(5));
    /// ```
    #[must_use]
    pub fn duration_since(self, earlier: Self) -> Duration {
        self.0.checked_sub(earlier.0).unwrap_or_default()
    }
}

/// Injectable monotonic clock used by the sleep runner.
pub trait MonotonicClock {
    /// Return the clock's current monotonic timestamp.
    fn now(&self) -> MonotonicTimestamp;

    /// Sleep for the requested logical duration.
    fn sleep(&mut self, duration: Duration);
}

/// Error returned for invalid real clock configuration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockConfigError {
    /// A logical second cannot map to zero real time.
    #[error("logical second duration must be greater than zero")]
    ZeroLogicalSecond,
}

/// Monotonic clock backed by [`std::time::Instant`].
#[derive(Debug)]
pub struct RealMonotonicClock {
    started_at: Instant,
    logical_second: Duration,
}

impl RealMonotonicClock {
    /// Create a real monotonic clock.
    ///
    /// `logical_second` controls how much real time corresponds to one logical
    /// second. Use `Duration::from_secs(1)` for production behaviour.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::RealMonotonicClock;
    ///
    /// let clock = RealMonotonicClock::new(Duration::from_secs(1));
    /// assert!(clock.is_ok());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ClockConfigError::ZeroLogicalSecond`] when `logical_second` is
    /// zero.
    pub fn new(logical_second: Duration) -> Result<Self, ClockConfigError> {
        if logical_second.is_zero() {
            Err(ClockConfigError::ZeroLogicalSecond)
        } else {
            Ok(Self {
                started_at: Instant::now(),
                logical_second,
            })
        }
    }
}

impl MonotonicClock for RealMonotonicClock {
    fn now(&self) -> MonotonicTimestamp {
        MonotonicTimestamp::from_elapsed(scale_real_to_logical(
            self.started_at.elapsed(),
            self.logical_second,
        ))
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(scale_logical_to_real(duration, self.logical_second));
    }
}

fn scale_logical_to_real(duration: Duration, logical_second: Duration) -> Duration {
    scale_nanos(duration, logical_second.as_nanos(), NANOS_PER_SECOND)
}

fn scale_real_to_logical(duration: Duration, logical_second: Duration) -> Duration {
    scale_nanos(duration, NANOS_PER_SECOND, logical_second.as_nanos())
}

fn scale_nanos(duration: Duration, numerator: u128, denominator: u128) -> Duration {
    let scaled = duration
        .as_nanos()
        .checked_mul(numerator)
        .and_then(|nanos| nanos.checked_div(denominator))
        .unwrap_or(u128::MAX);
    duration_from_nanos_saturating(scaled)
}

fn duration_from_nanos_saturating(nanos: u128) -> Duration {
    u64::try_from(nanos).map_or(Duration::MAX, Duration::from_nanos)
}
