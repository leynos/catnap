//! Generated contract cases for the build-standard readers. Each property
//! derives its expected result from the intended command or configuration
//! shape, rather than from another reader implementation.

use proptest::{prelude::*, test_runner::TestCaseError};
use rstest::rstest;

use super::{
    ci_steps::{coverage_problems, linker_install_problems},
    config::{Flags, Pin, config_problems},
    make::{Assignment, commands_from},
};

/// A toolchain file pinning a nightly channel.
const NIGHTLY: &str = "[toolchain]\nchannel = \"nightly-2026-05-28\"\n";
/// A toolchain file pinning a stable channel.
const STABLE: &str = "[toolchain]\nchannel = \"1.94.0\"\n";
/// A toolchain file that names no channel.
const NO_CHANNEL: &str = "[toolchain]\ncomponents = [\"clippy\"]\n";
/// A toolchain file that names two channels.
const TWO_CHANNELS: &str = "[toolchain]\nchannel = \"stable\"\nchannel = \"nightly\"\n";
/// A toolchain file naming a channel the standard does not know.
const UNKNOWN_CHANNEL: &str = "[toolchain]\nchannel = \"weekly\"\n";

/// Scenario: toolchain files pinning each kind of channel, and files that do
/// not.
///
/// Invariant: only a `nightly` channel reads as nightly, so only it is asked to
/// carry `-Zthreads`; a missing, repeated or unknown channel is an error, not a
/// stable pin by default.
#[rstest]
#[case::nightly(NIGHTLY, Some(Pin::Nightly))]
#[case::stable(STABLE, Some(Pin::Stable))]
#[case::missing(NO_CHANNEL, None)]
#[case::repeated(TWO_CHANNELS, None)]
#[case::unknown(UNKNOWN_CHANNEL, None)]
fn the_pin_reader_tells_the_channels_apart(#[case] toolchain: &str, #[case] expected: Option<Pin>) {
    assert_eq!(Pin::read(toolchain).ok(), expected);
}

proptest! {
    #[test]
    fn channels_are_classified_by_their_declared_kind(
        major in 1u16..100,
        minor in 0u16..100,
        patch in 0u16..100,
        year in 2020u16..2100,
        month in 1u8..=12,
        day in 1u8..=28,
    ) {
        let release = format!("[toolchain]\nchannel = \"{major}.{minor}.{patch}\"\n");
        let nightly = format!("[toolchain]\nchannel = \"nightly-{year}-{month:02}-{day:02}\"\n");
        prop_assert_eq!(Pin::read(&release), Ok(Pin::Stable));
        prop_assert_eq!(Pin::read(&nightly), Ok(Pin::Nightly));
    }

    #[test]
    fn split_codegen_flags_normalize_to_the_joined_spelling(
        option in prop_oneof!["[a-z]{1,12}", Just("-fuse-ld=mold".to_owned())],
    ) {
        let value = format!("link-arg={option}");
        let joined = format!("-C{value}");
        let split = Flags::from_words(["-C", value.as_str()]);
        prop_assert_eq!(&split, &Flags::from_words([joined.as_str()]));
        prop_assert_eq!(split.names_linker(), option == "-fuse-ld=mold");
        let mold = Flags::from_words(["-C", "link-arg=-fuse-ld=mold"]);
        prop_assert!(mold.names_linker());
        prop_assert!(mold.meets(Pin::Stable, true).is_ok());
    }

    #[test]
    fn an_architecture_table_cannot_stand_in_for_the_linux_cfg(arch in "[a-z]{1,12}") {
        let config = format!(
            "[build]\nrustflags = [\"-Zthreads=8\"]\n\
             [target.{arch}-unknown-linux-gnu]\n\
             rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
        );
        let problems = config_problems(&config, Pin::Nightly).map_err(TestCaseError::fail)?;
        prop_assert!(problems.iter().any(|problem| problem.contains("no all-Linux cfg target table")));

        let cfg_config = config.replace(
            &format!("[target.{arch}-unknown-linux-gnu]"),
            "[target.'cfg(target_os = \"linux\")']",
        );
        let cfg_problems = config_problems(&cfg_config, Pin::Nightly).map_err(TestCaseError::fail)?;
        prop_assert!(cfg_problems.is_empty(), "{cfg_problems:?}");
    }

    #[test]
    fn make_commands_keep_caller_flags_and_the_nightly_default(name in "[a-z]{1,12}") {
        let output = format!(
            "echo ignored\nRUSTFLAGS=\"${{RUSTFLAGS:+$RUSTFLAGS }}-D {name} -Zthreads=8\" cargo test\n"
        );
        let assignments = commands_from(&output).map_err(TestCaseError::fail)?;
        prop_assert_eq!(assignments.len(), 1);
        match assignments.as_slice() {
            [Assignment::Flags(flags, true)] => {
                prop_assert!(flags.meets(Pin::Nightly, false).is_ok());
                prop_assert_eq!(
                    flags,
                    &Flags::from_words(["-D", name.as_str(), "-Zthreads=8"]),
                );
            }
            _ => prop_assert!(false, "caller flags were lost: {assignments:?}"),
        }
    }

    #[test]
    fn sibling_steps_cannot_lend_mold_or_coverage_flags(name in "[a-z]{1,12}") {
        let setup_without_input = format!(
            "steps:\n  - name: {name}\n    uses: org/setup-rust@abc\n\
             \n  - name: sibling\n    uses: org/other@abc\n    with:\n      install-mold: 'true'\n"
        );
        prop_assert_eq!(linker_install_problems("fixture.yml", &setup_without_input).len(), 1);
        let input_in_env = setup_without_input.replacen(
            "uses: org/setup-rust@abc\n",
            "uses: org/setup-rust@abc\n    env:\n      install-mold: 'true'\n",
            1,
        );
        prop_assert_eq!(linker_install_problems("fixture.yml", &input_in_env).len(), 1);
        let setup_with_input = setup_without_input.replacen(
            "uses: org/setup-rust@abc\n",
            "uses: org/setup-rust@abc\n    with:\n      install-mold: 'true'\n",
            1,
        );
        prop_assert!(linker_install_problems("fixture.yml", &setup_with_input).is_empty());

        let coverage = format!(
            "steps:\n  - name: {name}\n    uses: org/generate-coverage@abc\n\
             \n  - name: sibling\n    env:\n      RUSTFLAGS: -D warnings\n"
        );
        prop_assert_eq!(coverage_problems("fixture.yml", &coverage).len(), 1);
    }
}
