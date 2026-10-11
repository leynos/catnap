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

/// One Makefile rule: its targets' prerequisites and its recipe lines.
struct Rule {
    targets: Vec<String>,
    prerequisites: Vec<String>,
    recipe: Vec<String>,
}

/// Reads the rules of a Makefile, joining nothing and expanding nothing.
fn rules(makefile: &str) -> Vec<Rule> {
    let mut found: Vec<Rule> = Vec::new();
    for line in makefile.lines() {
        if let Some(recipe) = line.strip_prefix('\t') {
            if let Some(rule) = found.last_mut() {
                rule.recipe.push(recipe.trim().to_owned());
            }
            continue;
        }
        if line.starts_with('#')
            || [":=", "?="]
                .iter()
                .any(|assignment| line.contains(assignment))
        {
            continue;
        }
        if let Some((targets, rest)) = line.split_once(':') {
            let prerequisites = rest.split("##").next().unwrap_or_default();
            found.push(Rule {
                targets: targets.split_whitespace().map(str::to_owned).collect(),
                prerequisites: prerequisites
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
                recipe: Vec::new(),
            });
        }
    }
    found
}

/// Returns the target a literal `$(MAKE) target` recipe line delegates to.
fn delegated_target(line: &str) -> Option<String> {
    line.strip_prefix("$(MAKE) ")
        .map(|next| next.trim().to_owned())
}

/// Returns every recipe line reachable from `lint` through prerequisites and
/// literal `$(MAKE) target` calls.
fn lint_closure(makefile: &str) -> Vec<String> {
    let rules = rules(makefile);
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
            pending.extend(rule.recipe.iter().filter_map(|line| delegated_target(line)));
            lines.extend(rule.recipe.iter().cloned());
        }
    }
    lines
}

/// Returns whether the closure runs a strict yamllint over `git ls-files` and
/// actionlint, in that command position.
fn lint_gate_is_wired(makefile: &str) -> bool {
    let lines = lint_closure(makefile);
    let yamllint = lines.iter().any(|line| {
        line.starts_with("git ls-files")
            && line.contains("'*.yml'")
            && line.contains("'*.yaml'")
            && line.contains("| xargs")
            && line.contains("$(YAMLLINT)")
            && line.contains("--strict")
    });
    let actionlint = lines.iter().any(|line| line.starts_with("$(ACTIONLINT)"));
    yamllint && actionlint
}

/// Returns whether a job installs the tools, pinned, before it runs the gate.
fn ci_installs_before_lint(workflow: &str) -> bool {
    let lines: Vec<&str> = code_lines(workflow).collect();
    let lint = lines.iter().position(|line| {
        matches!(
            line.trim_start_matches("- "),
            "run: make lint" | "run: /usr/bin/make lint"
        )
    });
    let install = lines.iter().position(|line| {
        line.trim_start_matches("- ")
            .strip_prefix("uses: ")
            .and_then(|uses| uses.strip_prefix(INSTALLER))
            .is_some_and(|reference| {
                reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
            })
    });
    matches!((install, lint), (Some(first), Some(second)) if first < second)
}

/// Returns whether CI still provisions a lint tool by hand.
fn ci_provisions_by_hand(workflow: &str) -> bool {
    code_lines(workflow).any(|line| {
        line.contains("uv tool install \"yamllint")
            || line.contains("download-actionlint")
            || line.contains("releases/download") && line.contains("actionlint")
    })
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
#[case::outside_lint(&GOOD_MAKEFILE.replace("lint: yamllint actionlint", "lint:\n\ttrue\nother: yamllint actionlint"), false)]
#[case::no_actionlint(&GOOD_MAKEFILE.replace("\t$(ACTIONLINT)", "\ttrue"), false)]
fn the_gate_judgement_detects_each_defect(#[case] makefile: &str, #[case] wired: bool) {
    assert_eq!(lint_gate_is_wired(makefile), wired, "{makefile}");
}

#[rstest]
#[case::pinned(
    "steps:\n  - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n  - run: make lint\n",
    true
)]
#[case::unpinned(
    "steps:\n  - uses: leynos/shared-actions/.github/actions/install-yaml-lint@v1\n  - run: make \
     lint\n",
    false
)]
#[case::absolute_make(
    "steps:\n  - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n  - run: /usr/bin/make lint\n",
    true
)]
#[case::missing("steps:\n  - run: make lint\n", false)]
#[case::after(
    "steps:\n  - run: make lint\n  - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n",
    false
)]
#[case::no_lint(
    "steps:\n  - uses: \
     leynos/shared-actions/.github/actions/install-yaml-lint@\
     0123456789abcdef0123456789abcdef01234567\n  - run: make test\n",
    false
)]
fn the_ci_judgement_detects_each_defect(#[case] workflow: &str, #[case] installs: bool) {
    assert_eq!(ci_installs_before_lint(workflow), installs, "{workflow}");
}

#[test]
fn make_lint_runs_the_yaml_lint_gate() {
    assert!(
        lint_gate_is_wired(MAKEFILE),
        "the lint closure lacks the gate"
    );
}

#[test]
fn ci_installs_the_tools_before_it_runs_the_gate() {
    assert!(ci_installs_before_lint(CI_WORKFLOW));
    assert!(!ci_provisions_by_hand(CI_WORKFLOW));
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
