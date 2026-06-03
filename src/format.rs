//! Locale-aware formatting for remaining-time progress messages.

use std::time::Duration;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_DAY: u64 = 86_400;

/// Format a remaining-time progress line for the selected locale.
///
/// The formatter currently localizes English and French labels, falling back
/// to English for other locales. The locale value is injected so tests do not
/// need to mutate process environment variables.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use vsleep::format_remaining_time;
///
/// let line = format_remaining_time(Duration::from_secs(65), "en-GB");
/// assert_eq!(line, "1 minute 5 seconds remaining\n");
/// ```
#[must_use]
pub fn format_remaining_time(remaining: Duration, locale: &str) -> String {
    let seconds = seconds_ceiling(remaining);
    let parts = time_parts(seconds);
    let language = Language::from_locale(locale);
    format!(
        "{} {}\n",
        join_parts(&parts, language),
        language.remaining(seconds)
    )
}

const fn seconds_ceiling(duration: Duration) -> u64 {
    let base = duration.as_secs();
    if duration.subsec_nanos() == 0 {
        base
    } else {
        base.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for remaining-time locale formatting.

    use std::time::Duration;

    use rstest::rstest;

    use super::format_remaining_time;

    #[rstest]
    #[case::english_singular(Duration::from_secs(1), "en-GB", "1 second remaining\n")]
    #[case::english_compound(
        Duration::from_secs(3_665),
        "en-US",
        "1 hour 1 minute 5 seconds remaining\n"
    )]
    #[case::french_plural(Duration::from_secs(2), "fr-FR", "2 secondes restantes\n")]
    #[case::fallback(Duration::from_mins(1), "cy-GB", "1 minute remaining\n")]
    fn formats_remaining_time(
        #[case] remaining: Duration,
        #[case] locale: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(format_remaining_time(remaining, locale), expected);
    }

    #[test]
    fn rounds_fractional_remaining_time_up_to_next_second() {
        assert_eq!(
            format_remaining_time(Duration::from_millis(1), "en"),
            "1 second remaining\n"
        );
    }
}

fn time_parts(total_seconds: u64) -> Vec<TimePart> {
    let (days, after_days) = div_rem(total_seconds, SECONDS_PER_DAY);
    let (hours, after_hours) = div_rem(after_days, SECONDS_PER_HOUR);
    let (minutes, seconds) = div_rem(after_hours, SECONDS_PER_MINUTE);
    [
        TimePart::new(days, Unit::Day),
        TimePart::new(hours, Unit::Hour),
        TimePart::new(minutes, Unit::Minute),
        TimePart::new(seconds, Unit::Second),
    ]
    .into_iter()
    .filter(TimePart::is_present)
    .collect()
}

fn div_rem(value: u64, divisor: u64) -> (u64, u64) {
    (
        value.checked_div(divisor).unwrap_or_default(),
        value.checked_rem(divisor).unwrap_or_default(),
    )
}

fn join_parts(parts: &[TimePart], language: Language) -> String {
    if parts.is_empty() {
        return language.format_part(TimePart::new(0, Unit::Second));
    }
    parts
        .iter()
        .map(|part| language.format_part(*part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Copy)]
struct TimePart {
    value: u64,
    unit: Unit,
}

impl TimePart {
    const fn new(value: u64, unit: Unit) -> Self { Self { value, unit } }

    const fn is_present(&self) -> bool { self.value > 0 }
}

#[derive(Debug, Clone, Copy)]
enum Unit {
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Debug, Clone, Copy)]
enum Language {
    English,
    French,
}

impl Language {
    fn from_locale(locale: &str) -> Self {
        if locale.to_ascii_lowercase().starts_with("fr") {
            Self::French
        } else {
            Self::English
        }
    }

    fn format_part(self, part: TimePart) -> String {
        format!("{} {}", part.value, self.unit_label(part.unit, part.value))
    }

    const fn remaining(self, seconds: u64) -> &'static str {
        match self {
            Self::English => "remaining",
            Self::French if seconds == 1 => "restante",
            Self::French => "restantes",
        }
    }

    const fn unit_label(self, unit: Unit, value: u64) -> &'static str {
        match (self, unit, value) {
            (Self::English, Unit::Day, 1) => "day",
            (Self::English, Unit::Day, _) => "days",
            (Self::English, Unit::Hour, 1) => "hour",
            (Self::English, Unit::Hour, _) => "hours",
            (Self::English | Self::French, Unit::Minute, 1) => "minute",
            (Self::English | Self::French, Unit::Minute, _) => "minutes",
            (Self::English, Unit::Second, 1) => "second",
            (Self::English, Unit::Second, _) => "seconds",
            (Self::French, Unit::Day, 1) => "jour",
            (Self::French, Unit::Day, _) => "jours",
            (Self::French, Unit::Hour, 1) => "heure",
            (Self::French, Unit::Hour, _) => "heures",
            (Self::French, Unit::Second, 1) => "seconde",
            (Self::French, Unit::Second, _) => "secondes",
        }
    }
}
