//! Generated contract cases for the build-standard readers. Each property
//! derives its expected result from the source values, not another reader.

use std::fmt::Write as _;

use proptest::{prelude::*, test_runner::TestCaseError};

use super::{
    ci_steps::{coverage_problems, linker_install_problems},
    config::{CODEGEN_BACKEND_FLAG, Flags, LINKER_FLAG, Pin, THREADS_FLAG, config_problems},
    make::{Assignment, assigned_rustflags, commands_from},
};

fn release_channels() -> impl Strategy<Value = String> {
    (1u16..100, 0u16..100, prop::option::of(0u16..100)).prop_map(|(major, minor, patch)| {
        patch.map_or_else(
            || format!("{major}.{minor}"),
            |patch_version| format!("{major}.{minor}.{patch_version}"),
        )
    })
}

fn supported_channels() -> impl Strategy<Value = (String, Pin)> {
    let dated_nightly = (2020u16..2100, 1u8..=12, 1u8..=28).prop_map(|(year, month, day)| {
        (format!("nightly-{year}-{month:02}-{day:02}"), Pin::Nightly)
    });
    prop_oneof![
        Just(("nightly".to_owned(), Pin::Nightly)),
        Just(("nightly-2024-02-29".to_owned(), Pin::Nightly)),
        dated_nightly,
        release_channels().prop_map(|channel| (channel, Pin::Stable)),
        Just(("stable".to_owned(), Pin::Stable)),
        Just(("beta".to_owned(), Pin::Stable)),
    ]
}

fn unsupported_channels() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("nightly-preview".to_owned()),
        Just("nightly-2025-02-29".to_owned()),
        Just("nightly-2026-02-31".to_owned()),
        Just("nightly-2026-13-01".to_owned()),
        (2020u16..2100, 1u8..=12, 1u8..=28)
            .prop_map(|(year, month, day)| { format!("nightly-{year}-{month:02}-{day:02}-extra") }),
        (1u16..100, 0u16..100, 0u16..100, 0u16..100)
            .prop_map(|(major, minor, patch, extra)| format!("{major}.{minor}.{patch}.{extra}")),
        (1u16..100, 0u16..100).prop_map(|(major, minor)| format!("{major}.x{minor}")),
        Just("STABLE".to_owned()),
    ]
}

#[derive(Clone, Debug)]
enum FlagPart {
    Word(String),
    Codegen(String),
}

fn flag_parts() -> impl Strategy<Value = Vec<FlagPart>> {
    let words = prop_oneof![
        Just(FlagPart::Word(THREADS_FLAG.to_owned())),
        Just(FlagPart::Word(CODEGEN_BACKEND_FLAG.to_owned())),
        Just(FlagPart::Word(LINKER_FLAG.to_owned())),
        "-D[a-z]{1,10}".prop_map(FlagPart::Word),
    ];
    let codegen = prop_oneof![
        Just(FlagPart::Codegen("link-arg=-fuse-ld=mold".to_owned())),
        "[a-z][a-z0-9-]{0,10}".prop_map(|value| FlagPart::Codegen(format!("option={value}"))),
    ];
    prop::collection::vec(prop_oneof![words, codegen], 0..8)
}

/// Builds equivalent joined and split spellings independently from the flag
/// groups supplied to rustc.
fn flag_encodings(parts: &[FlagPart]) -> (Vec<String>, Vec<String>) {
    let mut split = Vec::new();
    let mut joined = Vec::new();
    for part in parts {
        match part {
            FlagPart::Word(word) => {
                split.push(word.clone());
                joined.push(word.clone());
            }
            FlagPart::Codegen(option) => {
                split.push("-C".to_owned());
                split.push(option.clone());
                joined.push(format!("-C{option}"));
            }
        }
    }
    (split, joined)
}

fn part_has_threads(parts: &[FlagPart]) -> bool {
    parts
        .iter()
        .any(|part| matches!(part, FlagPart::Word(word) if word == THREADS_FLAG))
}

fn part_has_backend(parts: &[FlagPart]) -> bool {
    parts
        .iter()
        .any(|part| matches!(part, FlagPart::Word(word) if word == CODEGEN_BACKEND_FLAG))
}

fn part_has_linker(parts: &[FlagPart]) -> bool {
    parts.iter().any(|part| match part {
        FlagPart::Word(word) => word == LINKER_FLAG,
        FlagPart::Codegen(option) => option == "link-arg=-fuse-ld=mold",
    })
}

fn build_source() -> String {
    format!("[build]\nrustflags = [\"{THREADS_FLAG}\", \"{CODEGEN_BACKEND_FLAG}\"]\n")
}

fn linux_cfg_source() -> String {
    format!(
        "[target.'cfg(target_os = \"linux\")']\nrustflags = [\"{THREADS_FLAG}\", \
         \"{CODEGEN_BACKEND_FLAG}\", \"{LINKER_FLAG}\"]\n"
    )
}

fn architecture_source(architecture: &str, include_linker: bool) -> String {
    let linker = if include_linker {
        format!(", \"{LINKER_FLAG}\"")
    } else {
        String::new()
    };
    format!(
        "[target.\"{architecture}-unknown-linux-gnu\"]\nrustflags = [\"{THREADS_FLAG}\", \
         \"{CODEGEN_BACKEND_FLAG}\"{linker}]\n"
    )
}

fn architecture_linker_table(architecture: &str) -> String {
    format!("[target.\"{architecture}-unknown-linux-gnu\"]\nlinker = \"clang\"\n")
}

fn spaced_step_indent(additional: usize) -> String { " ".repeat(additional) }

proptest! {
    #[test]
    fn supported_toolchain_channels_have_their_declared_classification(
        (channel, expected) in supported_channels()
    ) {
        let toolchain = format!("[toolchain]\nchannel = \"{channel}\"\n");
        prop_assert_eq!(
            Pin::read(&toolchain),
            Ok(expected),
            "channel: {}",
            channel
        );
    }
    #[test]
    fn unsupported_toolchain_channels_are_rejected(channel in unsupported_channels()) {
        let toolchain = format!("[toolchain]\nchannel = \"{channel}\"\n");
        prop_assert!(Pin::read(&toolchain).is_err(), "accepted channel: {channel}");
    }
    #[test]
    fn channel_fields_must_be_exact_and_unambiguous(
        suffix in "[a-z]{1,10}",
        channel in supported_channels().prop_map(|(channel, _)| channel),
    ) {
        let alias = format!("[toolchain]\nchannel_{suffix} = \"stable\"\n");
        prop_assert!(Pin::read(&alias).is_err(), "accepted alias: {alias}");

        let duplicate = format!(
            "[toolchain]\nchannel = \"{channel}\"\nchannel = \"stable\"\n"
        );
        prop_assert!(Pin::read(&duplicate).is_err(), "accepted duplicate channel");

        let wrong_table = format!("[workspace]\nchannel = \"{channel}\"\n");
        prop_assert!(Pin::read(&wrong_table).is_err(), "accepted channel outside toolchain table");
    }
    #[test]
    fn split_codegen_pairs_preserve_normalized_flag_meaning(
        parts in flag_parts(),
        nightly in any::<bool>(),
        linux in any::<bool>(),
    ) {
        let (split_words, joined_words) = flag_encodings(&parts);
        let split = Flags::from_words(split_words.iter().map(String::as_str));
        let joined = Flags::from_words(joined_words.iter().map(String::as_str));
        let expected_threads = part_has_threads(&parts);
        let expected_backend = part_has_backend(&parts);
        let expected_linker = part_has_linker(&parts);
        let pin = if nightly { Pin::Nightly } else { Pin::Stable };
        let expected_compliant = expected_threads == nightly && expected_linker == linux;

        prop_assert_eq!(&split, &joined, "parts: {:?}", parts);
        prop_assert_eq!(split.words(), joined_words.as_slice(), "parts: {:?}", parts);
        prop_assert_eq!(split.names_threads(), expected_threads, "parts: {:?}", parts);
        prop_assert_eq!(split.names_cranelift(), expected_backend, "parts: {:?}", parts);
        prop_assert_eq!(split.names_linker(), expected_linker, "parts: {:?}", parts);
        prop_assert_eq!(
            split.meets(pin, linux).is_ok(),
            expected_compliant,
            "parts: {:?}, pin: {:?}, Linux: {}",
            parts,
            pin,
            linux
        );
    }
    #[test]
    fn valid_cargo_source_layouts_keep_the_cfg_source_authoritative(
        architecture in prop_oneof![
            Just("x86_64"),
            Just("aarch64"),
            Just("riscv64gc"),
        ],
        cfg_first in any::<bool>(),
        include_arch_linker_table in any::<bool>(),
    ) {
        let build = build_source();
        let cfg = linux_cfg_source();
        let architecture_table = architecture_linker_table(architecture);
        let config = match (cfg_first, include_arch_linker_table) {
            (true, true) => format!("{cfg}{architecture_table}{build}"),
            (false, true) => format!("{build}{architecture_table}{cfg}"),
            (true, false) => format!("{cfg}{build}"),
            (false, false) => format!("{build}{cfg}"),
        };
        let problems = config_problems(&config, Pin::Nightly)
            .map_err(TestCaseError::fail)?;
        prop_assert!(problems.is_empty(), "config: {config}\nproblems: {problems:?}");
    }
    #[test]
    fn architecture_specific_linux_sources_never_replace_the_cfg_source(
        architecture in prop_oneof![
            Just("x86_64"),
            Just("aarch64"),
            Just("riscv64gc"),
        ],
        build_first in any::<bool>(),
    ) {
        let build = build_source();
        let architecture_flags = architecture_source(architecture, true);
        let config = if build_first {
            format!("{build}{architecture_flags}")
        } else {
            format!("{architecture_flags}{build}")
        };
        let problems = config_problems(&config, Pin::Nightly)
            .map_err(TestCaseError::fail)?;
        prop_assert!(
            problems.iter().any(|problem| problem.contains("no all-Linux cfg target table")),
            "architecture-only config passed: {config}"
        );
    }
    #[test]
    fn rustflags_lookalike_keys_do_not_count_as_a_build_source(suffix in "[a-z]{1,10}") {
        let config = format!(
            "[build]\nrustflags_{suffix} = [\"{THREADS_FLAG}\", \"{CODEGEN_BACKEND_FLAG}\"]\n{}",
            linux_cfg_source(),
        );
        let problems = config_problems(&config, Pin::Nightly)
            .map_err(TestCaseError::fail)?;
        prop_assert!(
            problems.iter().any(|problem| problem.contains("no [build] rustflags")),
            "lookalike key was treated as rustflags: {config}"
        );
    }
    #[test]
    fn malformed_rustflags_arrays_are_rejected(
        array in prop_oneof![
            Just("[\"unterminated]".to_owned()),
            Just("[1, 2]".to_owned()),
            Just("not-an-array]".to_owned()),
        ]
    ) {
        let config = format!("[build]\nrustflags = {array}\n");
        prop_assert!(config_problems(&config, Pin::Nightly).is_err(), "accepted: {config}");
    }
    #[test]
    fn make_output_preserves_caller_flags_and_normalizes_split_pairs(
        warning in "[a-z]{1,10}",
        use_empty_fallback in any::<bool>(),
        continued in any::<bool>(),
    ) {
        let inheritance = if use_empty_fallback {
            "${RUSTFLAGS-}"
        } else {
            "${RUSTFLAGS:+$RUSTFLAGS }"
        };
        let continuation = if continued {
            concat!(" \\\n", "  cargo test")
        } else {
            " cargo test"
        };
        let output = format!(
            "echo ignored cargo test\nRUSTFLAGS=\"{inheritance}{THREADS_FLAG} \
             -C link-arg=-fuse-ld=mold -D {warning}\"{continuation}\n"
        );
        let assignments = commands_from(&output).map_err(TestCaseError::fail)?;
        let [Assignment::Flags(flags, true)] = assignments.as_slice() else {
            return Err(TestCaseError::fail(format!("unexpected command parse: {assignments:?}")));
        };
        prop_assert!(flags.names_threads(), "output: {output}");
        prop_assert!(flags.names_linker(), "output: {output}");
        let expected_words = vec![
            THREADS_FLAG.to_owned(),
            LINKER_FLAG.to_owned(),
            "-D".to_owned(),
            warning.clone(),
        ];
        prop_assert_eq!(
            flags.words(),
            expected_words.as_slice(),
            "output: {}",
            output
        );
        prop_assert!(flags.meets(Pin::Nightly, true).is_ok(), "output: {output}");
    }
    #[test]
    fn unsupported_make_assignments_are_rejected(
        line in prop_oneof![
            "[a-z]{1,10}".prop_map(|value| format!("RUSTFLAGS={value} cargo test")),
            "[a-z]{1,10}".prop_map(|value| format!("RUSTFLAGS=\"{value} cargo test")),
            "[a-z]{1,10}".prop_map(|value| format!("RUSTFLAGS='{value}' cargo test")),
        ]
    ) {
        prop_assert!(assigned_rustflags(&line).is_err(), "accepted assignment: {line}");
        prop_assert!(commands_from(&format!("{line}\n")).is_err(), "accepted output: {line}");
    }
    #[test]
    fn setup_step_inputs_are_owned_by_their_workflow_step(
        inputs in prop::collection::vec(any::<bool>(), 1..6),
        indent in 2usize..8,
    ) {
        let step_indent = spaced_step_indent(indent);
        let field_indent = spaced_step_indent(indent + 2);
        let mut workflow = String::from("steps:\n");
        for (position, has_input) in inputs.iter().enumerate() {
            write!(
                workflow,
                "{step_indent}- name: setup-{position}\n{field_indent}uses: org/setup-rust@abc\n"
            )
            .expect("writing to a String must succeed");
            if *has_input {
                write!(
                    workflow,
                    "{field_indent}with:\n{}install-mold: 'true'\n",
                    spaced_step_indent(indent + 4),
                )
                .expect("writing to a String must succeed");
            }
            write!(
                workflow,
                "{step_indent}- name: sibling-{position}\n{field_indent}uses: org/other@abc\n\
                 {field_indent}with:\n{}install-mold: 'true'\n",
                spaced_step_indent(indent + 4),
            )
            .expect("writing to a String must succeed");
        }
        let expected = inputs.iter().filter(|has_input| !**has_input).count();
        let found = linker_install_problems("fixture.yml", &workflow).len();
        prop_assert_eq!(found, expected, "workflow:\n{}", workflow);
    }
    #[test]
    fn coverage_flags_belong_to_the_coverage_step(
        has_own_assignment in any::<bool>(),
        own_assignment_has_dev_flag in any::<bool>(),
        sibling_has_assignment in any::<bool>(),
        indent in 2usize..8,
    ) {
        let step_indent = spaced_step_indent(indent);
        let field_indent = spaced_step_indent(indent + 2);
        let coverage_value = if own_assignment_has_dev_flag {
            THREADS_FLAG
        } else {
            "-D warnings"
        };
        let mut workflow = format!(
            "steps:\n{step_indent}- name: coverage\n{field_indent}uses: org/generate-coverage@abc\n"
        );
        if has_own_assignment {
            write!(
                workflow,
                "{field_indent}env:\n{}RUSTFLAGS: {coverage_value}\n",
                spaced_step_indent(indent + 4),
            )
            .expect("writing to a String must succeed");
        }
        write!(
            workflow,
            "{step_indent}- name: sibling\n{field_indent}env:\n{}RUSTFLAGS: {}\n",
            spaced_step_indent(indent + 4),
            if sibling_has_assignment { THREADS_FLAG } else { "-D warnings" },
        )
        .expect("writing to a String must succeed");
        let expected = usize::from(!has_own_assignment || own_assignment_has_dev_flag);
        let found = coverage_problems("fixture.yml", &workflow).len();
        prop_assert_eq!(found, expected, "workflow:\n{}", workflow);
    }
}
