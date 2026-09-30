//! Holds the hosted sccache lane's shape (developers' guide, "Compiler cache").
//!
//! On a GitHub-hosted runner the shared `setup-rust` action saves its sccache
//! directory only on a push to `main`, and keys it on a discriminator that
//! defaults to the job id. Three things must therefore hold, each judged by
//! the action's name and inputs and never by a revision the pin bump moves:
//! every `setup-rust` step names `expect-cache: any`; the pull-request
//! `build-test` step and the publisher's step, the only one that runs on a
//! push to `main`, share one explicit discriminator; and the release job,
//! which builds with `cross` in a container that receives neither
//! `RUSTC_WRAPPER` nor `SCCACHE_PATH`, turns sccache off. Each test mutates a
//! copy of this repository's workflows and asserts the clause meant to catch
//! it does.

use std::collections::BTreeMap;

use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;
use serde_norway::{Mapping, Value};

const SETUP_RUST: &str = "leynos/shared-actions/.github/actions/setup-rust@";
const DISCRIMINATOR: &str = "sccache-cache-discriminator";
const READER: (&str, &str) = ("ci.yml", "build-test");
const WRITER: (&str, &str) = ("coverage-main.yml", "coverage-upload");
const RELEASE: (&str, &str) = ("release.yml", "build");

type Workflows = BTreeMap<String, Value>;

/// Reads the three workflows the lane spans, by file name.
fn read_workflows() -> Result<Workflows, String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| error.to_string())?;
    [READER.0, WRITER.0, RELEASE.0]
        .into_iter()
        .map(|name| {
            let text = root
                .read_to_string(format!(".github/workflows/{name}"))
                .map_err(|error| error.to_string())?;
            let document = serde_norway::from_str(&text).map_err(|error| error.to_string())?;
            Ok((name.to_owned(), document))
        })
        .collect()
}

/// Returns the `setup-rust` steps of one job, or an empty list.
fn setup_steps<'a>(workflows: &'a Workflows, (file, job): (&str, &str)) -> Vec<&'a Mapping> {
    workflows
        .get(file)
        .and_then(|workflow| workflow.get("jobs")?.get(job)?.get("steps")?.as_sequence())
        .map(|steps| {
            steps
                .iter()
                .filter_map(Value::as_mapping)
                .filter(|step| {
                    step.get("uses")
                        .and_then(Value::as_str)
                        .is_some_and(|uses| uses.starts_with(SETUP_RUST))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Returns one input of a step's `with:`, treating a bare `with:` as empty.
fn input<'a>(step: &'a Mapping, name: &str) -> Option<&'a Value> { step.get("with")?.get(name) }

/// Returns the `with:` mapping of the first `setup-rust` step of a job to edit.
fn first_with_mut<'a>(
    workflows: &'a mut Workflows,
    (file, job): (&str, &str),
) -> Option<&'a mut Mapping> {
    workflows
        .get_mut(file)?
        .get_mut("jobs")?
        .get_mut(job)?
        .get_mut("steps")?
        .as_sequence_mut()?
        .iter_mut()
        .find(|step| {
            step.get("uses")
                .and_then(Value::as_str)
                .is_some_and(|uses| uses.starts_with(SETUP_RUST))
        })?
        .get_mut("with")?
        .as_mapping_mut()
}

/// Lists every way the workflows break the hosted sccache lane.
fn violations(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    for side in [READER, WRITER, RELEASE] {
        for step in setup_steps(workflows, side) {
            if input(step, "expect-cache").and_then(Value::as_str) != Some("any") {
                problems.push(format!(
                    "{}: a setup-rust step lacks `expect-cache: any`",
                    side.0
                ));
            }
        }
    }
    for step in setup_steps(workflows, RELEASE) {
        let is_off = input(step, "use-sccache")
            .is_some_and(|value| value.as_bool() == Some(false) || value.as_str() == Some("false"));
        if !is_off {
            problems.push("release.yml: a cross build must set `use-sccache: 'false'`".to_owned());
        }
    }
    let lanes = |side| -> Vec<Option<&str>> {
        setup_steps(workflows, side)
            .into_iter()
            .map(|step| input(step, DISCRIMINATOR).and_then(Value::as_str))
            .collect()
    };
    let (reader, writer) = (lanes(READER), lanes(WRITER));
    if reader.is_empty() || writer.is_empty() {
        problems.push("the reader and the writer must each run setup-rust".to_owned());
    } else if !reader
        .iter()
        .chain(&writer)
        .all(|lane| lane.is_some_and(|name| !name.is_empty()) && Some(lane) == reader.first())
    {
        problems.push("reader and writer must share one explicit discriminator".to_owned());
    }
    problems
}

/// Returns the violations after applying one mutation to a fresh copy.
fn violations_after(
    mutation: impl FnOnce(&mut Workflows) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let mut workflows = read_workflows()?;
    mutation(&mut workflows)?;
    Ok(violations(&workflows))
}

/// Fails unless some violation contains `fragment`.
fn assert_reports(found: &[String], fragment: &str) {
    assert!(
        found.iter().any(|problem| problem.contains(fragment)),
        "expected a violation naming {fragment:?}, got {found:?}"
    );
}

/// Sets or removes one input on the first `setup-rust` step of a job.
fn set_input(
    workflows: &mut Workflows,
    side: (&str, &str),
    name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let with = first_with_mut(workflows, side)
        .ok_or_else(|| format!("{}:{} has no setup-rust step with inputs", side.0, side.1))?;
    match value {
        Some(text) => with.insert(name.into(), text.into()),
        None => with.remove(name),
    };
    Ok(())
}

#[test]
fn repository_workflows_hold_the_lane() {
    let found = violations_after(|_| Ok(())).expect("the workflows should be readable");
    assert!(found.is_empty(), "expected no violations, got {found:?}");
}

#[rstest]
#[case::reader_missing(READER, None)]
#[case::reader_other_value(READER, Some("github"))]
#[case::release_missing(RELEASE, None)]
fn an_unnamed_or_different_expect_cache_is_refused(
    #[case] side: (&str, &str),
    #[case] value: Option<&str>,
) {
    let found = violations_after(|workflows| set_input(workflows, side, "expect-cache", value))
        .expect("the workflows should be readable");
    assert_reports(&found, "expect-cache");
}

#[rstest]
#[case::reader_drops_it(READER, None)]
#[case::writer_drops_it(WRITER, None)]
#[case::the_ends_differ(WRITER, Some("other"))]
#[case::both_empty(WRITER, Some(""))]
fn a_lane_without_one_explicit_shared_discriminator_is_refused(
    #[case] side: (&str, &str),
    #[case] value: Option<&str>,
) {
    let found = violations_after(|workflows| {
        if value == Some("") {
            set_input(workflows, READER, DISCRIMINATOR, value)?;
        }
        set_input(workflows, side, DISCRIMINATOR, value)
    })
    .expect("the workflows should be readable");
    assert_reports(&found, "discriminator");
}

#[test]
fn a_lane_without_a_writer_is_refused() {
    let found = violations_after(|workflows| {
        workflows
            .get_mut(WRITER.0)
            .and_then(|workflow| {
                workflow
                    .get_mut("jobs")?
                    .get_mut(WRITER.1)?
                    .as_mapping_mut()
            })
            .ok_or_else(|| "the publisher job should exist".to_owned())?
            .insert("steps".into(), Value::Sequence(Vec::new()));
        Ok(())
    })
    .expect("the workflows should be readable");
    assert_reports(&found, "must each run");
}

#[rstest]
#[case::missing(None)]
#[case::switched_on(Some("true"))]
fn a_release_with_sccache_on_is_refused(#[case] value: Option<&str>) {
    let found = violations_after(|workflows| set_input(workflows, RELEASE, "use-sccache", value))
        .expect("the workflows should be readable");
    assert_reports(&found, "cross");
}
