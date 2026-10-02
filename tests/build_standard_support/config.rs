//! Readers for the Cargo configuration half of the build standard: the
//! toolchain pin, the flag lists, and the `rustflags` sources in
//! `.cargo/config.toml`. TOML values are parsed before the reader classifies
//! the supported one-line source layout.

pub const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
pub const TOOLCHAIN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));

/// The parallel-frontend flag every `rustflags` source carries on a nightly
/// pin.
pub const THREADS_FLAG: &str = "-Zthreads=8";
/// The nightly backend flag used by development builds.
pub const CODEGEN_BACKEND_FLAG: &str = "-Zcodegen-backend=cranelift";
/// The linker flag the Linux source adds, normalized to one token.
pub const LINKER_FLAG: &str = "-Clink-arg=-fuse-ld=mold";
/// The one Cargo target table that covers every Linux architecture.
const LINUX_CFG_TABLE: &str = "target.'cfg(target_os = \"linux\")'";
/// Calendar month lengths in a non-leap year; leap day is added below.
const MONTH_LENGTHS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A list of complaints about the repository.
pub type Problems = Vec<String>;

/// The channel the toolchain file pins.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pin {
    Nightly,
    Stable,
}

impl Pin {
    /// Reads the pin from a `rust-toolchain.toml`.
    ///
    /// The channel must be named exactly once and be one the standard knows: a
    /// `nightly` (dated or not), `stable`, `beta`, or a numbered release.
    /// Anything else, and a missing or repeated `channel`, is an error rather
    /// than a guess that lets a malformed file pass as stable.
    ///
    /// ```text
    /// Pin::read("channel = \"nightly-2026-05-28\"") == Ok(Pin::Nightly)
    /// Pin::read("channel = \"1.94.0\"")             == Ok(Pin::Stable)
    /// Pin::read("[toolchain]")                       == Err(..)
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the reason when the channel is missing, repeated or unsupported.
    pub fn read(toolchain_source: &str) -> Result<Self, String> {
        let document = toml::from_str::<toml::Value>(toolchain_source)
            .map_err(|error| format!("reading rust-toolchain.toml: {error}"))?;
        let channel = document
            .get("toolchain")
            .and_then(toml::Value::as_table)
            .and_then(|toolchain_table| toolchain_table.get("channel"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "rust-toolchain.toml names no string channel".to_owned())?;
        Self::classify(channel)
    }

    /// Classifies one channel name.
    fn classify(channel: &str) -> Result<Self, String> {
        let is_nightly = channel == "nightly" || is_dated_nightly(channel);
        let is_release = is_release_channel(channel);
        if is_nightly {
            Ok(Self::Nightly)
        } else if is_release || matches!(channel, "stable" | "beta") {
            Ok(Self::Stable)
        } else {
            Err(format!(
                "the channel `{channel}` is not one the standard knows"
            ))
        }
    }

    /// Returns whether the pin takes `-Zthreads`, which is a nightly flag.
    pub const fn takes_threads(self) -> bool { matches!(self, Self::Nightly) }
}

/// Returns whether a nightly pin has a calendar-shaped release date.
fn is_dated_nightly(channel: &str) -> bool {
    let Some(date) = channel.strip_prefix("nightly-") else {
        return false;
    };
    let mut parts = date.split('-');
    let (Some(year_text), Some(month_text), Some(day_text), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let has_fixed_width_components = [year_text, month_text, day_text]
        .into_iter()
        .zip([4, 2, 2])
        .all(|(component, width)| component.len() == width);
    if !has_fixed_width_components {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year_text.parse::<u32>(),
        month_text.parse::<u32>(),
        day_text.parse::<u32>(),
    ) else {
        return false;
    };
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let Some(month_index) = month
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
    else {
        return false;
    };
    let Some(days_in_month) = MONTH_LENGTHS.get(month_index) else {
        return false;
    };
    let month_last_day = *days_in_month + u32::from(month == 2 && leap_year);
    (1..=month_last_day).contains(&day)
}

/// Returns whether a stable release pin has two or three numeric components.
fn is_release_channel(channel: &str) -> bool {
    let components: Vec<&str> = channel.split('.').collect();
    matches!(components.len(), 2 | 3)
        && components.iter().all(|component| {
            !component.is_empty() && component.chars().all(|digit| digit.is_ascii_digit())
        })
}

/// A list of compiler flags, with `-C value` pairs joined into `-Cvalue` so
/// both spellings compare equal.
#[derive(Debug, PartialEq, Eq)]
pub struct Flags(Vec<String>);

impl Flags {
    /// Reads a flag list from its words.
    ///
    /// ```text
    /// Flags::from_words(["-C", "link-arg=-fuse-ld=mold"]) == Flags::from_words(["-Clink-arg=-fuse-ld=mold"])
    /// ```
    pub fn from_words<'a>(words: impl IntoIterator<Item = &'a str>) -> Self {
        let mut joined: Vec<String> = Vec::new();
        for word in words {
            match joined.last_mut() {
                Some(last) if last == "-C" => *last = format!("-C{word}"),
                _ => joined.push(word.to_owned()),
            }
        }
        Self(joined)
    }

    /// Returns whether the list names one flag.
    fn names(&self, flag: &str) -> bool { self.0.iter().any(|candidate| candidate == flag) }

    /// Returns whether the list names the frontend flag.
    pub fn names_threads(&self) -> bool { self.names(THREADS_FLAG) }

    /// Returns whether the list names the linker flag.
    pub fn names_linker(&self) -> bool { self.names(LINKER_FLAG) }

    /// Returns whether the list names the development codegen backend.
    pub fn names_cranelift(&self) -> bool { self.names(CODEGEN_BACKEND_FLAG) }

    /// Returns the normalized flag words for contract assertions.
    #[must_use]
    pub fn words(&self) -> &[String] { &self.0 }

    /// Returns the list without the linker flag, which is the one that may
    /// differ.
    fn without_linker_flag(&self) -> Vec<&String> {
        self.0.iter().filter(|flag| *flag != LINKER_FLAG).collect()
    }

    /// Checks the list against a pin and whether the linker flag is expected.
    ///
    /// ```text
    /// Flags::from_words(["-Zthreads=8"]).meets(Pin::Nightly, false) == Ok(())
    /// Flags::from_words([]).meets(Pin::Nightly, false).is_err()
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the reason when the frontend or linker flag is wrong.
    pub fn meets(&self, pin: Pin, takes_linker_flag: bool) -> Result<(), String> {
        if self.names_threads() != pin.takes_threads() {
            return Err(format!("gets {THREADS_FLAG} wrong: {:?}", self.0));
        }
        if self.names_linker() != takes_linker_flag {
            return Err(format!("gets mold wrong: {:?}", self.0));
        }
        Ok(())
    }
}

/// One `rustflags` source in a Cargo configuration.
struct Source {
    table: String,
    flags: Flags,
}

impl Source {
    /// Returns whether the table covers every Linux target.
    fn is_linux(&self) -> bool { self.table == LINUX_CFG_TABLE }

    /// Returns what is wrong with the source's flags for a pin: the frontend
    /// flag on a nightly pin only, and mold in a Linux table only.
    fn problem(&self, pin: Pin) -> Option<String> {
        let reason = self.flags.meets(pin, self.is_linux()).err()?;
        Some(format!("[{}] {reason}", self.table))
    }
}

/// One line of a Cargo configuration, as far as the standard reads it.
enum Line {
    Table(String),
    Rustflags(Flags),
    Other,
}

/// Reads one configuration line. A `rustflags` entry is a one-line array of
/// strings, which is the shape the standard prescribes; an entry spread over
/// several lines is refused rather than half read.
///
/// ```text
/// read_line("[build]")                     -> Line::Table("build")
/// read_line("rustflags = [\"-Zthreads=8\"]") -> Line::Rustflags(..)
/// ```
fn read_line(source_line: &str) -> Result<Line, String> {
    if source_line.starts_with('[') {
        let table_line = source_line.split('#').next().unwrap_or_default().trim();
        return Ok(Line::Table(
            table_line.trim_matches(|c| c == '[' || c == ']').to_owned(),
        ));
    }
    let Some((key, raw_value)) = source_line.split_once('=') else {
        return Ok(Line::Other);
    };
    if key.trim() != "rustflags" {
        return Ok(Line::Other);
    }
    let rustflags_value = raw_value.trim();
    if !rustflags_value.starts_with('[') || !rustflags_value.contains(']') {
        return Err("a `rustflags` array spans lines; keep it on one".to_owned());
    }
    let document = toml::from_str::<toml::Value>(&format!("rustflags = {rustflags_value}"))
        .map_err(|error| format!("reading a `rustflags` array: {error}"))?;
    let words = document
        .get("rustflags")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "`rustflags` is not an array".to_owned())?
        .iter()
        .map(|toml_value| {
            toml_value
                .as_str()
                .ok_or_else(|| "a `rustflags` array contains a non-string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Line::Rustflags(Flags::from_words(words)))
}

/// Returns every `rustflags` source in a Cargo configuration.
fn sources(config: &str) -> Result<Vec<Source>, String> {
    let mut table = String::new();
    let mut found = Vec::new();
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
    {
        match read_line(line)? {
            Line::Table(name) => table = name,
            Line::Rustflags(flags) => found.push(Source {
                table: table.clone(),
                flags,
            }),
            Line::Other => {}
        }
    }
    Ok(found)
}

/// Returns the complaints about which sources exist: the Linux `cfg` table is
/// required because an architecture-specific table leaves other Linux targets
/// without mold; a nightly pin also needs `[build]` for non-Linux hosts.
fn shape_problems(found: &[Source], pin: Pin) -> Problems {
    let checks = [
        (found.is_empty(), "no rustflags source"),
        (
            !found.iter().any(Source::is_linux),
            "no all-Linux cfg target table carries rustflags",
        ),
        (
            pin.takes_threads() && !found.iter().any(|source| source.table == "build"),
            "no [build] rustflags for non-Linux hosts",
        ),
    ];
    checks
        .into_iter()
        .filter(|(failed, _)| *failed)
        .map(|(_, text)| text.to_owned())
        .collect()
}

/// Returns a complaint when the sources differ in anything but the linker,
/// since Cargo applies one source rather than merging them.
fn drift_problem(found: &[Source]) -> Option<String> {
    let mut stripped: Vec<Vec<&String>> = found
        .iter()
        .map(|source| source.flags.without_linker_flag())
        .collect();
    stripped.dedup();
    (stripped.len() > 1).then(|| format!("sources differ beyond the linker: {stripped:?}"))
}

/// Returns every complaint about the configuration sources.
///
/// ```text
/// config_problems(CONFIG, Pin::read(TOOLCHAIN)) == Ok(vec![])   // a compliant repository
/// ```
///
/// # Errors
///
/// Returns the reason when the configuration cannot be read.
pub fn config_problems(config: &str, pin: Pin) -> Result<Problems, String> {
    let found = sources(config)?;
    let mut problems = shape_problems(&found, pin);
    problems.extend(found.iter().filter_map(|source| source.problem(pin)));
    problems.extend(drift_problem(&found));
    Ok(problems)
}

#[cfg(test)]
mod mutation_contract {
    //! Mutation checks prove that source-shape violations are rejected.

    use super::{CONFIG, LINUX_CFG_TABLE, Pin, TOOLCHAIN, config_problems};

    #[test]
    fn replacing_the_repository_cfg_source_with_one_architecture_fails() {
        let mutated = CONFIG.replacen(LINUX_CFG_TABLE, "target.\"x86_64-unknown-linux-gnu\"", 1);
        assert_ne!(
            mutated, CONFIG,
            "the Linux cfg source must be present to mutate"
        );
        let pin = Pin::read(TOOLCHAIN).expect("repository toolchain must have a supported pin");
        let problems = config_problems(&mutated, pin).expect("mutated Cargo config must parse");
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("no all-Linux cfg target table")),
            "architecture-only mutation passed: {problems:?}"
        );
    }
}
