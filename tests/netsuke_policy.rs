//! Structured contracts for the Netsuke manifest and its CI provisioning.

use serde_norway::Value;

/// Parse tracked YAML so keys and sequence order cannot hide in substrings.
fn yaml(contents: &str) -> Result<Value, serde_norway::Error> { serde_norway::from_str(contents) }

/// Find a named action in the parsed Netsukefile.
fn action<'a>(manifest: &'a Value, name: &str) -> Option<&'a Value> {
    manifest
        .get("actions")
        .and_then(Value::as_sequence)?
        .iter()
        .find(|action| action.get("name").and_then(Value::as_str) == Some(name))
}

/// Find a workflow job's ordered steps.
fn steps<'a>(workflow: &'a Value, job_name: &str) -> Option<&'a [Value]> {
    workflow
        .get("jobs")
        .and_then(|jobs| jobs.get(job_name))
        .and_then(|job_value| job_value.get("steps"))
        .and_then(Value::as_sequence)
        .map(Vec::as_slice)
}

/// Find a named step without accepting the same text in a different step.
fn step<'a>(steps: &'a [Value], name: &str) -> Option<&'a Value> {
    steps
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
}

/// Pin the public action surface and the serial default dependency graph.
#[test]
fn manifest_preserves_public_actions_and_serial_dependencies() {
    let manifest = yaml(include_str!("../Netsukefile")).expect("parse Netsukefile");
    let names: Vec<&str> = manifest
        .get("actions")
        .and_then(Value::as_sequence)
        .expect("manifest actions are a sequence")
        .iter()
        .map(|action| {
            action
                .get("name")
                .and_then(Value::as_str)
                .expect("named action")
        })
        .collect();
    assert_eq!(
        names,
        [
            "all",
            "clean",
            "test",
            "build",
            "release",
            "coverage",
            "lint",
            "rust-lint",
            "github-actions-lint",
            "typecheck",
            "fmt",
            "check-fmt",
            "markdownlint",
            "spelling",
            "nixie",
        ]
    );
    assert_eq!(
        manifest.get("defaults").and_then(Value::as_sequence),
        Some(&vec![Value::String("all".to_owned())])
    );
    for (name, expected) in [
        ("all", &["check-fmt", "lint", "test", "spelling"][..]),
        ("lint", &["rust-lint", "github-actions-lint"][..]),
    ] {
        let action = action(&manifest, name).expect("find serial action");
        let deps: Vec<&str> = action
            .get("deps")
            .and_then(Value::as_sequence)
            .expect("serial action has dependencies")
            .iter()
            .map(|dependency| dependency.as_str().expect("dependency is a name"))
            .collect();
        assert_eq!(deps, expected, "{name} dependencies changed");
        assert_eq!(
            action.get("dependency_order").and_then(Value::as_str),
            Some("serial")
        );
        assert!(
            action.get("command").is_none(),
            "{name} must remain dependency-only"
        );
    }
    let markdown_deps: Vec<&str> = action(&manifest, "markdownlint")
        .expect("find markdownlint action")
        .get("deps")
        .and_then(Value::as_sequence)
        .expect("markdownlint depends on spelling")
        .iter()
        .map(|dependency| dependency.as_str().expect("dependency is a name"))
        .collect();
    assert_eq!(markdown_deps, ["spelling"]);
}

/// Both jobs must pin the release and installation toolchain.
#[test]
fn ci_and_coverage_jobs_pin_netsuke_version() {
    let ci = yaml(include_str!("../.github/workflows/ci.yml")).expect("parse CI workflow");
    let coverage = yaml(include_str!("../.github/workflows/coverage-main.yml"))
        .expect("parse coverage workflow");
    for (workflow, job_name) in [(&ci, "build-test"), (&coverage, "coverage-upload")] {
        let job = workflow
            .get("jobs")
            .and_then(|jobs| jobs.get(job_name))
            .expect("find workflow job");
        let environment = job.get("env").expect("find job environment");
        assert_eq!(
            environment.get("NETSUKE_VERSION").and_then(Value::as_str),
            Some("0.1.0-beta4")
        );
        assert_eq!(
            environment.get("NETSUKE_TOOLCHAIN").and_then(Value::as_str),
            Some("nightly-2026-08-23")
        );
    }
}

/// Both jobs must cache the exact executable under the same keyed identity.
#[test]
fn ci_and_coverage_jobs_cache_netsuke_executable() {
    let ci = yaml(include_str!("../.github/workflows/ci.yml")).expect("parse CI workflow");
    let coverage = yaml(include_str!("../.github/workflows/coverage-main.yml"))
        .expect("parse coverage workflow");
    for (workflow, job_name) in [(&ci, "build-test"), (&coverage, "coverage-upload")] {
        let ordered = steps(workflow, job_name).expect("find job steps");
        let cache = step(ordered, "Cache Netsuke").expect("find Netsuke cache step");
        assert_eq!(
            cache.get("uses").and_then(Value::as_str),
            Some("actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9")
        );
        let settings = cache.get("with").expect("find Netsuke cache settings");
        assert_eq!(
            settings.get("path").and_then(Value::as_str),
            Some("~/.cargo/bin/netsuke")
        );
        assert_eq!(
            settings.get("key").and_then(Value::as_str),
            Some(
                "netsuke-${{ runner.os }}-${{ runner.arch }}-${{ env.NETSUKE_VERSION }}-${{ \
                 env.NETSUKE_TOOLCHAIN }}"
            )
        );
    }
}

/// Coverage must provision Netsuke and Ninja before running integration tests.
#[test]
fn ci_and_coverage_jobs_install_netsuke_before_tests() {
    let ci = yaml(include_str!("../.github/workflows/ci.yml")).expect("parse CI workflow");
    let coverage = yaml(include_str!("../.github/workflows/coverage-main.yml"))
        .expect("parse coverage workflow");
    for (workflow, job_name) in [(&ci, "build-test"), (&coverage, "coverage-upload")] {
        let ordered = steps(workflow, job_name).expect("find job steps");
        let install_position = ordered
            .iter()
            .position(|candidate| {
                candidate.get("name").and_then(Value::as_str)
                    == Some("Install Netsuke from crates.io")
            })
            .expect("find Netsuke install step");
        let coverage_position = ordered
            .iter()
            .position(|candidate| {
                candidate.get("name").and_then(Value::as_str) == Some("Test and Measure Coverage")
            })
            .expect("find coverage step");
        assert!(
            install_position < coverage_position,
            "Netsuke must precede coverage tests"
        );
        let install = ordered
            .get(install_position)
            .expect("find ordered install step");
        let script = install
            .get("run")
            .and_then(Value::as_str)
            .expect("find Netsuke install script");
        assert!(
            script.contains("rustup toolchain install \"${NETSUKE_TOOLCHAIN}\" --profile minimal")
        );
        assert!(script.contains("cargo \"+${NETSUKE_TOOLCHAIN}\" install --locked"));
        assert!(script.contains("netsuke-build --version \"=${NETSUKE_VERSION}\""));
    }
    let ordered = steps(&coverage, "coverage-upload").expect("find coverage job steps");
    let linkers = step(ordered, "Install coverage linkers").expect("find linker install step");
    assert!(
        linkers
            .get("run")
            .and_then(Value::as_str)
            .expect("find linker install command")
            .contains("ninja-build")
    );
}

/// CI must invoke the public gates rather than bypassing the manifest.
#[test]
fn ci_invokes_netsuke_gates() {
    let ci = yaml(include_str!("../.github/workflows/ci.yml")).expect("parse CI workflow");
    let ordered = steps(&ci, "build-test").expect("find CI job steps");
    for (name, expected) in [
        ("Format", "netsuke build check-fmt"),
        ("Spelling", "netsuke build spelling"),
        (
            "Lint",
            "ACTIONLINT=\"$GITHUB_WORKSPACE/actionlint\" netsuke build lint",
        ),
    ] {
        assert_eq!(
            step(ordered, name)
                .expect("find CI gate step")
                .get("run")
                .and_then(Value::as_str),
            Some(expected)
        );
    }
}
