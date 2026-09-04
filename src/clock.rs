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

    /// Convert a logical sleep duration to the corresponding real duration.
    ///
    /// This crate-visible seam keeps blocking thread sleep at the application
    /// boundary while allowing the runner's deterministic tests to advance a
    /// manual monotonic clock by the same real duration.
    #[must_use]
    pub(crate) fn real_sleep_duration(&self, logical_duration: Duration) -> Duration {
        scale_logical_to_real(logical_duration, self.logical_second)
    }
}

impl LogicalSleeper for ThreadLogicalSleeper {
    fn logical_elapsed(&self, real_elapsed: Duration) -> Duration {
        scale_real_to_logical(real_elapsed, self.logical_second)
    }

    fn sleep(&mut self, logical_duration: Duration) {
        std::thread::sleep(self.real_sleep_duration(logical_duration));
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

    use proptest::prelude::*;
    use rstest::rstest;

    use super::{ClockConfigError, LogicalSleeper, ThreadLogicalSleeper, scale_logical_to_real};

    #[test]
    fn rejects_a_zero_logical_second() {
        assert!(matches!(
            ThreadLogicalSleeper::new(Duration::ZERO),
            Err(ClockConfigError::ZeroLogicalSecond)
        ));
    }

    #[rstest]
    #[case::half_second(
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_secs(2)
    )]
    #[case::quarter_second(
        Duration::from_millis(250),
        Duration::from_secs(1),
        Duration::from_secs(4)
    )]
    #[case::two_seconds(
        Duration::from_secs(2),
        Duration::from_secs(1),
        Duration::from_millis(500)
    )]
    fn scales_real_elapsed_time_to_logical_time(
        #[case] logical_second: Duration,
        #[case] real_elapsed: Duration,
        #[case] expected_logical_elapsed: Duration,
    ) {
        let sleeper = ThreadLogicalSleeper::new(logical_second)
            .expect("a non-zero logical second should be accepted");

        assert_eq!(
            sleeper.logical_elapsed(real_elapsed),
            expected_logical_elapsed
        );
    }

    #[rstest]
    #[case::half_second(
        Duration::from_millis(500),
        Duration::from_secs(2),
        Duration::from_secs(1)
    )]
    #[case::quarter_second(
        Duration::from_millis(250),
        Duration::from_secs(4),
        Duration::from_secs(1)
    )]
    #[case::two_seconds(Duration::from_secs(2), Duration::from_secs(1), Duration::from_secs(2))]
    fn scales_logical_sleep_duration_to_real_time(
        #[case] logical_second: Duration,
        #[case] logical_duration: Duration,
        #[case] expected_real_duration: Duration,
    ) {
        let sleeper = ThreadLogicalSleeper::new(logical_second)
            .expect("a non-zero logical second should be accepted");

        assert_eq!(
            sleeper.real_sleep_duration(logical_duration),
            expected_real_duration
        );
    }

    #[test]
    fn saturates_the_real_sleep_duration_on_overflow() {
        assert_eq!(
            scale_logical_to_real(Duration::MAX, Duration::MAX),
            Duration::MAX
        );
    }

    proptest! {
        #[test]
        fn preserves_zero_elapsed_time(logical_second_nanos in 1_u64..=4_000_000_000) {
            let sleeper = ThreadLogicalSleeper::new(Duration::from_nanos(logical_second_nanos))
                .expect("a generated non-zero logical second should be accepted");

            prop_assert_eq!(sleeper.logical_elapsed(Duration::ZERO), Duration::ZERO);
        }

        #[test]
        fn logical_elapsed_time_is_monotonic(
            logical_second_nanos in 1_u64..=4_000_000_000,
            earlier_nanos in 0_u64..=1_000_000_000_000,
            additional_nanos in 0_u64..=1_000_000_000_000,
        ) {
            let sleeper = ThreadLogicalSleeper::new(Duration::from_nanos(logical_second_nanos))
                .expect("a generated non-zero logical second should be accepted");
            let earlier = Duration::from_nanos(earlier_nanos);
            let later = Duration::from_nanos(earlier_nanos + additional_nanos);

            prop_assert!(sleeper.logical_elapsed(earlier) <= sleeper.logical_elapsed(later));
        }

        #[test]
        fn scaling_round_trips_with_bounded_truncation(
            logical_second_nanos in 1_u64..=4_000_000_000,
            logical_duration_nanos in 0_u64..=1_000_000_000_000,
        ) {
            let sleeper = ThreadLogicalSleeper::new(Duration::from_nanos(logical_second_nanos))
                .expect("a generated non-zero logical second should be accepted");
            let logical_duration = Duration::from_nanos(logical_duration_nanos);
            let real_duration = sleeper.real_sleep_duration(logical_duration);
            let round_trip = sleeper.logical_elapsed(real_duration);
            let maximum_truncation_nanos =
                1_000_000_000_u64.div_ceil(logical_second_nanos);

            prop_assert!(round_trip <= logical_duration);
            prop_assert!(
                logical_duration.saturating_sub(round_trip).as_nanos()
                    <= u128::from(maximum_truncation_nanos)
            );
        }
    }
}
