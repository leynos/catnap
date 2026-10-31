//! Contract test for the development build footprint rule.
//!
//! The Concordat `rust-dev-build-footprint` rule needs the default development
//! profile to disable incremental compilation and debug information, and a
//! dated nightly pin from 2026-07-31 to set the native Cargo metadata key. This
//! contract holds all three settings by reading the configuration, starts no Cargo process,
//! and refuses the raw rustc metadata flags that broke build scripts.

/// Root manifest holding the development profile.
const MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
/// Cargo configuration holding the native metadata key.
const CARGO_CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
/// Toolchain file whose dated nightly decides which metadata key is right.
const TOOLCHAIN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));
/// First dated nightly that accepts the native `embed-metadata` key.
const NATIVE_KEY_FROM: &str = "2026-07-31";

/// Returns the non-comment lines of the named top-level TOML table.
fn table_lines<'a>(document: &'a str, table: &str) -> Vec<&'a str> {
    let header = format!("[{table}]");
    document
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty())
        .collect()
}

/// Returns the value assigned to `key` in a table's lines.
fn value<'a>(lines: &[&'a str], key: &str) -> Option<&'a str> {
    lines.iter().find_map(|line| {
        let (name, assigned) = line.split_once('=')?;
        (name.trim() == key).then_some(assigned.trim())
    })
}

/// Scenario: the default development profile.
///
/// Invariant: it keeps no incremental state and no debug information.
#[test]
fn the_development_profile_keeps_the_build_footprint_settings() {
    let profile = table_lines(MANIFEST, "profile.dev");
    assert_eq!(
        value(&profile, "incremental"),
        Some("false"),
        "profile.dev must set incremental = false"
    );
    assert!(
        matches!(value(&profile, "debug"), Some("0" | "false")),
        "profile.dev must set debug = 0 or false, found {:?}",
        value(&profile, "debug")
    );
}

/// Scenario: the Cargo configuration of a pin from 2026-07-31.
///
/// Invariant: it sets the native `embed-metadata = false` key, not the older
/// spelling.
#[test]
fn the_cargo_configuration_disables_metadata_embedding_natively() {
    let unstable = table_lines(CARGO_CONFIG, "unstable");
    assert_eq!(
        value(&unstable, "embed-metadata"),
        Some("false"),
        "[unstable] must set embed-metadata = false"
    );
    assert_eq!(
        value(&unstable, "no-embed-metadata"),
        None,
        "the older no-embed-metadata spelling is wrong for a pin from {NATIVE_KEY_FROM}"
    );
}

/// Returns the `rustflags` values that Cargo reads from `[build]` and `[target.*]`.
fn rustflags_values(config: &toml::Table) -> Vec<&toml::Value> {
    let build = config
        .get("build")
        .and_then(toml::Value::as_table)
        .and_then(|build| build.get("rustflags"));
    let targets = config
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::values)
        .filter_map(toml::Value::as_table)
        .filter_map(|target| target.get("rustflags"));
    build.into_iter().chain(targets).collect()
}

/// Returns whether a `rustflags` value, a string or an array of strings, carries a raw
/// metadata flag.
fn carries_metadata_flag(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(flags) => flags
            .split_whitespace()
            .any(|flag| flag.contains("embed-metadata")),
        toml::Value::Array(flags) => flags
            .iter()
            .filter_map(toml::Value::as_str)
            .any(|flag| flag.contains("embed-metadata")),
        _ => false,
    }
}

/// Scenario: every `rustflags` source in the Cargo configuration.
///
/// Invariant: none carries a raw rustc metadata flag. The configuration is parsed as TOML, so a
/// quoted key or a multiline array is read as Cargo reads it.
#[test]
fn no_rustflags_source_carries_a_raw_metadata_flag() {
    let config: toml::Table =
        toml::from_str(CARGO_CONFIG).expect("the Cargo configuration must be valid TOML");
    let flags = rustflags_values(&config);
    assert!(!flags.is_empty(), "expected at least one rustflags source");
    assert!(
        flags.iter().all(|value| !carries_metadata_flag(value)),
        "no raw embed-metadata flag may appear in rustflags: {flags:?}"
    );
}

/// Returns the pinned channel named in `rust-toolchain.toml`, if any.
fn pinned_channel() -> Option<String> {
    TOOLCHAIN.lines().find_map(|line| {
        let (name, assigned) = line.split_once('=')?;
        (name.trim() == "channel").then(|| assigned.trim().trim_matches('"').to_owned())
    })
}

/// Scenario: the pinned toolchain.
///
/// Invariant: it is a dated nightly from 2026-07-31, the range the native key
/// serves.
#[test]
fn the_pin_is_a_dated_nightly_that_accepts_the_native_key() {
    let channel = pinned_channel().unwrap_or_default();
    let date = channel.strip_prefix("nightly-").unwrap_or("");
    assert!(
        date >= NATIVE_KEY_FROM,
        "{channel:?} must be a dated nightly from {NATIVE_KEY_FROM}; older pins use \
         no-embed-metadata"
    );
}
