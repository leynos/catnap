//! The pinned channel, read from `rust-toolchain.toml`.
//!
//! Dependabot moves the dated nightly, so the contracts take it from the
//! toolchain file instead of restating it. The reader is a plain function over
//! TOML text so its behaviour is tested with inputs of its own.

use std::{fmt, ops::Deref, sync::OnceLock};

/// The repository's toolchain file: the single source of the pinned channel.
const TOOLCHAIN_FILE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));

/// Returns the `toolchain.channel` of a `rust-toolchain.toml` document.
fn channel_of(document: &str) -> Result<String, String> {
    let parsed: toml::Table = document
        .parse()
        .map_err(|error| format!("rust-toolchain.toml must parse as TOML: {error}"))?;
    parsed
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "rust-toolchain.toml must name a toolchain channel".to_owned())
}

/// The pinned channel, read from the repository's file on first use.
pub struct PinnedToolchain;

impl PinnedToolchain {
    /// The channel; a malformed toolchain file fails the calling test here.
    fn channel() -> &'static str {
        static CHANNEL: OnceLock<String> = OnceLock::new();
        CHANNEL
            .get_or_init(|| match channel_of(TOOLCHAIN_FILE) {
                Ok(channel) => channel,
                Err(message) => panic!("{message}"),
            })
            .as_str()
    }
}

impl Deref for PinnedToolchain {
    type Target = str;

    fn deref(&self) -> &str { Self::channel() }
}

impl fmt::Display for PinnedToolchain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(Self::channel())
    }
}

/// Stands for the pinned channel wherever the tests name it.
pub const PINNED_TOOLCHAIN: PinnedToolchain = PinnedToolchain;

#[cfg(test)]
mod tests {
    //! Reader behaviour over inputs of its own.

    use super::{PINNED_TOOLCHAIN, channel_of};

    #[test]
    fn the_reader_follows_its_input() {
        let read = |channel: &str| channel_of(&format!("[toolchain]\nchannel = \"{channel}\"\n"));
        assert_eq!(
            read("nightly-1999-01-01").as_deref(),
            Ok("nightly-1999-01-01")
        );
        assert_eq!(read("stable").as_deref(), Ok("stable"));
    }

    #[test]
    fn the_reader_rejects_invalid_toml_and_a_missing_channel() {
        assert!(
            channel_of("[toolchain").is_err(),
            "invalid TOML must be rejected"
        );
        assert!(
            channel_of("[toolchain]\ncomponents = []\n").is_err(),
            "a missing channel must be rejected"
        );
    }

    #[test]
    fn the_pinned_channel_is_the_files_channel() {
        let from_file = channel_of(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/rust-toolchain.toml"
        )));
        assert_eq!(from_file.as_deref(), Ok(&*PINNED_TOOLCHAIN));
    }
}
