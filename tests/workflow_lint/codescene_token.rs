//! Checks the narrow local `CodeScene` token-step naming and command contract.

use std::error::Error;

use rstest::rstest;

use super::{read_repository_file, workflow_step, workflow_step_field};

const COVERAGE_WORKFLOW: &str = ".github/workflows/coverage-main.yml";
const TOKEN_STEP_NAME: &str = "Check for the CodeScene token";
const UPLOAD_STEP_NAME: &str = "Upload coverage data to CodeScene";
const TOKEN_STEP_PREFIX: &str = "      - name: ";
const STEP_RUN_PREFIX: &str = "        run: ";
const TOKEN_CHECK_COMMAND: &str =
    r#"echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT""#;
const UPLOAD_WITH_HEADER: &str = "        with:\n";
const UPLOAD_MODE_LINE: &str = "          mode: upload\n";
const UPLOAD_MODE_PREFIX: &str = "          mode: ";
const UPLOAD_ACCESS_TOKEN_LINE: &str = "          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n";

#[derive(Debug, Eq, PartialEq)]
enum TokenStepContractFailure {
    CanonicalStepCount(usize),
    ExactCommandCount(usize),
    MissingCanonicalStep,
    MissingRunCommand,
    CanonicalStepCommandMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UploadModeContractFailure {
    MissingUploadStep,
    MissingWithBlock,
    ModeFieldCount(usize),
    ModeValueMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenUploadOrderFailure {
    MissingTokenStep,
    MissingUploadStep,
    TokenStepAfterUpload,
}

fn validate_token_step_contract(workflow: &str) -> Result<(), TokenStepContractFailure> {
    let canonical_step_count = workflow
        .lines()
        .filter(|line| line.strip_prefix(TOKEN_STEP_PREFIX) == Some(TOKEN_STEP_NAME))
        .count();
    if canonical_step_count != 1 {
        return Err(TokenStepContractFailure::CanonicalStepCount(
            canonical_step_count,
        ));
    }

    let exact_command_count = workflow
        .lines()
        .filter_map(|line| line.strip_prefix(STEP_RUN_PREFIX))
        .filter(|command| *command == TOKEN_CHECK_COMMAND)
        .count();
    if exact_command_count != 1 {
        return Err(TokenStepContractFailure::ExactCommandCount(
            exact_command_count,
        ));
    }

    let token_step = workflow_step(workflow, TOKEN_STEP_NAME)
        .map_err(|_| TokenStepContractFailure::MissingCanonicalStep)?;
    let token_command = workflow_step_field(token_step, "run")
        .map_err(|_| TokenStepContractFailure::MissingRunCommand)?;
    if token_command != TOKEN_CHECK_COMMAND {
        return Err(TokenStepContractFailure::CanonicalStepCommandMismatch);
    }

    Ok(())
}

fn validate_upload_mode_contract(workflow: &str) -> Result<(), UploadModeContractFailure> {
    let upload_step = workflow_step(workflow, UPLOAD_STEP_NAME)
        .map_err(|_| UploadModeContractFailure::MissingUploadStep)?;
    let (_, with_fields) = upload_step
        .split_once(UPLOAD_WITH_HEADER)
        .ok_or(UploadModeContractFailure::MissingWithBlock)?;
    let modes: Vec<&str> = with_fields
        .lines()
        .take_while(|line| is_with_mapping_line(line))
        .filter_map(|line| line.strip_prefix(UPLOAD_MODE_PREFIX))
        .collect();
    if modes.len() != 1 {
        return Err(UploadModeContractFailure::ModeFieldCount(modes.len()));
    }
    if modes.first().copied() != Some("upload") {
        return Err(UploadModeContractFailure::ModeValueMismatch);
    }

    Ok(())
}

fn validate_token_step_precedes_upload(workflow: &str) -> Result<(), TokenUploadOrderFailure> {
    let token_step_line = workflow
        .lines()
        .position(|line| line.strip_prefix(TOKEN_STEP_PREFIX) == Some(TOKEN_STEP_NAME))
        .ok_or(TokenUploadOrderFailure::MissingTokenStep)?;
    let upload_step_line = workflow
        .lines()
        .position(|line| line.strip_prefix(TOKEN_STEP_PREFIX) == Some(UPLOAD_STEP_NAME))
        .ok_or(TokenUploadOrderFailure::MissingUploadStep)?;

    if token_step_line >= upload_step_line {
        return Err(TokenUploadOrderFailure::TokenStepAfterUpload);
    }

    Ok(())
}

fn is_with_mapping_line(line: &str) -> bool {
    line.trim().is_empty() || line.len() - line.trim_start().len() > 8
}

fn coverage_workflow() -> Result<String, Box<dyn Error>> { read_repository_file(COVERAGE_WORKFLOW) }

#[rstest]
fn main_coverage_has_one_canonical_token_step_and_command() {
    let workflow = coverage_workflow().expect("read main coverage workflow");

    assert_eq!(validate_token_step_contract(&workflow), Ok(()));
    assert_eq!(validate_upload_mode_contract(&workflow), Ok(()));
    assert_eq!(validate_token_step_precedes_upload(&workflow), Ok(()));
}

#[rstest]
#[case::omitted("", UploadModeContractFailure::ModeFieldCount(0))]
#[case::changed(
    "          mode: check\n",
    UploadModeContractFailure::ModeValueMismatch
)]
fn contract_rejects_an_omitted_or_changed_upload_mode(
    #[case] replacement: &str,
    #[case] expected_failure: UploadModeContractFailure,
) {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let changed = workflow.replacen(UPLOAD_MODE_LINE, replacement, 1);

    assert_ne!(workflow, changed, "the test mutation must apply");
    assert_eq!(
        validate_upload_mode_contract(&changed),
        Err(expected_failure)
    );
}

#[rstest]
fn contract_rejects_upload_mode_moved_to_a_sibling_env_block() {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let without_mode = workflow.replacen(UPLOAD_MODE_LINE, "", 1);
    let moved = without_mode.replacen(
        UPLOAD_ACCESS_TOKEN_LINE,
        concat!(
            "          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n",
            "        env:\n",
            "          mode: upload\n"
        ),
        1,
    );

    assert_ne!(workflow, without_mode, "mode removal mutation must apply");
    assert_ne!(without_mode, moved, "sibling env mutation must apply");
    assert_eq!(
        validate_upload_mode_contract(&moved),
        Err(UploadModeContractFailure::ModeFieldCount(0))
    );
}

#[rstest]
fn contract_rejects_token_step_reordered_after_upload() {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let token_start = workflow
        .find("      - name: Check for the CodeScene token\n")
        .expect("find token step start");
    let upload_start = workflow
        .find("      - name: Upload coverage data to CodeScene\n")
        .expect("find upload step start");
    assert!(
        token_start < upload_start,
        "mutation anchors must be ordered"
    );

    let prefix = workflow
        .get(..token_start)
        .expect("slice workflow prefix before token step");
    let token_block = workflow
        .get(token_start..upload_start)
        .expect("slice token step before uploader");
    let upload_block = workflow
        .get(upload_start..)
        .expect("slice uploader and remaining workflow");
    let reordered = format!("{prefix}{upload_block}{token_block}");

    assert_ne!(workflow, reordered, "the test mutation must apply");
    assert_eq!(
        validate_token_step_precedes_upload(&reordered),
        Err(TokenUploadOrderFailure::TokenStepAfterUpload)
    );
}

#[rstest]
fn contract_rejects_a_renamed_token_step() {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let renamed = workflow.replacen(
        "      - name: Check for the CodeScene token\n",
        "      - name: Check CodeScene token\n",
        1,
    );

    assert_ne!(workflow, renamed, "the test mutation must apply");
    assert_eq!(
        validate_token_step_contract(&renamed),
        Err(TokenStepContractFailure::CanonicalStepCount(0))
    );
}

#[rstest]
fn contract_rejects_a_changed_token_check_command() {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let changed = workflow.replacen(TOKEN_CHECK_COMMAND, "echo available=true", 1);

    assert_ne!(workflow, changed, "the test mutation must apply");
    assert_eq!(
        validate_token_step_contract(&changed),
        Err(TokenStepContractFailure::ExactCommandCount(0))
    );
}

#[rstest]
fn contract_rejects_the_exact_command_duplicated_in_another_step() {
    let workflow = coverage_workflow().expect("read main coverage workflow");
    let upload_step = "      - name: Upload coverage data to CodeScene\n";
    let duplicate_and_upload_steps = format!(
        "      - name: Duplicate token check\n        run: {TOKEN_CHECK_COMMAND}\n{upload_step}"
    );
    let duplicated = workflow.replacen(upload_step, &duplicate_and_upload_steps, 1);

    assert_ne!(workflow, duplicated, "the test mutation must apply");
    assert_eq!(
        validate_token_step_contract(&duplicated),
        Err(TokenStepContractFailure::ExactCommandCount(2))
    );
}
