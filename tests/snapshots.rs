//! Snapshot tests for locale-aware progress output.

use std::time::Duration;

use vsleep::format_remaining_time;

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
