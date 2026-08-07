//! Snapshot tests for locale-aware progress output.

use std::{ffi::OsString, time::Duration};

use catnap::{format_remaining_time, run_application};

#[test]
fn snapshots_representative_remaining_time_lines() {
    let lines = [
        format_remaining_time(Duration::from_secs(19), "en-GB"),
        format_remaining_time(Duration::from_secs(65), "en-US"),
        format_remaining_time(Duration::from_secs(2), "fr-FR"),
    ]
    .join("");

    insta::assert_snapshot!(
        &lines,
        @r###"
19 seconds remaining
1 minute 5 seconds remaining
2 secondes restantes
"###
    );
}

#[test]
fn snapshots_compound_duration_diagnostic() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let _status = run_application(
        [OsString::from("catnap"), OsString::from("5h20m")],
        &mut stdout,
        &mut stderr,
    );

    assert!(stdout.is_empty());
    insta::assert_snapshot!(
        String::from_utf8(stderr).expect("diagnostic should be valid UTF-8"),
        @r###"
catnap: invalid time interval '5h20m'
catnap: durations are separate operands; did you mean 'catnap 5h 20m'?
Try 'catnap --help' for more information.
"###
    );
}
