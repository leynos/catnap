//! Readers for the Netsukefile half of the build standard: public actions are
//! executed with fake tools so their actual `RUSTFLAGS` reach the contract.

use std::{env, process::Command};

use cap_std::{
    ambient_authority,
    fs::{Dir, PermissionsExt},
};
use tempfile::TempDir;

use super::config::{CODEGEN_BACKEND_FLAG, Flags, LINKER_FLAG, Pin, Problems, THREADS_FLAG};

/// Netsuke actions that build for development. A command in one either assigns
/// `RUSTFLAGS` with the standard flags or assigns none and so takes the
/// configuration's. The list is this repository's own, and a target that stops
/// being defined fails the contract rather than dropping out of it.
const DEVELOPMENT_TARGETS: &[&str] = &["test", "typecheck", "lint", "build"];
/// Netsuke actions that measure or ship, so every command assigns `RUSTFLAGS`
/// and none carries a standard flag.
const HELD_OUT_TARGETS: &[&str] = &["coverage", "release"];

/// The host Netsuke is told it runs on, through `BUILD_HOST_OS`.
#[derive(Clone, Copy)]
pub enum Host {
    Linux,
    Darwin,
}

impl Host {
    /// Returns the value `uname -s` reports for the host.
    const fn host_name(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Darwin => "Darwin",
        }
    }

    /// Returns whether the host takes mold, which ships for Linux alone.
    const fn takes_linker_flag(self) -> bool { matches!(self, Self::Linux) }
}

/// What one Cargo command assigns to `RUSTFLAGS`.
#[derive(Debug, PartialEq, Eq)]
pub enum Assignment {
    Unassigned,
    /// An assignment, and whether it keeps the caller's own `RUSTFLAGS`.
    Flags(Flags, bool),
}

/// Reads the `RUSTFLAGS` a command line assigns. An unreadable form is
/// an error, because it still replaces the configuration's sources and so must
/// not pass.
///
/// ```text
/// assigned_rustflags("RUSTFLAGS=\"-Zthreads=8\" cargo test") -> Flags(["-Zthreads=8"], inherits: false)
/// assigned_rustflags("RUSTFLAGS=\"${RUSTFLAGS:+$RUSTFLAGS }-Zthreads=8\" cargo test") -> inherits: true
/// assigned_rustflags("cargo test")                           -> Unassigned
/// assigned_rustflags("RUSTFLAGS=-Zthreads=8 cargo test")     -> Err
/// ```
///
/// # Errors
///
/// Returns the reason when an assignment is unquoted or unterminated.
pub fn assigned_rustflags(line: &str) -> Result<Assignment, String> {
    let Some((_, rest)) = line.split_once("RUSTFLAGS=\"") else {
        if line.contains("RUSTFLAGS=") {
            return Err(format!("unreadable RUSTFLAGS assignment in `{line}`"));
        }
        return Ok(Assignment::Unassigned);
    };
    let (assigned, _) = rest
        .split_once('"')
        .ok_or_else(|| format!("unterminated RUSTFLAGS in `{line}`"))?;
    // The recipes prepend the caller's own flags with these expansions; they are
    // not standard flags, and glued to the next word they would hide it.
    let inherits =
        assigned.contains("${RUSTFLAGS:+$RUSTFLAGS }") || assigned.contains("${RUSTFLAGS-}");
    let own = assigned
        .replace("${RUSTFLAGS:+$RUSTFLAGS }", " ")
        .replace("${RUSTFLAGS-}", "");
    Ok(Assignment::Flags(
        Flags::from_words(own.split_whitespace()),
        inherits,
    ))
}

/// Reads the assignment of each Cargo command in a plan. Whitaker's
/// separate toolchain must not inherit the development build flags.
///
/// # Errors
///
/// Returns the reason when a command assigns `RUSTFLAGS` in an unreadable form.
pub fn commands_from(stdout: &str) -> Result<Vec<Assignment>, String> {
    // A recipe continued with a trailing backslash is one command.
    let joined = stdout.replace("\\\n", " ");
    joined
        .lines()
        .filter(|line| !line.trim_start().starts_with("echo"))
        .filter(|line| {
            line.split_whitespace()
                .any(|word| word.rsplit('/').next() == Some("cargo"))
        })
        .map(assigned_rustflags)
        .collect()
}

/// Install one executable into a private command directory.
fn fake_tool(directory: &Dir, name: &str, script: &str) -> Result<(), String> {
    directory
        .write(name, script)
        .map_err(|error| error.to_string())?;
    let mut permissions = directory
        .metadata(name)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    directory
        .set_permissions(name, permissions)
        .map_err(|error| error.to_string())
}

/// Execute a public action with fake Cargo and read the flags it actually saw.
fn netsuke_commands(target: &str, host: Host) -> Result<Vec<Assignment>, String> {
    let workspace = TempDir::new().map_err(|error| error.to_string())?;
    let tools = TempDir::new().map_err(|error| error.to_string())?;
    let root = Dir::open_ambient_dir(workspace.path(), ambient_authority())
        .map_err(|error| error.to_string())?;
    let tool_root = Dir::open_ambient_dir(tools.path(), ambient_authority())
        .map_err(|error| error.to_string())?;
    root.write("Netsukefile", include_str!("../../Netsukefile"))
        .map_err(|error| error.to_string())?;
    root.create_dir("scripts")
        .map_err(|error| error.to_string())?;
    tool_root
        .create_dir("bin")
        .map_err(|error| error.to_string())?;
    fake_tool(&root, "scripts/check-build-tools.sh", "#!/bin/sh\nexit 0\n")?;
    fake_tool(
        &tool_root,
        "bin/cargo",
        "#!/bin/sh\nprintf '%s\\n' \"${RUSTFLAGS-__UNSET__}\" >> \"$BUILD_STANDARD_LOG\"\n",
    )?;
    for tool in ["whitaker", "yamllint", "actionlint"] {
        fake_tool(&tool_root, &format!("bin/{tool}"), "#!/bin/sh\nexit 0\n")?;
    }
    let fake_bin = tools.path().join("bin");
    let mut path_entries = vec![fake_bin];
    path_entries.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let sandbox_path = env::join_paths(path_entries).map_err(|error| error.to_string())?;
    let output = Command::new("netsuke")
        .args(["build", target])
        .current_dir(workspace.path())
        .env("PATH", sandbox_path)
        .env("BUILD_TOOLS_PREFIX", tools.path())
        .env("BUILD_HOST_OS", host.host_name())
        .env(
            "BUILD_STANDARD_LOG",
            workspace.path().join("cargo-flags.log"),
        )
        .env("RUSTFLAGS", "-Copt-level=1")
        .env_remove("BASH_ENV")
        .output()
        .map_err(|error| format!("running Netsuke {target}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Netsuke {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let flags = root
        .read_to_string("cargo-flags.log")
        .map_err(|error| format!("Netsuke {target} ran no Cargo command: {error}"))?;
    Ok(flags
        .lines()
        .map(|value| {
            if matches!(value, "__UNSET__" | "-Copt-level=1") {
                Assignment::Unassigned
            } else {
                Assignment::Flags(
                    Flags::from_words(value.split_whitespace()),
                    value.contains("-Copt-level=1"),
                )
            }
        })
        .collect())
}

/// Returns the complaint about one development command, if any: an assigned
/// `RUSTFLAGS` keeps the caller's own flags and restates the frontend flag on a
/// nightly pin, and mold on Linux.
fn development_problem(
    target: &str,
    host: Host,
    pin: Pin,
    assignment: &Assignment,
) -> Option<String> {
    let Assignment::Flags(flags, inherits) = assignment else {
        return Some(format!(
            "`netsuke build {target}` on {} has a Cargo command with unassigned RUSTFLAGS",
            host.host_name()
        ));
    };
    if !inherits {
        return Some(format!(
            "`netsuke build {target}` on {} drops the caller's RUSTFLAGS",
            host.host_name()
        ));
    }
    let reason = flags.meets(pin, host.takes_linker_flag()).err()?;
    Some(format!(
        "`netsuke build {target}` on {} {reason}",
        host.host_name()
    ))
}

/// Returns every complaint about the development targets on one host, and how
/// many assignments it read, so a test can refuse to pass over nothing.
///
/// # Errors
///
/// Returns the reason when a listed target is not defined or unreadable.
pub fn development_problems(host: Host, pin: Pin) -> Result<(Problems, usize), String> {
    let mut problems = Vec::new();
    let mut read = 0;
    for target in DEVELOPMENT_TARGETS {
        let commands = netsuke_commands(target, host)?;
        if commands.is_empty() {
            problems.push(format!(
                "`netsuke build {target}` on {} runs no Cargo command",
                host.host_name()
            ));
        }
        read += commands
            .iter()
            .filter(|command| **command != Assignment::Unassigned)
            .count();
        problems.extend(
            commands
                .iter()
                .filter_map(|command| development_problem(target, host, pin, command)),
        );
    }
    Ok((problems, read))
}

/// Returns every complaint about one held-out command: it assigns nothing, so
/// it takes the configuration's flags, or the assignment names a standard flag.
fn held_out_command_problems(target: &str, assignment: &Assignment) -> Problems {
    let Assignment::Flags(flags, _) = assignment else {
        return vec![format!(
            "`netsuke build {target}` runs a command that takes the configuration's flags"
        )];
    };
    let named = [
        (flags.names_threads(), THREADS_FLAG),
        (flags.names_cranelift(), CODEGEN_BACKEND_FLAG),
        (flags.names_linker(), LINKER_FLAG),
    ];
    named
        .into_iter()
        .filter(|(is_named, _)| *is_named)
        .map(|(_, flag)| format!("`netsuke build {target}` takes {flag}"))
        .collect()
}

/// Returns every complaint about the held-out targets, and how many commands it
/// read: each assigns `RUSTFLAGS`, since only an assignment displaces the
/// configuration's sources.
///
/// # Errors
///
/// Returns the reason when a listed target is not defined or unreadable.
pub fn held_out_problems() -> Result<(Problems, usize), String> {
    let mut problems = Vec::new();
    let mut read = 0;
    for target in HELD_OUT_TARGETS {
        let commands = netsuke_commands(target, Host::Linux)?;
        read += commands.len();
        problems.extend(
            commands
                .iter()
                .flat_map(|command| held_out_command_problems(target, command)),
        );
    }
    Ok((problems, read))
}

/// Returns the number of held-out targets the repository defines.
pub const fn held_out_target_count() -> usize { HELD_OUT_TARGETS.len() }

#[cfg(test)]
mod mutation_contract {
    //! A single unassigned Cargo command must fail the development contract.

    use super::{Assignment, Host, Pin, development_problem};
    use crate::config::{CODEGEN_BACKEND_FLAG, Flags, LINKER_FLAG, THREADS_FLAG};

    #[test]
    fn removing_one_development_command_assignment_is_reported() {
        let commands = [
            Assignment::Flags(
                Flags::from_words([THREADS_FLAG, CODEGEN_BACKEND_FLAG, LINKER_FLAG]),
                true,
            ),
            Assignment::Unassigned,
        ];
        let problems = commands
            .iter()
            .filter_map(|command| development_problem("test", Host::Linux, Pin::Nightly, command))
            .collect::<Vec<_>>();

        assert_eq!(problems.len(), 1, "mutation problems: {problems:?}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("unassigned RUSTFLAGS")),
            "mutation problems: {problems:?}"
        );
    }
}
