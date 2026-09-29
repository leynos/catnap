//! Pins the exact nextest timeout policy for ordinary tests and the nested
//! Cargo UI tests. A changed override must be reviewed with the build standard.

use std::collections::BTreeMap;

const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.config/nextest.toml"));

/// Reads the key/value fields of one inline `slow-timeout` table.
///
/// For example, `period = "180s"` becomes the `period` key with value
/// `"180s"`; duplicates and malformed fields are errors.
///
/// # Errors
///
/// Returns a description of a malformed or repeated field.
fn timeout_fields(line: &str) -> Result<BTreeMap<&str, &str>, String> {
    let body = line
        .strip_prefix("slow-timeout = {")
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| format!("not an inline slow-timeout table: {line}"))?;
    let mut fields = BTreeMap::new();
    for field in body.split(',') {
        let (key, value) = field
            .trim()
            .split_once('=')
            .ok_or_else(|| format!("malformed slow-timeout field: {field}"))?;
        if fields.insert(key.trim(), value.trim()).is_some() {
            return Err(format!("repeated slow-timeout field: {key}"));
        }
    }
    Ok(fields)
}

/// The default is one 180 s period; only `binary(ui)` may take three.
#[test]
fn nextest_preserves_the_exact_default_and_ui_allowances() {
    let mut lines = CONFIG
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    assert_eq!(lines.next(), Some("[profile.default]"));
    let default = lines.next().expect("missing default slow-timeout");
    assert_eq!(
        timeout_fields(default).expect("default timeout table is malformed"),
        BTreeMap::from([
            ("grace-period", "\"5s\""),
            ("period", "\"180s\""),
            ("terminate-after", "1"),
        ])
    );
    assert_eq!(lines.next(), Some("[[profile.default.overrides]]"));
    assert_eq!(lines.next(), Some("filter = 'binary(ui)'"));
    let ui = lines.next().expect("missing UI slow-timeout");
    assert_eq!(
        timeout_fields(ui).expect("UI timeout table is malformed"),
        BTreeMap::from([
            ("grace-period", "\"5s\""),
            ("period", "\"180s\""),
            ("terminate-after", "3"),
        ])
    );
    assert_eq!(lines.next(), None, "unexpected nextest settings");
}
