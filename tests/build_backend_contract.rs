//! Holds `.cargo/config.toml` free of a codegen backend while the release
//! builds on stable.
//!
//! The release workflow builds with `cross +stable build --release`, and stable
//! Cargo reads `.cargo/config.toml` like any other Cargo. It refuses a
//! `codegen-backend` key there ("config profile `dev` is not valid") and stops,
//! so a backend selected in that file breaks every release build. The Cranelift
//! selection therefore lives in `tools/dev-fast/config.toml`, which only the
//! development make targets pass with `--config`. The judgement is driven
//! against fixtures first, because a rule exercised only over this repository's
//! own compliant files would pass whether or not it detects anything, and then
//! applied to the real files. Both are read as text so the contract needs no
//! parser dependency.

use proptest::prelude::*;
use rstest::rstest;

const CARGO_CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
const RELEASE_WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/workflows/release.yml"
));

/// Returns the lines of `text` with comments removed and blanks dropped.
///
/// A `#` starts a comment in both TOML and YAML. Cutting at the first `#` is
/// safe for the keys and commands sought here, none of which contains one.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
}

/// The toolchain selected by one Cargo or `cross` build command.
enum BuildChannel<'a> {
    Inherited,
    Override(&'a str),
}

/// Returns the channel selection on a Cargo or `cross` build command.
///
/// An override on `fmt`, `test`, or another command cannot decide which
/// toolchain the release build uses.
fn build_channel(line: &str) -> Option<BuildChannel<'_>> {
    let mut words = line.split_whitespace();
    while let Some(word) = words.next() {
        let name = word.rsplit('/').next().unwrap_or_default();
        if !matches!(name, "cargo" | "cross") {
            continue;
        }
        let first_argument = words.next()?;
        let (channel, command) = match first_argument.strip_prefix('+') {
            Some(channel) => (BuildChannel::Override(channel), words.next()?),
            None => (BuildChannel::Inherited, first_argument),
        };
        if command == "build" {
            return Some(channel);
        }
    }
    None
}

/// Returns whether the workflow builds on the stable toolchain.
///
/// Each build command selects its own override or inherits the setup action's
/// toolchain. An override on another command does not change that selection.
fn builds_on_stable(workflow: &str) -> bool {
    let has_stable_setup =
        code_lines(workflow).any(|line| line.replace(' ', "") == "toolchain:stable");
    code_lines(workflow)
        .filter_map(build_channel)
        .any(|channel| match channel {
            BuildChannel::Inherited => has_stable_setup,
            BuildChannel::Override(name) => name == "stable",
        })
}

/// Splits a configuration line into its top-level entries, dropping spaces and
/// quote marks: an inline table's entries and its comma-separated pairs, with
/// anything inside a quoted string kept whole.
fn entries(line: &str) -> Vec<String> {
    let mut found = vec![String::new()];
    let mut quote: Option<char> = None;
    let mut is_escaped = false;
    for c in line.chars() {
        if is_escaped {
            if let Some(current) = found.last_mut() {
                current.push(c);
            }
            is_escaped = false;
            continue;
        }
        match (quote, c) {
            (Some('"'), '\\') => {
                is_escaped = true;
                if let Some(current) = found.last_mut() {
                    current.push(c);
                }
            }
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '{' | ',') => found.push(String::new()),
            (None, ' ') => {}
            _ => {
                if let Some(current) = found.last_mut() {
                    current.push(c);
                }
            }
        }
    }
    found
}

/// Returns whether a configuration line sets a `codegen-backend` key. A string
/// value that merely contains the words, such as an `[env]` entry, is not one.
fn sets_backend_key(line: &str) -> bool {
    entries(line)
        .iter()
        .any(|entry| entry.starts_with("codegen-backend="))
}

/// Returns the configuration lines that select or enable a codegen backend.
///
/// The key is matched wherever it sits in the file, so a nested table such as
/// `[profile.dev.package.foo]` or `[profile.dev.build-override]`, an inline
/// table, and a quoted key are reported as well as `[profile.dev]` and
/// `[unstable]`. Cargo refuses every one of them on stable.
fn backend_keys(config: &str) -> Vec<&str> {
    code_lines(config)
        .filter(|line| sets_backend_key(line))
        .collect()
}

/// Returns the reasons the configuration would break a stable release build.
fn findings(config: &str, release: &str) -> Vec<String> {
    if !builds_on_stable(release) {
        return Vec::new();
    }
    backend_keys(config)
        .into_iter()
        .map(|key| format!("`{key}` is set while the release builds on stable, which refuses it"))
        .collect()
}

/// A release workflow building on stable, as this repository's does.
const STABLE_RELEASE: &str = "steps:\n  - run: cross +stable build --release\n";
/// A release workflow building on the pinned nightly.
const NIGHTLY_RELEASE: &str = "steps:\n  - run: cross +nightly-2026-05-28 build --release\n";
/// A release workflow installing stable through a toolchain action.
const STABLE_TOOLCHAIN_ACTION: &str = concat!(
    "steps:\n  - uses: actions-rust-lang/setup-rust-toolchain@abc\n",
    "    with:\n      toolchain: stable\n  - run: cargo build --release\n"
);
/// A release workflow installing the pinned nightly through a toolchain action.
const NIGHTLY_TOOLCHAIN_ACTION: &str = concat!(
    "steps:\n  - uses: actions-rust-lang/setup-rust-toolchain@abc\n",
    "    with:\n      toolchain: nightly-2026-05-28\n  - run: cargo build --release\n"
);
/// A release workflow that installs stable but builds with a nightly override,
/// so the command, not the action, names the toolchain Cargo runs.
const STABLE_ACTION_NIGHTLY_COMMAND: &str = concat!(
    "steps:\n  - uses: actions-rust-lang/setup-rust-toolchain@abc\n",
    "    with:\n      toolchain: stable\n  - run: cross +nightly-2026-05-28 build --release\n"
);
/// A non-build override must not hide the stable toolchain used by `build`.
const STABLE_ACTION_NIGHTLY_FORMAT: &str = concat!(
    "steps:\n  - uses: actions-rust-lang/setup-rust-toolchain@abc\n",
    "    with:\n      toolchain: stable\n",
    "  - run: cargo +nightly-2026-05-28 fmt\n  - run: cargo build --release\n"
);
/// A release workflow that mentions `+stable` in a command that builds nothing.
const STABLE_IN_AN_ECHO: &str =
    "steps:\n  - run: echo +stable is not a toolchain\n  - run: cargo build --release\n";
/// A release workflow invoking Cargo by absolute path on stable.
const STABLE_BY_PATH: &str = "steps:\n  - run: /root/.cargo/bin/cargo +stable build --release\n";
/// A release workflow that mentions stable only in a comment.
const STABLE_IN_A_COMMENT: &str =
    "steps:\n  # Was: cross +stable build --release\n  - run: cross build --release\n";
/// The configuration shape a Cranelift default takes.
const CRANELIFT: &str =
    "[unstable]\ncodegen-backend = true\n\n[profile.dev]\ncodegen-backend = \"cranelift\"\n";
/// A configuration with a linker table and a comment naming the key.
const LINKER_ONLY: &str = concat!(
    "# codegen-backend = \"cranelift\" is deliberately absent.\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n"
);
/// An escaped quote and comma inside a TOML value do not start a new key.
const ESCAPED_VALUE: &str = r#"[env]
HINT = "quoted \" codegen-backend=ignored, still a value"
"#;
/// A real key after that value must still be found.
const ESCAPED_VALUE_AND_KEY: &str = r#"profile = { dev = { note = "quoted \" codegen-backend=ignored, still a value", codegen-backend = "cranelift" } }
"#;

/// Scenario: a configuration with and without a backend key, against release
/// workflows that build on stable or on the pinned nightly.
///
/// Invariant: only a backend key beside a stable release build is reported,
/// once per key, and a comment naming either is not, so the rule is as narrow
/// as it is sufficient.
#[rstest]
#[case::cranelift_with_stable_release(CRANELIFT, STABLE_RELEASE, 2)]
#[case::cranelift_with_stable_toolchain_action(CRANELIFT, STABLE_TOOLCHAIN_ACTION, 2)]
#[case::profile_key_alone("[profile.dev]\ncodegen-backend = \"cranelift\"\n", STABLE_RELEASE, 1)]
#[case::release_profile_key("[profile.release]\ncodegen-backend=\"llvm\"\n", STABLE_RELEASE, 1)]
#[case::nested_package_table(
    "[profile.dev.package.foo]\ncodegen-backend = \"llvm\"\n",
    STABLE_RELEASE,
    1
)]
#[case::build_override_table(
    "[profile.dev.build-override]\ncodegen-backend=\"llvm\"\n",
    STABLE_RELEASE,
    1
)]
#[case::inline_table(
    "profile = { dev = { codegen-backend = \"cranelift\" } }\n",
    STABLE_RELEASE,
    1
)]
#[case::quoted_key(
    "[profile.dev]\n\"codegen-backend\" = \"cranelift\"\n",
    STABLE_RELEASE,
    1
)]
#[case::cargo_invoked_by_path(CRANELIFT, STABLE_BY_PATH, 2)]
#[case::inline_table_entry_after_a_comma(
    "profile = { dev = { opt-level = 1, codegen-backend = \"cranelift\" } }\n",
    STABLE_RELEASE,
    1
)]
#[case::string_value_naming_the_key(
    "[env]\nBACKEND_HINT = \"codegen-backend=cranelift\"\n",
    STABLE_RELEASE,
    0
)]
#[case::string_value_holding_a_comma_and_the_key(
    "[env]\nHINT = \"opt-level=1, codegen-backend=cranelift\"\n",
    STABLE_RELEASE,
    0
)]
#[case::escaped_quote_inside_string_value(ESCAPED_VALUE, STABLE_RELEASE, 0)]
#[case::real_key_after_escaped_string_value(ESCAPED_VALUE_AND_KEY, STABLE_RELEASE, 1)]
#[case::stable_action_with_a_nightly_command(CRANELIFT, STABLE_ACTION_NIGHTLY_COMMAND, 0)]
#[case::stable_action_with_a_nightly_format(CRANELIFT, STABLE_ACTION_NIGHTLY_FORMAT, 2)]
#[case::linker_only_with_stable_release(LINKER_ONLY, STABLE_RELEASE, 0)]
#[case::cranelift_with_nightly_toolchain_action(CRANELIFT, NIGHTLY_TOOLCHAIN_ACTION, 0)]
#[case::stable_named_only_in_an_echo(CRANELIFT, STABLE_IN_AN_ECHO, 0)]
#[case::cranelift_with_nightly_release(CRANELIFT, NIGHTLY_RELEASE, 0)]
#[case::stable_named_only_in_a_comment(CRANELIFT, STABLE_IN_A_COMMENT, 0)]
fn a_backend_key_is_refused_only_beside_a_stable_release(
    #[case] config: &str,
    #[case] release: &str,
    #[case] expected: usize,
) {
    let found = findings(config, release);
    assert_eq!(found.len(), expected, "saw {found:?}");
}

/// Scenario: this repository's own configuration and release workflow.
///
/// Invariant: the release builds on stable, so `.cargo/config.toml` selects no
/// backend. The first check keeps the rule from passing vacuously should the
/// release move off stable without this contract being revisited.
#[test]
fn the_configuration_selects_no_backend_while_the_release_builds_on_stable() {
    assert!(
        builds_on_stable(RELEASE_WORKFLOW),
        "the release no longer builds on stable; revisit this contract and the Cranelift \
         exception in docs/developers-guide.md"
    );
    let found = findings(CARGO_CONFIG, RELEASE_WORKFLOW);
    assert!(found.is_empty(), "{found:?}");
}

proptest! {
    #[test]
    fn unrelated_nightly_commands_do_not_hide_a_stable_build(
        program in prop_oneof![Just("cargo"), Just("cross")],
        command in prop_oneof![Just("fmt"), Just("test"), Just("check")],
    ) {
        let workflow = format!(
            "steps:\n  - uses: org/setup-rust@abc\n    with:\n      toolchain: stable\n\
             \n  - run: {program} +nightly-2026-05-28 {command}\n\
             \n  - run: cargo build --release\n"
        );
        prop_assert_eq!(findings(CRANELIFT, &workflow).len(), 2);
    }

    #[test]
    fn escaped_toml_values_cannot_create_or_hide_a_backend_key(
        prefix in "[a-z]{0,12}",
        value in "[a-z]{1,12}",
    ) {
        let hidden = format!(
            "profile = {{ dev = {{ note = \"{prefix} \\\" codegen-backend={value}, still text\" }} }}\n"
        );
        prop_assert!(findings(&hidden, STABLE_RELEASE).is_empty());
        let with_key = format!(
            "profile = {{ dev = {{ note = \"{prefix} \\\" codegen-backend={value}, still text\", codegen-backend = \"cranelift\" }} }}\n"
        );
        prop_assert_eq!(findings(&with_key, STABLE_RELEASE).len(), 1);
    }
}
