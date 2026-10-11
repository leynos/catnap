//! Keeps the YAML lint gate wired into `make lint` and CI.
//!
//! `make lint` must run `yamllint --strict` over the Git-tracked YAML and
//! actionlint, and CI must install both through the pinned shared installer
//! before it runs the gate. The judgement is driven against fixtures first,
//! because a rule exercised only over this repository's own compliant files
//! would pass whether or not it detects anything, and then applied to the real
//! files. Everything is read as text, so the contract needs no parser
//! dependency; the Concordat `yaml-lint-baseline` rule audits the same wiring
//! with a full Makefile parse.

use rstest::rstest;

const MAKEFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Makefile"));
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
    /// The target a literal `$(MAKE) target` recipe line delegates to.
    fn delegated_target(self) -> Option<String> {
        self.0
            .strip_prefix("$(MAKE) ")
            .map(|next| next.trim().to_owned())
    }

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
            && line.contains("$(YAMLLINT)")
            && line.contains("--strict")
            && !line.contains("|| true")
    }

    /// Whether the line runs actionlint with its failure left to fail the target.
    fn is_sound_actionlint(self) -> bool {
        self.0.starts_with("$(ACTIONLINT)") && !self.0.contains("|| true")
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
            "run: make lint" | "run: /usr/bin/make lint"
        )
    }

    /// Whether the line mentions both `make` and `lint`.
    fn mentions_the_gate(self) -> bool { self.0.contains("make") && self.0.contains("lint") }

    /// Whether the line lets a failing step pass.
    fn ignores_failure(self) -> bool {
        self.0.contains("continue-on-error: true") || self.0.contains("|| true")
    }

    /// Whether the line provisions a lint tool by hand.
    fn provisions_by_hand(self) -> bool {
        let line = self.0;
        line.contains("uv tool install \"yamllint")
            || line.contains("download-actionlint")
            || line.contains("releases/download") && line.contains("actionlint")
    }
}

/// One Makefile rule: its targets' prerequisites and its recipe lines.
struct Rule {
    targets: Vec<String>,
    prerequisites: Vec<String>,
    recipe: Vec<String>,
}

/// A Makefile, read as text.
struct Makefile<'a>(&'a str);

impl Makefile<'_> {
    /// The rule a Makefile line opens, if it opens one.
    fn rule_header(line: &str) -> Option<Rule> {
        let assignment = [":=", "?="].iter().any(|operator| line.contains(operator));
        if line.starts_with('#') || assignment {
            return None;
        }
        let (targets, rest) = line.split_once(':')?;
        let prerequisites = rest.split("##").next().unwrap_or_default();
        Some(Rule {
            targets: targets.split_whitespace().map(str::to_owned).collect(),
            prerequisites: prerequisites
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            recipe: Vec::new(),
        })
    }

    /// The rules, joining nothing and expanding nothing.
    fn rules(&self) -> Vec<Rule> {
        let mut found: Vec<Rule> = Vec::new();
        for line in self.0.lines() {
            match (line.strip_prefix('\t'), found.last_mut()) {
                (Some(recipe), Some(rule)) => rule.recipe.push(recipe.trim().to_owned()),
                (Some(_), None) => {}
                (None, _) => found.extend(Self::rule_header(line)),
            }
        }
        found
    }

    /// Every recipe line reachable from `lint` through prerequisites and
    /// literal `$(MAKE) target` calls.
    fn lint_closure(&self) -> Vec<String> {
        let rules = self.rules();
        let mut pending = vec!["lint".to_owned()];
        let mut seen: Vec<String> = Vec::new();
        let mut lines = Vec::new();
        while let Some(target) = pending.pop() {
            if seen.contains(&target) {
                continue;
            }
            seen.push(target.clone());
            for rule in rules.iter().filter(|rule| rule.targets.contains(&target)) {
                pending.extend(rule.prerequisites.iter().cloned());
                pending.extend(
                    rule.recipe
                        .iter()
                        .filter_map(|line| Line(line).delegated_target()),
                );
                lines.extend(rule.recipe.iter().cloned());
            }
        }
        lines
    }

    /// Whether the closure runs a strict yamllint over `git ls-files` and
    /// actionlint.
    fn gate_is_wired(&self) -> bool {
        let lines = self.lint_closure();
        lines.iter().any(|line| Line(line).is_sound_yamllint())
            && lines.iter().any(|line| Line(line).is_sound_actionlint())
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

const GOOD_MAKEFILE: &str = "lint: yamllint actionlint\nyamllint:\n\tgit ls-files -z '*.yml' \
                             '*.yaml' | xargs -0 -r $(YAMLLINT) --strict \
                             --\nactionlint:\n\t$(ACTIONLINT)\n";

#[rstest]
#[case::wired(GOOD_MAKEFILE, true)]
#[case::delegated(
    "lint:\n\t$(MAKE) gate\ngate: yamllint actionlint\nyamllint:\n\tgit ls-files -z '*.yml' \
     '*.yaml' | xargs -0 -r $(YAMLLINT) --strict --\nactionlint:\n\t$(ACTIONLINT)\n",
    true
)]
#[case::not_strict(&GOOD_MAKEFILE.replace(" --strict", ""), false)]
#[case::one_pathspec(&GOOD_MAKEFILE.replace(" '*.yaml'", ""), false)]
#[case::workflows_only(&GOOD_MAKEFILE.replace("git ls-files -z '*.yml' '*.yaml' | xargs -0 -r $(YAMLLINT) --strict --", "$(YAMLLINT) --strict .github/workflows"), false)]
#[case::workflows_filter(&GOOD_MAKEFILE.replace("'*.yaml' |", "'*.yaml' .github/workflows |"), false)]
#[case::failure_ignored(&GOOD_MAKEFILE.replace("--strict --", "--strict -- || true"), false)]
#[case::yamllint_dash(&GOOD_MAKEFILE.replace("	git ls-files", "	-git ls-files"), false)]
#[case::yamllint_negated(&GOOD_MAKEFILE.replace("	git ls-files", "	! git ls-files"), false)]
#[case::actionlint_ignored(&GOOD_MAKEFILE.replace("	$(ACTIONLINT)", "	$(ACTIONLINT) || true"), false)]
#[case::actionlint_dash(&GOOD_MAKEFILE.replace("	$(ACTIONLINT)", "	-$(ACTIONLINT)"), false)]
#[case::actionlint_negated(&GOOD_MAKEFILE.replace("	$(ACTIONLINT)", "	! $(ACTIONLINT)"), false)]
#[case::outside_lint(&GOOD_MAKEFILE.replace("lint: yamllint actionlint", "lint:\n\ttrue\nother: yamllint actionlint"), false)]
#[case::no_actionlint(&GOOD_MAKEFILE.replace("\t$(ACTIONLINT)", "\ttrue"), false)]
fn the_gate_judgement_detects_each_defect(#[case] makefile: &str, #[case] wired: bool) {
    assert_eq!(Makefile(makefile).gate_is_wired(), wired, "{makefile}");
}

#[rstest]
#[case::pinned(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n      - run: make lint\n",
    true
)]
#[case::unpinned(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@v1\n      - run: make lint\n",
    false
)]
#[case::absolute_make(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n      - run: /usr/bin/make lint\n",
    true
)]
#[case::missing("jobs:\n  gate:\n    steps:\n      - run: make lint\n", false)]
#[case::other_job(
    "jobs:\n  tools:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n  gate:\n    steps:\n      - run: make lint\n",
    false
)]
#[case::same_job(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n      - run: make lint\n",
    true
)]
#[case::ignored_failure(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n      - run: make lint || true\n",
    false
)]
#[case::after(
    "jobs:\n  gate:\n    steps:\n      - run: make lint\n  - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n",
    false
)]
#[case::no_lint(
    "jobs:\n  gate:\n    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n      - run: make test\n",
    false
)]
fn the_ci_judgement_detects_each_defect(#[case] workflow: &str, #[case] installs: bool) {
    assert_eq!(
        Workflow(workflow).installs_before_lint(),
        installs,
        "{workflow}"
    );
}

#[test]
fn make_lint_runs_the_yaml_lint_gate() {
    assert!(
        Makefile(MAKEFILE).gate_is_wired(),
        "the lint closure lacks the gate"
    );
}

#[test]
fn ci_installs_the_tools_before_it_runs_the_gate() {
    assert!(Workflow(CI_WORKFLOW).installs_before_lint());
    assert!(!Workflow(CI_WORKFLOW).provisions_by_hand());
}

#[test]
fn the_local_yamllint_configuration_extends_the_estate_base() {
    let keys: Vec<&str> = code_lines(YAMLLINT_CONFIG)
        .filter(|line| *line != "---")
        .collect();
    assert_eq!(keys, ["extends: .yamllint-estate.yml"]);
}

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
