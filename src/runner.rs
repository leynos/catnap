//! Sleep orchestration that reports remaining time from injected time policy.

use std::{io::Write, time::Duration};

use monotony::MonotonicClock;

use crate::{LogicalSleeper, duration::report_interval, format::format_remaining_time};

/// Configuration for a visual sleep run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    total_duration: Duration,
    locale: String,
}

impl RunConfig {
    /// Create a sleep-run configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::RunConfig;
    ///
    /// let config = RunConfig::new(Duration::from_secs(5), "en-GB");
    /// assert_eq!(config.total_duration(), Duration::from_secs(5));
    /// ```
    #[must_use]
    pub fn new<S>(total_duration: Duration, locale: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            total_duration,
            locale: locale.into(),
        }
    }

    /// Return the total requested duration.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::RunConfig;
    ///
    /// let config = RunConfig::new(Duration::from_secs(1), "en");
    /// assert_eq!(config.total_duration(), Duration::from_secs(1));
    /// ```
    #[must_use]
    pub const fn total_duration(&self) -> Duration { self.total_duration }

    /// Return the locale used for progress formatting.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use catnap::RunConfig;
    ///
    /// let config = RunConfig::new(Duration::from_secs(1), "fr-FR");
    /// assert_eq!(config.locale(), "fr-FR");
    /// ```
    #[must_use]
    pub fn locale(&self) -> &str { &self.locale }
}

/// Sleep until the configured duration has elapsed, reporting remaining time.
///
/// # Examples
///
/// ```
/// use std::{io, time::Duration};
///
/// use catnap::{RunConfig, ThreadLogicalSleeper, run_sleep};
/// use monotony::StdMonotonicClock;
///
/// let mut output = Vec::new();
/// let clock = StdMonotonicClock;
/// let mut sleeper = ThreadLogicalSleeper::new(Duration::from_secs(1))?;
/// let config = RunConfig::new(Duration::from_secs(0), "en-GB");
/// run_sleep(&clock, &mut sleeper, &mut output, &config)?;
/// assert!(output.is_empty());
/// # Ok::<(), io::Error>(())
/// ```
///
/// # Errors
///
/// Returns any I/O error produced while writing progress output.
pub fn run_sleep<C, S, W>(
    clock: &C,
    sleeper: &mut S,
    writer: &mut W,
    config: &RunConfig,
) -> std::io::Result<()>
where
    C: MonotonicClock,
    S: LogicalSleeper,
    W: Write,
{
    let start = clock.now();
    let interval = report_interval(config.total_duration());

    loop {
        let elapsed = sleeper.logical_elapsed(clock.now().duration_since(start));
        if elapsed >= config.total_duration() {
            return Ok(());
        }

        let remaining = remaining_duration(config.total_duration(), elapsed);
        sleeper.sleep(shorter_duration(interval, remaining));
        let elapsed_after_sleep = sleeper.logical_elapsed(clock.now().duration_since(start));
        if elapsed_after_sleep < config.total_duration() {
            let remaining_after_sleep =
                remaining_duration(config.total_duration(), elapsed_after_sleep);
            writer.write_all(
                format_remaining_time(remaining_after_sleep, config.locale()).as_bytes(),
            )?;
        }
    }
}

fn shorter_duration(left: Duration, right: Duration) -> Duration {
    if left <= right { left } else { right }
}

fn remaining_duration(total: Duration, elapsed: Duration) -> Duration {
    total.checked_sub(elapsed).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    //! Unit tests for runner orchestration with deterministic time injection.

    use std::time::{Duration, Instant};

    use monotony::test_util::SharedManualMonotonicClock;

    use super::{RunConfig, run_sleep};
    use crate::LogicalSleeper;

    struct AdvancingSleeper {
        clock: SharedManualMonotonicClock,
    }

    impl LogicalSleeper for AdvancingSleeper {
        fn logical_elapsed(&self, real_elapsed: Duration) -> Duration { real_elapsed }

        fn sleep(&mut self, logical_duration: Duration) { self.clock.advance(logical_duration); }
    }

    #[test]
    fn reports_remaining_time_after_each_deterministic_tick() {
        let clock = SharedManualMonotonicClock::new(Instant::now());
        let mut sleeper = AdvancingSleeper {
            clock: clock.clone(),
        };

        let mut output = Vec::new();
        let config = RunConfig::new(Duration::from_secs(2), "en-GB");
        let result = run_sleep(&clock, &mut sleeper, &mut output, &config);

        assert!(result.is_ok());
        assert_eq!(
            String::from_utf8(output),
            Ok("1 second remaining\n".to_owned())
        );
    }
}
