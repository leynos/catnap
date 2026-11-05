//! The main-branch coverage publisher runs the whole suite, whose
//! formatter-boundary tests execute `mdtablefix`, so it must install the tool
//! before it measures coverage. Without it every run failed before the upload.

use serde_norway::Value;

/// The publisher workflow under contract.
const PUBLISHER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/workflows/coverage-main.yml"
));

/// Returns the index of the first step whose `uses` mentions `needle`.
fn step_using(steps: &[Value], needle: &str) -> Option<usize> {
    steps.iter().position(|step| {
        step.get("uses")
            .and_then(Value::as_str)
            .is_some_and(|uses| uses.contains(needle))
    })
}

#[test]
fn the_publisher_installs_mdtablefix_before_measuring_coverage() {
    let workflow: Value = serde_norway::from_str(PUBLISHER).expect("parse the publisher");
    let jobs = workflow
        .get("jobs")
        .and_then(Value::as_mapping)
        .expect("the publisher has jobs");
    let installed_before_coverage = jobs.values().any(|job| {
        let steps = job
            .get("steps")
            .and_then(Value::as_sequence)
            .map_or(&[][..], Vec::as_slice);
        match (
            step_using(steps, "/install-mdtablefix@"),
            step_using(steps, "/generate-coverage@"),
        ) {
            (Some(install), Some(coverage)) => install < coverage,
            _ => false,
        }
    });
    assert!(
        installed_before_coverage,
        "coverage-main.yml must install mdtablefix before generate-coverage"
    );
}
