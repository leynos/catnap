//! Parses workflow YAML and builds mutation fixtures for build-tool contracts.

use cap_std::{ambient_authority, fs_utf8::Dir};
use serde_norway::Value;

const SETUP_RUST: &str =
    "leynos/shared-actions/.github/actions/setup-rust@6cec89bac47a21cf756d68d638a9a510998e57f8";
const GENERATE_COVERAGE: &str = "leynos/shared-actions/.github/actions/generate-coverage@";
const INSTALL_CRANELIFT: &str = "make install-cranelift";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunnerPlatform {
    Linux,
    Other,
    Unknown,
}

pub(super) fn repository_root() -> Result<Dir, String> {
    Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| error.to_string())
}

pub(super) fn workflow_names(root: &Dir) -> Result<Vec<String>, String> {
    let workflow_directory = root
        .open_dir(".github/workflows")
        .map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    for entry_result in workflow_directory
        .read_dir(".")
        .map_err(|error| error.to_string())?
    {
        let entry = entry_result.map_err(|error| error.to_string())?;
        let name = entry.file_name().map_err(|error| error.to_string())?;
        let file_extension = name.rsplit_once('.').map(|(_, extension)| extension);
        let is_workflow = entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && file_extension.is_some_and(|extension_name| {
                extension_name.eq_ignore_ascii_case("yml")
                    || extension_name.eq_ignore_ascii_case("yaml")
            });
        if is_workflow {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub(super) fn workflow_violations(workflow: &Value, source: &str) -> Result<Vec<String>, String> {
    let jobs = workflow
        .get("jobs")
        .and_then(Value::as_mapping)
        .ok_or_else(|| format!("{source}: workflow has no jobs mapping"))?;
    let mut violations = Vec::new();

    for (name, job) in jobs {
        let job_name = name.as_str().unwrap_or("<unnamed>");
        violations.extend(job_violations(job, source, job_name)?);
    }
    Ok(violations)
}

fn job_violations(job: &Value, source: &str, job_name: &str) -> Result<Vec<String>, String> {
    let Some(steps) = job.get("steps").and_then(Value::as_sequence) else {
        return Ok(Vec::new());
    };
    if !steps.iter().any(is_rust_suite_step) {
        return Ok(Vec::new());
    }

    match classify_job_runner(job) {
        RunnerPlatform::Other => return Ok(Vec::new()),
        RunnerPlatform::Unknown => {
            return Err(format!(
                "{source}: Rust suite job {job_name} has an unclassifiable runs-on"
            ));
        }
        RunnerPlatform::Linux => {}
    }

    let mut violations = Vec::new();
    for suite_index in steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| is_rust_suite_step(step).then_some(index))
    {
        violations.extend(suite_setup_violations(steps, suite_index, source, job_name));
        if steps
            .get(suite_index)
            .is_some_and(is_development_suite_step)
        {
            violations.extend(cranelift_setup_violations(
                steps,
                suite_index,
                source,
                job_name,
            ));
        }
    }
    Ok(violations)
}

fn cranelift_setup_violations(
    steps: &[Value],
    suite_index: usize,
    source: &str,
    job_name: &str,
) -> Vec<String> {
    let setup = steps
        .iter()
        .enumerate()
        .take(suite_index)
        .find(|(_, step)| is_cranelift_setup(step));
    let Some((_, setup_step)) = setup else {
        return vec![format!(
            "{source}: Linux Rust development suite job {job_name} has no earlier Cranelift setup"
        )];
    };

    if has_field(setup_step, "if") || has_field(setup_step, "continue-on-error") {
        return vec![format!(
            "{source}: Linux Rust development suite job {job_name} has a conditional or ignored \
             Cranelift setup"
        )];
    }
    Vec::new()
}

fn is_cranelift_setup(step: &Value) -> bool {
    step.get("run")
        .and_then(Value::as_str)
        .is_some_and(|run| run.trim() == INSTALL_CRANELIFT)
}

fn is_development_suite_step(step: &Value) -> bool {
    step.get("run")
        .and_then(Value::as_str)
        .is_some_and(|run| run_invokes_rust_suite(run) && !run.contains("llvm-cov"))
}

fn suite_setup_violations(
    steps: &[Value],
    suite_index: usize,
    source: &str,
    job_name: &str,
) -> Vec<String> {
    let setup = steps
        .iter()
        .enumerate()
        .take(suite_index)
        .find(|(_, step)| is_pinned_mold_setup(step));
    let Some((setup_index, setup_step)) = setup else {
        return vec![format!(
            "{source}: Linux Rust suite job {job_name} has no earlier pinned mold setup"
        )];
    };

    let mut violations = Vec::new();
    if has_field(setup_step, "if") || has_field(setup_step, "continue-on-error") {
        violations.push(format!(
            "{source}: Linux Rust suite job {job_name} has a conditional or ignored mold setup"
        ));
    }
    if steps
        .iter()
        .skip(setup_index + 1)
        .any(is_distro_mold_install)
    {
        violations.push(format!(
            "{source}: Linux Rust suite job {job_name} installs distro mold after pinned setup"
        ));
    }
    violations
}

fn is_pinned_mold_setup(step: &Value) -> bool {
    step.get("uses").and_then(Value::as_str) == Some(SETUP_RUST)
        && step
            .get("with")
            .and_then(|with| with.get("install-mold"))
            .is_some_and(|value| value.as_bool() == Some(true) || value.as_str() == Some("true"))
}

fn has_field(step: &Value, field: &str) -> bool {
    step.as_mapping()
        .is_some_and(|mapping| mapping.contains_key(field))
}

fn is_distro_mold_install(step: &Value) -> bool {
    step.get("run")
        .and_then(Value::as_str)
        .is_some_and(run_installs_distro_mold)
}

pub(super) fn run_installs_distro_mold(run: &str) -> bool {
    let tokens = run.split_whitespace().collect::<Vec<_>>();
    tokens.windows(2).enumerate().any(|(index, pair)| {
        let [program, subcommand] = pair else {
            return false;
        };
        is_package_install_command(clean_token(program), clean_token(subcommand))
            && tokens
                .iter()
                .skip(index.saturating_add(2))
                .take_while(|token| !is_shell_boundary(token))
                .any(is_mold_package_token)
    })
}

fn is_package_install_command(program: &str, subcommand: &str) -> bool {
    let program_name = program.rsplit('/').next().unwrap_or(program);
    matches!(
        program_name,
        "apt" | "apt-get" | "dnf" | "yum" | "apk" | "zypper"
    ) && matches!(subcommand, "install" | "add")
}

fn is_mold_package_token(token: &&str) -> bool {
    let package_name = clean_token(token);
    package_name == "mold" || package_name.starts_with("mold=")
}

fn is_rust_suite_step(step: &Value) -> bool {
    step.get("uses")
        .and_then(Value::as_str)
        .is_some_and(|uses| uses.starts_with(GENERATE_COVERAGE))
        || step
            .get("run")
            .and_then(Value::as_str)
            .is_some_and(run_invokes_rust_suite)
}

fn run_invokes_rust_suite(run: &str) -> bool {
    let tokens = run.split_whitespace().collect::<Vec<_>>();
    tokens.iter().enumerate().any(|(index, token)| {
        let command = clean_token(token).rsplit('/').next().unwrap_or(*token);
        if command == "whitaker" {
            return true;
        }
        if command == "make"
            && tokens
                .iter()
                .skip(index.saturating_add(1))
                .take_while(|next| !is_shell_boundary(next))
                .any(|next| matches!(clean_token(next), "test" | "lint"))
        {
            return true;
        }
        command == "cargo" && cargo_suite_subcommand(&tokens, index)
    })
}

fn cargo_suite_subcommand(tokens: &[&str], command_index: usize) -> bool {
    tokens
        .iter()
        .skip(command_index.saturating_add(1))
        .map(|argument| clean_token(argument))
        .find(|argument| !argument.starts_with('+') && !argument.starts_with('-'))
        .is_some_and(|subcommand| matches!(subcommand, "test" | "nextest" | "llvm-cov"))
}

fn is_shell_boundary(token: &&str) -> bool { matches!(*token, "&&" | "||" | ";" | "|") }

fn clean_token(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(character, '\'' | '"' | '(' | ')' | ';' | '&' | '|')
    })
}

fn classify_job_runner(job: &Value) -> RunnerPlatform {
    let Some(runner) = job.get("runs-on") else {
        return RunnerPlatform::Unknown;
    };
    if let Some(expression) = runner.as_str().and_then(matrix_expression_key) {
        return classify_matrix_runner(job, expression);
    }
    classify_runner_value(runner)
}

fn matrix_expression_key(expression: &str) -> Option<&str> {
    let trimmed_expression = expression.trim();
    let matrix_expression = trimmed_expression
        .strip_prefix("${{")?
        .strip_suffix("}}")?
        .trim();
    matrix_expression.strip_prefix("matrix.")
}

fn classify_matrix_runner(job: &Value, key: &str) -> RunnerPlatform {
    let Some(includes) = job
        .get("strategy")
        .and_then(|strategy| strategy.get("matrix"))
        .and_then(|matrix| matrix.get("include"))
        .and_then(Value::as_sequence)
    else {
        return RunnerPlatform::Unknown;
    };
    if includes.is_empty() {
        return RunnerPlatform::Unknown;
    }

    let platforms = includes
        .iter()
        .map(|include| {
            include
                .get(key)
                .map_or(RunnerPlatform::Unknown, classify_runner_value)
        })
        .collect::<Vec<_>>();
    if platforms.contains(&RunnerPlatform::Unknown) {
        RunnerPlatform::Unknown
    } else if platforms.contains(&RunnerPlatform::Linux) {
        RunnerPlatform::Linux
    } else {
        RunnerPlatform::Other
    }
}

fn classify_runner_value(runner: &Value) -> RunnerPlatform {
    match runner {
        Value::String(label) => classify_label(label),
        Value::Sequence(labels) => combine_platforms(labels.iter().map(|label| {
            label
                .as_str()
                .map_or(RunnerPlatform::Unknown, classify_label)
        })),
        Value::Mapping(mapping) => combine_platforms(
            [mapping.get("labels"), mapping.get("group")]
                .into_iter()
                .flatten()
                .map(classify_runner_value),
        ),
        _ => RunnerPlatform::Unknown,
    }
}

fn combine_platforms(platforms: impl Iterator<Item = RunnerPlatform>) -> RunnerPlatform {
    match platforms.fold((false, false), |(linux, other), platform| {
        (
            linux || platform == RunnerPlatform::Linux,
            other || platform == RunnerPlatform::Other,
        )
    }) {
        (true, false) => RunnerPlatform::Linux,
        (false, true) => RunnerPlatform::Other,
        _ => RunnerPlatform::Unknown,
    }
}

fn classify_label(label: &str) -> RunnerPlatform {
    let normalized_label = label.to_ascii_lowercase();
    if normalized_label.starts_with("linux") || normalized_label.starts_with("ubuntu") {
        RunnerPlatform::Linux
    } else if normalized_label.starts_with("windows") || normalized_label.starts_with("macos") {
        RunnerPlatform::Other
    } else {
        RunnerPlatform::Unknown
    }
}
