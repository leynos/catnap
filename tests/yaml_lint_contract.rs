//! Keeps the YAML lint gate wired into `netsuke build lint` and CI.
//!
//! The lint action must run `yamllint --strict` over the Git-tracked YAML and
//! actionlint, and CI must install both through the pinned shared installer
//! before it runs the gate. The judgement is driven against fixtures first,
//! because a rule exercised only over this repository's own compliant files
//! would pass whether or not it detects anything, and then applied to the real
//! files. The manifest closure helper belongs only to these policy tests: it
//! follows named dependencies and reads command strings, without rendering
//! templates or acting as a general Netsuke compiler.

use std::collections::{BTreeMap, BTreeSet};

use rstest::rstest;
use serde_norway::Value;

const NETSUKEFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Netsukefile"));
const CI_WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/workflows/ci.yml"
));
const YAMLLINT_CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.yamllint.yml"));
const ACTIONLINT_CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/actionlint.yaml"
));

const INSTALLER: &str = "leynos/shared-actions/.github/actions/install-yaml-lint@";

/// Returns the lines of `text` with comments removed and blanks dropped.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(|line| line.split(" #").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// One line of a recipe or a workflow step, trimmed.
#[derive(Clone, Copy)]
struct Line<'a>(&'a str);

impl Line<'_> {
    /// Whether the line lints the tracked YAML strictly, from Git's own file
    /// list, with its failure left to fail the target.
    fn is_sound_yamllint(self) -> bool {
        let line = self.0;
        let from_git = line.starts_with("git ls-files")
            && line.contains("| xargs")
            && !line.contains(".github");
        let every_pathspec = line.contains("'*.yml'") && line.contains("'*.yaml'");
        from_git
            && every_pathspec
            && line.contains("${YAMLLINT:-yamllint}")
            && line.contains("--strict")
            && !self.ignores_failure()
    }

    /// Whether the line runs actionlint with its failure left to fail the target.
    fn is_sound_actionlint(self) -> bool {
        self.0.starts_with("${ACTIONLINT:-actionlint}") && !self.ignores_failure()
    }

    /// Whether the line is a job key: indented by exactly two spaces.
    fn is_job_header(self) -> bool { self.0.starts_with("  ") && !self.0.starts_with("   ") }

    /// Whether the step line pins the shared installer to a full commit SHA.
    fn pins_the_installer(self) -> bool {
        self.0
            .trim_start_matches("- ")
            .strip_prefix("uses: ")
            .and_then(|uses| uses.strip_prefix(INSTALLER))
            .is_some_and(|reference| {
                reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
            })
    }

    /// Whether the step line runs the lint gate.
    fn runs_the_gate(self) -> bool {
        matches!(
            self.0.trim_start_matches("- "),
            "run: netsuke build lint" | "run: /usr/bin/netsuke build lint"
        )
    }

    /// Whether the line mentions the Netsuke lint action.
    fn mentions_the_gate(self) -> bool { self.0.contains("netsuke") && self.0.contains("lint") }

    /// Whether the line lets a failing step pass.
    fn ignores_failure(self) -> bool {
        self.0.contains("continue-on-error: true")
            || self.0.contains("||")
            || self.0.starts_with('!')
            || self.0.contains("; true")
    }

    /// Whether the line provisions a lint tool by hand.
    fn provisions_by_hand(self) -> bool {
        let line = self.0;
        line.contains("uv tool install \"yamllint")
            || line.contains("download-actionlint")
            || line.contains("releases/download") && line.contains("actionlint")
    }
}

/// A Netsuke manifest inspected without evaluating its command templates.
struct Manifest<'a>(&'a str);

impl Manifest<'_> {
    /// Collect command strings from `lint` and each dependency once.
    /// Invalid YAML, shapes, duplicate names and missing references return `None`.
    fn lint_closure(&self) -> Option<Vec<String>> {
        let document: Value = serde_norway::from_str(self.0).ok()?;
        let indexed = Self::indexed_actions(&document)?;
        let mut pending = vec!["lint"];
        let mut seen = BTreeSet::new();
        let mut lines = Vec::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name) {
                continue;
            }
            let action = indexed.get(name)?;
            pending.extend(Self::dependencies(action)?);
            lines.extend(Self::commands(action)?);
        }
        Some(lines)
    }

    /// Index named actions for these contracts, rejecting invalid or repeated names.
    fn indexed_actions(document: &Value) -> Option<BTreeMap<&str, &Value>> {
        let actions = document.get("actions")?.as_sequence()?;
        let mut indexed = BTreeMap::new();
        for action in actions {
            let name = action.get("name")?.as_str()?;
            if indexed.insert(name, action).is_some() {
                return None;
            }
        }
        Some(indexed)
    }

    /// Read dependency names without accepting a scalar or non-string list item.
    fn dependencies(action: &Value) -> Option<Vec<&str>> {
        match action.get("deps") {
            Some(deps) => deps.as_sequence()?.iter().map(Value::as_str).collect(),
            None => Some(Vec::new()),
        }
    }

    /// Read a scalar or list command; dependency-only actions need no command.
    fn commands(action: &Value) -> Option<Vec<String>> {
        match action.get("command") {
            Some(Value::String(command)) => Some(vec![command.clone()]),
            Some(Value::Sequence(commands)) => commands
                .iter()
                .map(|command| command.as_str().map(str::to_owned))
                .collect(),
            None => (!action.get("deps")?.as_sequence()?.is_empty()).then(Vec::new),
            Some(_) => None,
        }
    }

    /// Whether the closure runs a strict yamllint over `git ls-files` and
    /// actionlint.
    fn gate_is_wired(&self) -> bool {
        self.lint_closure().is_some_and(|lines| {
            lines.iter().any(|line| Line(line).is_sound_yamllint())
                && lines.iter().any(|line| Line(line).is_sound_actionlint())
        })
    }
}

/// A workflow, read as text.
struct Workflow<'a>(&'a str);

impl Workflow<'_> {
    /// The jobs, each as its code lines. A job starts at a two-space-indented
    /// key under `jobs:` and runs to the next such key.
    fn jobs(&self) -> Vec<Vec<&str>> {
        let mut found: Vec<Vec<&str>> = Vec::new();
        let mut in_jobs = false;
        for line in self.0.lines() {
            let code = line.split(" #").next().unwrap_or_default();
            if code.trim().is_empty() || code.trim_start().starts_with('#') {
                continue;
            }
            if !code.starts_with(' ') {
                in_jobs = code.trim_end() == "jobs:";
            } else if in_jobs && Line(code).is_job_header() {
                found.push(Vec::new());
            } else if let Some(job) = found.last_mut().filter(|_| in_jobs) {
                job.push(code.trim());
            }
        }
        found
    }

    /// Whether some job runs the gate and every such job installs the tools,
    /// pinned, in the same job and before it, with failure not ignored.
    fn installs_before_lint(&self) -> bool {
        let gated: Vec<Vec<&str>> = self
            .jobs()
            .into_iter()
            .filter(|job| job.iter().any(|line| Line(line).mentions_the_gate()))
            .collect();
        !gated.is_empty()
            && gated.iter().all(|job| {
                let install = job.iter().position(|line| Line(line).pins_the_installer());
                let lint = job.iter().position(|line| Line(line).runs_the_gate());
                let ignored = job.iter().any(|line| Line(line).ignores_failure());
                matches!((install, lint), (Some(first), Some(second)) if first < second) && !ignored
            })
    }

    /// Whether the workflow still provisions a lint tool by hand.
    fn provisions_by_hand(&self) -> bool {
        code_lines(self.0).any(|line| Line(line).provisions_by_hand())
    }
}

const GOOD_MANIFEST: &str = concat!(
    "actions:\n  - name: lint\n    deps: [yamllint, actionlint]\n",
    "  - name: yamllint\n    command: >-\n",
    "      git ls-files -z '*.yml' '*.yaml' | xargs -0 -r ${YAMLLINT:-yamllint} --strict --\n",
    "  - name: actionlint\n    command: ${ACTIONLINT:-actionlint}\n",
);

/// The judgement follows dependencies and rejects each gate weakening.
#[rstest]
#[case::wired(GOOD_MANIFEST, true)]
#[case::indirect(&GOOD_MANIFEST.replace("deps: [yamllint, actionlint]", "deps: [gate]\n  - name: gate\n    deps: [yamllint, actionlint]"), true)]
#[case::command_list(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command:\n      - echo checking\n      - ${ACTIONLINT:-actionlint}"), true)]
#[case::not_strict(&GOOD_MANIFEST.replace(" --strict", ""), false)]
#[case::one_pathspec(&GOOD_MANIFEST.replace(" '*.yaml'", ""), false)]
#[case::workflows_only(&GOOD_MANIFEST.replace("git ls-files -z '*.yml' '*.yaml' | xargs -0 -r ", "").replace("--strict --", "--strict .github/workflows"), false)]
#[case::workflows_filter(&GOOD_MANIFEST.replace("'*.yaml' |", "'*.yaml' .github/workflows |"), false)]
#[case::failure_ignored(&GOOD_MANIFEST.replace("--strict --", "--strict -- || true"), false)]
#[case::yamllint_then_true(&GOOD_MANIFEST.replace("--strict --", "--strict --; true"), false)]
#[case::yamllint_negated(&GOOD_MANIFEST.replace("      git ls-files", "      ! git ls-files"), false)]
#[case::actionlint_ignored(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command: ${ACTIONLINT:-actionlint} || true"), false)]
#[case::actionlint_then_true(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command: ${ACTIONLINT:-actionlint}; true"), false)]
#[case::actionlint_negated(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command: '! ${ACTIONLINT:-actionlint}'"), false)]
#[case::outside_lint(&GOOD_MANIFEST.replace("deps: [yamllint, actionlint]", "command: echo skipped\n  - name: other\n    deps: [yamllint, actionlint]"), false)]
#[case::no_actionlint(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command: echo skipped"), false)]
#[case::missing_dependency(&GOOD_MANIFEST.replace("deps: [yamllint, actionlint]", "deps: [yamllint, actionlint, absent]"), false)]
#[case::invalid_dependencies(&GOOD_MANIFEST.replace("deps: [yamllint, actionlint]", "deps: invalid"), false)]
#[case::non_string_dependency(&GOOD_MANIFEST.replace("deps: [yamllint, actionlint]", "deps: [yamllint, actionlint, 42]"), false)]
#[case::invalid_command(&GOOD_MANIFEST.replace("command: ${ACTIONLINT:-actionlint}", "command: [42]"), false)]
#[case::invalid_actions("actions: invalid", false)]
#[case::invalid_yaml("actions: [", false)]
#[case::missing_name(&GOOD_MANIFEST.replace("name: actionlint", "description: actionlint"), false)]
#[case::duplicate_name(&GOOD_MANIFEST.replace("name: actionlint", "name: yamllint"), false)]
fn the_gate_judgement_detects_each_defect(#[case] manifest: &str, #[case] wired: bool) {
    assert_eq!(Manifest(manifest).gate_is_wired(), wired, "{manifest}");
}

/// Shared dependencies contribute their commands once, even through two paths.
#[test]
fn the_lint_closure_deduplicates_shared_actions() {
    let manifest = GOOD_MANIFEST.replace(
        "deps: [yamllint, actionlint]",
        "deps: [gate, yamllint, actionlint]\n  - name: gate\n    deps: [yamllint, actionlint]",
    );
    let commands = Manifest(&manifest)
        .lint_closure()
        .expect("a shared dependency graph has a valid lint closure");
    assert_eq!(commands.len(), 2);
}

const GOOD_WORKFLOW: &str = concat!(
    "jobs:\n  gate:\n    steps:\n      - uses: ",
    "leynos/shared-actions/.github/actions/install-yaml-lint@",
    "0123456789abcdef0123456789abcdef01234567\n      - run: netsuke build lint\n",
);

/// The installer must be pinned before a non-ignored lint step in the same job.
#[rstest]
#[case::pinned(GOOD_WORKFLOW, true)]
#[case::unpinned(&GOOD_WORKFLOW.replace("0123456789abcdef0123456789abcdef01234567", "v1"), false)]
#[case::absolute_netsuke(&GOOD_WORKFLOW.replace("run: netsuke", "run: /usr/bin/netsuke"), true)]
#[case::missing("jobs:\n  gate:\n    steps:\n      - run: netsuke build lint\n", false)]
#[case::other_job(&GOOD_WORKFLOW.replace("      - run:", "  other:\n    steps:\n      - run:"), false)]
#[case::ignored_failure(&GOOD_WORKFLOW.replace("netsuke build lint", "netsuke build lint || true"), false)]
#[case::continue_on_error(&GOOD_WORKFLOW.replace("      - run:", "        continue-on-error: true\n      - run:"), false)]
#[case::after(&GOOD_WORKFLOW.replace("      - uses:", "      - run: netsuke build lint\n      - uses:"), false)]
#[case::no_lint(&GOOD_WORKFLOW.replace("netsuke build lint", "netsuke build test"), false)]
#[case::cargo_only(&GOOD_WORKFLOW.replace("netsuke build lint", "cargo clippy --all-targets"), false)]
fn the_ci_judgement_detects_each_defect(#[case] workflow: &str, #[case] installs: bool) {
    assert_eq!(
        Workflow(workflow).installs_before_lint(),
        installs,
        "{workflow}"
    );
}

/// Apply the exercised judgement to the tracked manifest.
#[test]
fn netsuke_lint_runs_the_yaml_lint_gate() {
    assert!(
        Manifest(NETSUKEFILE).gate_is_wired(),
        "the lint closure lacks the gate"
    );
}

/// Apply installer ordering and provisioning contracts to the tracked workflow.
#[test]
fn ci_installs_the_tools_before_it_runs_the_gate() {
    assert!(Workflow(CI_WORKFLOW).installs_before_lint());
    assert!(!Workflow(CI_WORKFLOW).provisions_by_hand());
}

/// The repository overlay must inherit the canonical estate base unchanged.
#[test]
fn the_local_yamllint_configuration_extends_the_estate_base() {
    let keys: Vec<&str> = code_lines(YAMLLINT_CONFIG)
        .filter(|line| *line != "---")
        .collect();
    assert_eq!(keys, ["extends: .yamllint-estate.yml"]);
}

/// The workflow linter recognizes the estate's self-hosted runner labels.
#[test]
fn actionlint_accepts_the_estate_runner_labels() {
    assert!(code_lines(ACTIONLINT_CONFIG).any(|line| line == "- ubicloud-*"));
}

/// The estate base, as its code lines: the canonical configuration with
/// comments and blank lines dropped.
const CANONICAL_ESTATE_BASE: [&str; 28] = [
    "---",
    "extends: default",
    "ignore: |",
    "  **/charts/**/templates/",
    "rules:",
    "  document-start: disable",
    "  comments:",
    "    min-spaces-from-content: 1",
    "  indentation:",
    "    spaces: consistent",
    "    indent-sequences: consistent",
    "    check-multi-line-strings: false",
    "  braces:",
    "    min-spaces-inside: 0",
    "    max-spaces-inside: 1",
    "  brackets:",
    "    min-spaces-inside: 0",
    "    max-spaces-inside: 1",
    "  line-length:",
    "    max: 120",
    "    allow-non-breakable-words: true",
    "    allow-non-breakable-inline-mappings: true",
    "    ignore: |",
    "      .github/workflows/",
    "      .github/actions/",
    "  truthy:",
    "    allowed-values: ['true', 'false']",
    "    check-keys: false",
];

/// Every estate setting stays in canonical order and at its original depth.
#[test]
fn the_estate_base_is_the_canonical_configuration() {
    let base = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.yamllint-estate.yml"));
    // Every rule, heading and value, in order: a dropped or changed setting
    // fails even when its text survives under a sibling rule. The Concordat
    // `yaml-lint-baseline` rule audits the same file by digest.
    let code: Vec<&str> = base
        .lines()
        .map(|line| line.split(" #").next().unwrap_or_default().trim_end())
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect();
    assert_eq!(code, CANONICAL_ESTATE_BASE);
}
