//! Sleep orchestration that reports remaining time from an injected clock.

use std::{io::Write, time::Duration};

use crate::{clock::MonotonicClock, duration::report_interval, format::format_remaining_time};

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
/// use catnap::{MonotonicClock, MonotonicTimestamp, RunConfig, run_sleep};
///
/// struct DoneClock;
///
/// impl MonotonicClock for DoneClock {
///     fn now(&self) -> MonotonicTimestamp {
///         MonotonicTimestamp::from_elapsed(Duration::from_secs(1))
///     }
///
///     fn sleep(&mut self, _duration: Duration) {}
/// }
///
/// let mut output = Vec::new();
/// let mut clock = DoneClock;
/// let config = RunConfig::new(Duration::from_secs(0), "en-GB");
/// run_sleep(&mut clock, &mut output, &config)?;
/// assert!(output.is_empty());
/// # Ok::<(), io::Error>(())
/// ```
///
/// # Errors
///
/// Returns any I/O error produced while writing progress output.
pub fn run_sleep<C, W>(clock: &mut C, writer: &mut W, config: &RunConfig) -> std::io::Result<()>
where
    C: MonotonicClock,
    W: Write,
{
    let start = clock.now();
    let interval = report_interval(config.total_duration());

    loop {
        let elapsed = clock.now().duration_since(start);
        if elapsed >= config.total_duration() {
            return Ok(());
        }

        let remaining = remaining_duration(config.total_duration(), elapsed);
        clock.sleep(shorter_duration(interval, remaining));
        write_progress_after_sleep(clock, writer, config, start)?;
    }
}

fn write_progress_after_sleep<C, W>(
    clock: &C,
    writer: &mut W,
    config: &RunConfig,
    start: crate::MonotonicTimestamp,
) -> std::io::Result<()>
where
    C: MonotonicClock,
    W: Write,
{
    let elapsed = clock.now().duration_since(start);
    if elapsed < config.total_duration() {
        let remaining = remaining_duration(config.total_duration(), elapsed);
        writer.write_all(format_remaining_time(remaining, config.locale()).as_bytes())?;
    }
    Ok(())
}

fn shorter_duration(left: Duration, right: Duration) -> Duration {
    if left <= right { left } else { right }
}

fn remaining_duration(total: Duration, elapsed: Duration) -> Duration {
    total.checked_sub(elapsed).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    //! Unit tests for runner orchestration with an injected mock clock.

    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use mockall::{mock, predicate::eq};

    use super::{RunConfig, run_sleep};
    use crate::{MonotonicClock, MonotonicTimestamp};

    mock! {
        Clock {}

        impl MonotonicClock for Clock {
            fn now(&self) -> MonotonicTimestamp;
            fn sleep(&mut self, duration: Duration);
        }
    }

    #[test]
    fn reports_remaining_time_after_each_mocked_tick() {
        let elapsed = Arc::new(Mutex::new(Duration::ZERO));
        let mut clock = MockClock::new();
        let now_elapsed = Arc::clone(&elapsed);
        clock
            .expect_now()
            .returning(move || MonotonicTimestamp::from_elapsed(elapsed_value(&now_elapsed)));

        let sleep_elapsed = Arc::clone(&elapsed);
        clock
            .expect_sleep()
            .with(eq(Duration::from_secs(1)))
            .times(2)
            .returning(move |duration| {
                add_elapsed(&sleep_elapsed, duration);
            });

        let mut output = Vec::new();
        let config = RunConfig::new(Duration::from_secs(2), "en-GB");
        let result = run_sleep(&mut clock, &mut output, &config);

        assert!(result.is_ok());
        assert_eq!(
            String::from_utf8(output),
            Ok("1 second remaining\n".to_owned())
        );
    }

    fn elapsed_value(elapsed: &Mutex<Duration>) -> Duration {
        match elapsed.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn add_elapsed(elapsed: &Mutex<Duration>, duration: Duration) {
        match elapsed.lock() {
            Ok(mut guard) => *guard += duration,
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard += duration;
            }
        }
    }
}
