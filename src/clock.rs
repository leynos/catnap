//! Logical-time sleep policy built around Monotony's clock abstraction.

use std::time::Duration;

const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Catnap's logical-time policy for sleep orchestration.
///
/// The runner pairs this policy with a [`monotony::MonotonicClock`]. The clock
/// observes real monotonic time, while this adapter converts elapsed and sleep
/// durations to Catnap's logical-time scale. Implement this trait only for
/// alternate sleeping strategies used by the runner, such as deterministic
/// tests; command parsing and other application code must use
/// [`ThreadLogicalSleeper`].
pub trait LogicalSleeper {
    /// Convert a real elapsed duration to Catnap logical time.
    #[must_use]
    fn logical_elapsed(&self, real_elapsed: Duration) -> Duration;

    /// Sleep for the requested Catnap logical duration.
    fn sleep(&mut self, logical_duration: Duration);
}

/// Error returned for invalid logical-sleeper configuration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockConfigError {
    /// A logical second cannot map to zero real time.
    #[error("logical second duration must be greater than zero")]
    ZeroLogicalSecond,
}

/// Thread-blocking logical sleeper used by the command-line application.
#[derive(Debug)]
pub struct ThreadLogicalSleeper {
    logical_second: Duration,
}

impl ThreadLogicalSleeper {
    /// Create a thread-blocking logical sleeper.
    ///
    /// `logical_second` controls how much real time corresponds to one logical
    /// second. Use `Duration::from_secs(1)` for production behaviour.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::ThreadLogicalSleeper;
    ///
    /// let sleeper = ThreadLogicalSleeper::new(Duration::from_secs(1));
    /// assert!(sleeper.is_ok());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ClockConfigError::ZeroLogicalSecond`] when `logical_second` is
    /// zero.
    pub const fn new(logical_second: Duration) -> Result<Self, ClockConfigError> {
        if logical_second.is_zero() {
            Err(ClockConfigError::ZeroLogicalSecond)
        } else {
            Ok(Self { logical_second })
        }
    }
}

impl LogicalSleeper for ThreadLogicalSleeper {
    fn logical_elapsed(&self, real_elapsed: Duration) -> Duration {
        scale_real_to_logical(real_elapsed, self.logical_second)
    }

    fn sleep(&mut self, logical_duration: Duration) {
        std::thread::sleep(scale_logical_to_real(logical_duration, self.logical_second));
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

#[cfg(test)]
mod tests {
    //! Unit tests for logical-time duration conversion.

    use std::time::Duration;

    use super::{LogicalSleeper, ThreadLogicalSleeper};

    #[test]
    fn scales_real_elapsed_time_to_logical_time() {
        let sleeper = ThreadLogicalSleeper::new(Duration::from_millis(250))
            .expect("a non-zero logical second should be accepted");

        assert_eq!(
            sleeper.logical_elapsed(Duration::from_secs(1)),
            Duration::from_secs(4)
        );
    }
}
