//! Contract tests for the repository's slow-test allowances.

const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.config/nextest.toml"));

/// Checks the structured timeout table against the agreed retry budget.
fn assert_slow_timeout(table: &toml::Table, terminate_after: i64) {
    assert_eq!(
        table.get("period").and_then(toml::Value::as_str),
        Some("180s")
    );
    assert_eq!(
        table
            .get("terminate-after")
            .and_then(toml::Value::as_integer),
        Some(terminate_after)
    );
    assert_eq!(
        table.get("grace-period").and_then(toml::Value::as_str),
        Some("5s")
    );
}

/// The default has one 180 s allowance; `binary(ui)` gets three attempts.
#[test]
fn nextest_preserves_the_exact_default_and_ui_allowances() {
    let document =
        toml::from_str::<toml::Value>(CONFIG).expect("nextest configuration must parse as TOML");
    let default = document
        .get("profile")
        .and_then(toml::Value::as_table)
        .and_then(|profile| profile.get("default"))
        .and_then(toml::Value::as_table)
        .expect("profile.default must be a table");
    let default_timeout = default
        .get("slow-timeout")
        .and_then(toml::Value::as_table)
        .expect("profile.default must define slow-timeout as a table");
    assert_slow_timeout(default_timeout, 1);

    let overrides = default
        .get("overrides")
        .and_then(toml::Value::as_array)
        .expect("profile.default.overrides must be an array of tables");
    let ui_overrides = overrides
        .iter()
        .filter(|override_table| {
            override_table.get("filter").and_then(toml::Value::as_str) == Some("binary(ui)")
        })
        .collect::<Vec<_>>();
    assert_eq!(ui_overrides.len(), 1, "one binary(ui) override is required");
    let ui_timeout = ui_overrides
        .first()
        .and_then(|override_table| override_table.get("slow-timeout"))
        .and_then(toml::Value::as_table)
        .expect("binary(ui) override must define slow-timeout as a table");
    assert_slow_timeout(ui_timeout, 3);
}
