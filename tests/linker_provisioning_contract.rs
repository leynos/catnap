//! Contract test that CI installs `clang`, `lld` and `mold` through `setup-rust`.
//!
//! `.cargo/config.toml` links Linux builds with `clang` and `mold`, and coverage
//! links with lld, so the runner needs all three before any cargo command runs.
//! The workflows get them from `setup-rust`'s `install-mold` and
//! `install-clang-lld` inputs, which accept only the string `'true'` or
//! `'false'`, and the mutation-testing caller forwards the same inputs to
//! `mutation-cargo.yml`. A step that lost an input, set it to another value, put
//! it outside `with:`, or went back to an `apt` line would surface only as a
//! failed link on a runner, so this suite parses each workflow and fails first.
//!
//! The workflows are parsed as YAML, so comments, block and folded scalars and
//! flow styles read exactly as GitHub reads them.

use std::io;

use cap_std::{ambient_authority, fs::Dir};
use rstest::rstest;
use serde_norway::Value;

/// The action every provisioning step must use, pinned by commit SHA.
const SETUP_RUST: &str = "leynos/shared-actions/.github/actions/setup-rust@";

/// The reusable mutation workflow, whose callers forward the same inputs.
const MUTATION_CARGO: &str = "leynos/shared-actions/.github/workflows/mutation-cargo.yml@";

/// The two inputs that must each be the string `'true'`.
const LINKER_INPUTS: [&str; 2] = ["install-mold", "install-clang-lld"];

/// The packages whose hand installation the contract rejects.
const LINKER_PACKAGES: &str = "clang lld mold";

/// Returns whether `uses` is `prefix` followed by a full 40-hex commit SHA.
fn is_pinned(uses: &str, prefix: &str) -> bool {
    uses.strip_prefix(prefix)
        .is_some_and(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Returns every job mapping and every step mapping in the workflow.
fn nodes(workflow: &Value) -> Vec<&Value> {
    let all_jobs = workflow.get("jobs").and_then(Value::as_mapping);
    all_jobs
        .into_iter()
        .flat_map(|mapping| mapping.values())
        .flat_map(|job| {
            let steps = job.get("steps").and_then(Value::as_sequence);
            std::iter::once(job).chain(steps.into_iter().flatten())
        })
        .collect()
}

/// Returns the nodes whose `uses:` pins `prefix` to a full SHA.
fn pinned_to<'a>(nodes: &[&'a Value], prefix: &str) -> Vec<&'a Value> {
    nodes
        .iter()
        .copied()
        .filter(|node| {
            node.get("uses")
                .and_then(Value::as_str)
                .is_some_and(|uses| is_pinned(uses, prefix))
        })
        .collect()
}

/// Returns a complaint when more nodes name `prefix` than pin it to a full SHA,
/// so an unpinned reference cannot hide beside a correctly pinned one.
fn unpinned_references(nodes: &[&Value], prefix: &str, pinned: usize) -> Vec<String> {
    let named = nodes
        .iter()
        .filter_map(|node| node.get("uses").and_then(Value::as_str))
        .filter(|uses| uses.starts_with(prefix))
        .count();
    (named > pinned)
        .then(|| {
            format!(
                "{} setup-rust reference(s) not pinned to a full commit SHA",
                named - pinned
            )
        })
        .into_iter()
        .collect()
}

/// Returns the complaints about one node's `with:` mapping: each linker input
/// that is not the string `'true'`.
fn missing_inputs(node: &Value, owner: &str) -> Vec<String> {
    LINKER_INPUTS
        .iter()
        .filter(|input| {
            let value = node.get("with").and_then(|with| with.get(**input));
            value.and_then(Value::as_str) != Some("true")
        })
        .map(|input| format!("{owner} does not set {input}: 'true'"))
        .collect()
}

/// Returns whether the shell line runs `apt` or `apt-get` with `install` and
/// names one of the linker packages.
fn installs_a_linker(line: &str) -> bool {
    let words: Vec<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .collect();
    let apt = words.iter().any(|word| matches!(*word, "apt" | "apt-get"));
    let package = words
        .iter()
        .any(|word| LINKER_PACKAGES.split(' ').any(|p| p == *word));
    apt && words.contains(&"install") && package
}

/// Splits a script into commands: a trailing backslash joins a line to the next
/// one, and a comment line is never continued.
fn commands(script: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = String::new();
    for line in script.lines().map(str::trim) {
        if line.starts_with('#') {
            found.push(line.to_owned());
        } else if let Some(head) = line.strip_suffix('\\') {
            pending.push_str(head);
            pending.push(' ');
        } else {
            pending.push_str(line);
            found.push(std::mem::take(&mut pending));
        }
    }
    found.push(pending);
    found
}

/// Returns the shell commands in the nodes' `run:` scripts that install a
/// linker by hand.
fn hand_installs(nodes: &[&Value]) -> Vec<String> {
    nodes
        .iter()
        .filter_map(|node| node.get("run").and_then(Value::as_str))
        .flat_map(commands)
        .filter(|command| !command.starts_with('#') && installs_a_linker(command))
        .collect()
}

/// Returns the complaints about a mutation-testing caller: it must forward both
/// inputs and carry no `setup-commands` script.
fn mutation_problems(call: &Value) -> Vec<String> {
    let mut found = missing_inputs(call, "mutation-cargo call");
    if call
        .get("with")
        .and_then(|with| with.get("setup-commands"))
        .is_some()
    {
        found.push("mutation-cargo call still passes setup-commands".to_owned());
    }
    found
}

/// Checks a workflow, returning what is wrong with its provisioning: every
/// pinned `setup-rust` step must set both inputs, every pinned
/// `mutation-cargo.yml` call must forward both, and nothing may install a linker
/// by hand.
fn problems(text: &str) -> Vec<String> {
    let workflow: Value = match serde_norway::from_str(text) {
        Ok(parsed) => parsed,
        Err(error) => return vec![format!("workflow does not parse: {error}")],
    };
    let all = nodes(&workflow);
    let steps = pinned_to(&all, SETUP_RUST);
    let calls = pinned_to(&all, MUTATION_CARGO);
    if steps.is_empty() && calls.is_empty() {
        return vec!["no setup-rust step pinned to a full commit SHA".to_owned()];
    }
    let mut found = unpinned_references(&all, SETUP_RUST, steps.len());
    found.extend(
        steps
            .iter()
            .enumerate()
            .flat_map(|(n, step)| missing_inputs(step, &format!("setup-rust step {}", n + 1))),
    );
    found.extend(calls.into_iter().flat_map(mutation_problems));
    found.extend(
        hand_installs(&all)
            .into_iter()
            .map(|line| format!("hand-rolled install: {line}")),
    );
    found
}

/// Reads a workflow from the crate root, or `None` if the repository has no
/// such workflow.
fn workflow_text(name: &str) -> io::Result<Option<String>> {
    let dir = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    match dir.read_to_string(format!(".github/workflows/{name}")) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[rstest]
#[case("ci.yml")]
#[case("coverage-main.yml")]
#[case("act-validation.yml")]
fn workflows_install_the_linkers_through_setup_rust(#[case] name: &str) {
    let Some(text) = workflow_text(name).expect("workflow should be readable") else {
        // This repository has no such workflow; ci.yml is asserted below.
        return;
    };

    assert_eq!(problems(&text), Vec::<String>::new(), "{name}");
}

#[test]
fn ci_yml_exists() {
    assert!(
        workflow_text("ci.yml")
            .expect("ci.yml should be readable")
            .is_some(),
        "the repository's CI workflow must exist for the provisioning contract to read"
    );
}

const PIN: &str = "0123456789abcdef0123456789abcdef01234567";
const BOTH: &str =
    "        with:\n          install-mold: 'true'\n          install-clang-lld: 'true'\n";

/// Builds a `build-test` job: a pinned setup-rust step, then the extra steps.
fn job(with_block: &str, extra_steps: &str) -> String {
    format!(
        "jobs:\n  build-test:\n    steps:\n      - name: Setup Rust\n        uses: \
         {SETUP_RUST}{PIN}\n{with_block}{extra_steps}"
    )
}

/// Asserts the workflow text reports a problem containing `expected`.
fn assert_reports(text: &str, expected: &str) {
    let found = problems(text);
    assert!(
        found.iter().any(|problem| problem.contains(expected)),
        "expected a problem naming {expected:?}, got {found:?}"
    );
}

#[test]
fn both_inputs_and_no_hand_install_pass() {
    assert_eq!(problems(&job(BOTH, "")), Vec::<String>::new());
}

#[test]
fn comments_flow_styles_and_trailing_comments_read_as_yaml_does() {
    let with_block = "        with: # install the linkers\n          install-mold: 'true' # dev \
                      builds\n          # a comment\n          install-clang-lld: \"true\"\n";

    assert_eq!(problems(&job(with_block, "")), Vec::<String>::new());
}

#[rstest]
#[case::no_with_block("", "install-mold")]
#[case::no_second_input("        with:\n          install-mold: 'true'\n", "install-clang-lld")]
#[case::false_value(
    "        with:\n          install-mold: 'false'\n          install-clang-lld: 'true'\n",
    "install-mold"
)]
#[case::boolean_value(
    "        with:\n          install-mold: true\n          install-clang-lld: 'true'\n",
    "install-mold"
)]
#[case::commented_out(
    "        with:\n          # install-mold: 'true'\n          install-clang-lld: 'true'\n",
    "install-mold"
)]
#[case::under_env(
    "        env:\n          install-mold: 'true'\n          install-clang-lld: 'true'\n",
    "install-mold"
)]
#[case::scalar_lookalike(
    "        with:\n          install-mold: 'false'\n          note: |\n            install-mold: \
     'true'\n            install-clang-lld: 'true'\n",
    "install-mold"
)]
fn a_missing_or_wrong_input_is_reported(#[case] with_block: &str, #[case] expected: &str) {
    assert_reports(&job(with_block, ""), expected);
}

#[test]
fn every_setup_rust_step_must_set_the_inputs() {
    let second = format!(
        "      - name: Second\n        uses: {SETUP_RUST}{PIN}\n        with:\n          \
         install-mold: 'true'\n"
    );

    assert_reports(&job(BOTH, &second), "step 2 does not set install-clang-lld");
}

#[test]
fn an_unpinned_reference_is_reported() {
    let text = format!("jobs:\n  b:\n    steps:\n      - uses: {SETUP_RUST}main\n");

    assert_reports(&text, "no setup-rust step pinned");
}

#[test]
fn a_decoy_uses_line_inside_a_scalar_is_not_a_step() {
    let text = format!(
        "jobs:\n  b:\n    steps:\n      - name: Note\n        run: |\n          uses: \
         {SETUP_RUST}{PIN}\n          with:\n            install-mold: 'true'\n            \
         install-clang-lld: 'true'\n"
    );

    assert_reports(&text, "no setup-rust step pinned");
}

#[rstest]
#[case::apt_get("sudo apt-get install --yes clang lld")]
#[case::apt("sudo apt install --yes clang lld")]
#[case::continued("sudo apt-get install --yes \\\n  clang lld mold")]
#[case::after_a_comment_ending_in_a_backslash(
    "# note \\\nsudo apt-get install --yes clang lld mold"
)]
fn a_hand_rolled_install_is_reported_in_any_spelling(#[case] command: &str) {
    let script = command
        .lines()
        .map(|line| ["          ", line, "\n"].concat())
        .collect::<String>();
    let by_hand = format!("      - name: Install\n        run: |\n{script}");

    assert_reports(&job(BOTH, &by_hand), "hand-rolled install");
}

#[test]
fn a_folded_scalar_install_is_reported() {
    let by_hand =
        "      - name: Install\n        run: >-\n          sudo apt-get install --yes\n          \
         clang lld mold\n";

    assert_reports(&job(BOTH, by_hand), "hand-rolled install");
}

#[rstest]
#[case::other_package("sudo apt-get install --yes jq")]
#[case::commented("# sudo apt-get install --yes clang lld mold")]
#[case::a_build_step_naming_a_linker("make CC=clang")]
fn an_unrelated_or_commented_command_is_not_reported(#[case] command: &str) {
    let step = format!("      - name: Other\n        run: {command}\n");

    assert_eq!(problems(&job(BOTH, &step)), Vec::<String>::new());
}

/// Builds a mutation-testing caller whose `with:` mapping is `with_block`.
fn mutation(with_block: &str) -> String {
    format!("jobs:\n  mutation:\n    uses: {MUTATION_CARGO}{PIN}\n{with_block}")
}

#[test]
fn a_mutation_call_forwarding_both_inputs_passes() {
    let with_block = "    with:\n      extra-args: x\n      install-mold: 'true'\n      \
                      install-clang-lld: 'true'\n";

    assert_eq!(problems(&mutation(with_block)), Vec::<String>::new());
}

#[rstest]
#[case::missing_input("    with:\n      install-clang-lld: 'true'\n", "install-mold")]
#[case::false_value(
    "    with:\n      install-mold: 'false'\n      install-clang-lld: 'true'\n",
    "install-mold"
)]
#[case::setup_commands_left(
    "    with:\n      install-mold: 'true'\n      install-clang-lld: 'true'\n      \
     setup-commands: |\n        true\n",
    "setup-commands"
)]
fn a_mutation_call_is_checked_too(#[case] with_block: &str, #[case] expected: &str) {
    assert_reports(&mutation(with_block), expected);
}

#[test]
fn an_unpinned_reference_beside_a_pinned_one_is_reported() {
    let other = format!("      - name: Other\n        uses: {SETUP_RUST}main\n");

    assert_reports(&job(BOTH, &other), "not pinned to a full commit SHA");
}

#[test]
fn a_duplicated_input_is_reported_as_a_parse_failure() {
    let with_block = "        with:\n          install-mold: 'true'\n          install-mold: \
                      'true'\n          install-clang-lld: 'true'\n";

    assert_reports(&job(with_block, ""), "does not parse");
}

#[test]
fn a_comment_aligned_with_with_before_the_inputs_is_accepted() {
    let with_block =
        "        with: # linkers\n        # note\n          install-mold: 'true'\n          \
         install-clang-lld: 'true'\n";

    assert_eq!(problems(&job(with_block, "")), Vec::<String>::new());
}
