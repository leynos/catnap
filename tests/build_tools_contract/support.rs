//! Provides an isolated executable and filesystem sandbox for build-tool contracts.

use std::{
    env,
    error::Error,
    ffi::OsString,
    path::PathBuf,
    process::{Command, Output},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, PermissionsExt},
};
use rstest::fixture;
use tempfile::TempDir;

pub(super) struct BuildToolsSandbox {
    directory: Dir,
    temporary_directory: TempDir,
    pub(super) prefix_bin: PathBuf,
    pub(super) cargo: PathBuf,
    cargo_log: PathBuf,
    rustup_log: PathBuf,
    install_log: PathBuf,
    path: OsString,
}

#[fixture]
pub(super) fn build_tools_sandbox() -> Result<BuildToolsSandbox, Box<dyn Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
    directory.create_dir_all("prefix/bin")?;
    directory.create_dir_all("incoming-bin")?;

    let mut paths = vec![temporary_directory.path().join("incoming-bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths)?;
    let cargo = temporary_directory.path().join("fake-cargo");
    let prefix_bin = temporary_directory.path().join("prefix/bin");
    let cargo_log = temporary_directory.path().join("cargo-invocations.log");
    let rustup_log = temporary_directory.path().join("rustup-invocations.log");
    let install_log = temporary_directory.path().join("install-order.log");
    directory.write("cargo-invocations.log", "")?;
    directory.write("rustup-invocations.log", "")?;
    directory.write("install-order.log", "")?;

    let sandbox = BuildToolsSandbox {
        directory,
        temporary_directory,
        prefix_bin,
        cargo,
        cargo_log,
        rustup_log,
        install_log,
        path,
    };
    sandbox.write_fake_cargo()?;
    Ok(sandbox)
}

impl BuildToolsSandbox {
    pub(super) fn write_mold(
        &self,
        relative_path: &str,
        version: &str,
    ) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            relative_path,
            &format!("#!/bin/sh\nprintf '%s\\n' 'mold {version}'\n"),
        )
    }

    pub(super) fn write_clang(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "incoming-bin/clang",
            "#!/bin/sh\nprintf 'clang version test\\n'\n",
        )
    }

    pub(super) fn write_broken_clang(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable("incoming-bin/clang", "#!/bin/sh\nexit 1\n")
    }

    pub(super) fn write_lld(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable("incoming-bin/ld.lld", "#!/bin/sh\nexit 1\n")
    }

    pub(super) fn write_usable_lld(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable("incoming-bin/ld.lld", "#!/bin/sh\nprintf 'LLD test\\n'\n")
    }

    pub(super) fn write_rustup(&self, toolchains: &str) -> Result<(), Box<dyn Error>> {
        self.write_rustup_with_components(
            toolchains,
            concat!(
                "clippy-x86_64-unknown-linux-gnu\n",
                "llvm-tools-x86_64-unknown-linux-gnu\n",
                "rustfmt-x86_64-unknown-linux-gnu\n"
            ),
        )
    }

    pub(super) fn write_rustup_with_components(
        &self,
        toolchains: &str,
        components: &str,
    ) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "incoming-bin/rustup",
            &format!(
                concat!(
                    "#!/bin/sh\n",
                    "if [ \"${{1:-}}\" = toolchain ] && [ \"${{2:-}}\" = list ]; then\n",
                    "  printf '%s\\n' '{toolchains}'\n",
                    "elif [ \"${{1:-}}\" = component ] && [ \"${{2:-}}\" = list ]; then\n",
                    "  printf '%s\\n' '{components}'\n",
                    "else\n  exit 2\nfi\n"
                ),
                toolchains = toolchains,
                components = components
            ),
        )
    }

    pub(super) fn write_uname(&self, operating_system: &str) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "incoming-bin/uname",
            &format!("#!/bin/sh\nprintf '%s\\n' '{operating_system}'\n"),
        )
    }

    pub(super) fn write_recording_rustup(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "incoming-bin/rustup",
            concat!(
                "#!/bin/sh\n",
                "printf 'rustup\\n' >> \"$BUILD_TOOLS_INSTALL_LOG\"\n",
                "printf '%s\\n' \"$@\" > \"$BUILD_TOOLS_RUSTUP_LOG\"\n"
            ),
        )
    }

    pub(super) fn write_fake_mold_download_tools(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "incoming-bin/curl",
            concat!(
                "#!/bin/sh\n",
                "output=\n",
                "while [ \"$#\" -gt 0 ]; do\n",
                "  if [ \"$1\" = --output ]; then output=$2; shift 2; else shift; fi\n",
                "done\n",
                "printf 'fixture archive\\n' > \"$output\"\n",
                "printf 'curl\\n' >> \"$BUILD_TOOLS_INSTALL_LOG\"\n"
            ),
        )?;
        self.write_executable(
            "incoming-bin/tar",
            concat!(
                "#!/bin/sh\n",
                "printf 'tar\\n' >> \"$BUILD_TOOLS_INSTALL_LOG\"\n"
            ),
        )
    }

    fn write_fake_cargo(&self) -> Result<(), Box<dyn Error>> {
        self.write_executable(
            "fake-cargo",
            concat!(
                "#!/bin/sh\n",
                "if [ \"${1:-}\" = nextest ]; then exit 1; fi\n",
                "printf '%s\\n' \"$*\" >> \"$BUILD_TOOLS_CARGO_LOG\"\n",
            ),
        )
    }

    fn write_executable(&self, path: &str, contents: &str) -> Result<(), Box<dyn Error>> {
        self.directory.write(path, contents)?;
        let mut permissions = self.directory.metadata(path)?.permissions();
        permissions.set_mode(0o755);
        self.directory.set_permissions(path, permissions)?;
        Ok(())
    }

    pub(super) fn run_make(&self, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
        Ok(self.make_command(arguments).output()?)
    }

    pub(super) fn run_installer_with_checksum(
        &self,
        checksum: &str,
    ) -> Result<Output, Box<dyn Error>> {
        let checksum_file = self.temporary_directory.path().join("mold-checksums.txt");
        self.directory.write(
            "mold-checksums.txt",
            format!("{checksum}  mold-2.41.0-x86_64-linux.tar.gz\n"),
        )?;
        let mut command = self.make_command(&["install-build-tools"]);
        command.env("MOLD_SHA256SUMS_FILE", checksum_file);
        Ok(command.output()?)
    }

    fn make_command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new("make");
        command
            .current_dir(repository_root())
            .arg(format!("CARGO={}", self.cargo.display()))
            .args(arguments)
            .env(
                "BUILD_TOOLS_PREFIX",
                self.temporary_directory.path().join("prefix"),
            )
            .env("BUILD_TOOLS_CARGO_LOG", &self.cargo_log)
            .env("BUILD_TOOLS_RUSTUP_LOG", &self.rustup_log)
            .env("BUILD_TOOLS_INSTALL_LOG", &self.install_log)
            .env("PATH", &self.path)
            .env_remove("GITHUB_ACTIONS");
        command
    }

    pub(super) fn run_checker(&self, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
        let checker = repository_root().join("scripts/check-build-tools.sh");
        Ok(Command::new(checker)
            .args(arguments)
            .env(
                "BUILD_TOOLS_PREFIX",
                self.temporary_directory.path().join("prefix"),
            )
            .env("PATH", &self.path)
            .output()?)
    }

    pub(super) fn cargo_invocations(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.directory.read_to_string("cargo-invocations.log")?)
    }

    pub(super) fn rustup_invocations(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.directory.read_to_string("rustup-invocations.log")?)
    }

    pub(super) fn install_order(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.directory.read_to_string("install-order.log")?)
    }
}

fn repository_root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }
