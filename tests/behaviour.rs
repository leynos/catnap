//! Behavioural tests for GNU-like sleep command parsing.

use anyhow::Context;
use catnap::{CliError, CommandAction, parse_command};
use rstest::fixture;
use rstest_bdd::{ScenarioState as _, Slot};
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

#[derive(Default, ScenarioState)]
struct SleepState {
    operands: Slot<Vec<String>>,
    duration_seconds: Slot<u64>,
    error_message: Slot<String>,
}

#[fixture]
fn sleep_state() -> SleepState {
    let state = SleepState::default();
    state.reset();
    state
}

#[given("sleep operands {operands:string}")]
fn sleep_operands(sleep_state: &SleepState, operands: &str) {
    sleep_state.operands.set(
        operands
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
    );
}

#[when(expr = "the sleep command is parsed", result)]
fn parse_sleep_command(sleep_state: &SleepState) -> anyhow::Result<()> {
    let operands = sleep_state
        .operands
        .get()
        .context("sleep operands must be supplied before parsing")?;
    let args = std::iter::once("catnap".to_owned())
        .chain(operands)
        .collect::<Vec<_>>();
    store_parse_result(sleep_state, parse_command(args));
    Ok(())
}

#[test]
fn parsing_without_sleep_operands_fails_the_step() {
    let state = SleepState::default();
    let error = parse_sleep_command(&state).expect_err("missing operands must fail the step");
    assert!(
        error
            .to_string()
            .contains("sleep operands must be supplied"),
        "unexpected missing-operand error: {error}"
    );
}

#[then("the parsed duration is {seconds:u64} seconds")]
fn parsed_duration_is(sleep_state: &SleepState, seconds: u64) {
    assert_eq!(sleep_state.duration_seconds.get(), Some(seconds));
}

#[then("the command error mentions {expected:string}")]
fn command_error_mentions(sleep_state: &SleepState, expected: &str) {
    let Some(error) = sleep_state.error_message.get() else {
        panic!("expected command error");
    };
    assert!(
        error.contains(expected),
        "expected '{error}' to contain '{expected}'"
    );
}

fn store_parse_result(sleep_state: &SleepState, result: Result<CommandAction, CliError>) {
    match result {
        Ok(CommandAction::Sleep(command)) => {
            sleep_state
                .duration_seconds
                .set(command.duration().as_secs());
        }
        Ok(other) => {
            sleep_state
                .error_message
                .set(format!("unexpected command action: {other:?}"));
        }
        Err(error) => {
            sleep_state.error_message.set(error.to_string());
        }
    }
}

#[scenario(
    path = "tests/features/sleep_cli.feature",
    name = "Sum suffixed operands"
)]
fn sum_suffixed_operands(#[from(sleep_state)] _sleep_state: SleepState) {}

#[scenario(
    path = "tests/features/sleep_cli.feature",
    name = "Reject invalid suffixes"
)]
fn reject_invalid_suffixes(#[from(sleep_state)] _sleep_state: SleepState) {}
