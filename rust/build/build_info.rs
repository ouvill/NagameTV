//! Collect build inputs without depending on Qt or application initialization.
use std::{
    error::Error,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;
// Keep timestamps representable as UTC dates in the QML diagnostics display.
const LAST_UNIX_SECOND_YEAR_9999: u64 = 253_402_300_799;

enum Source {
    Git { commit: String, state: State },
    Unavailable,
}

enum State {
    Clean,
    Dirty,
}

impl Source {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        if value == "unavailable" {
            return Ok(Self::Unavailable);
        }
        let (commit, state) = value.split_once(':').ok_or("Expected COMMIT:clean|dirty")?;
        if !matches!(commit.len(), SHA1_HEX_LENGTH | SHA256_HEX_LENGTH)
            || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Build commit must be a full Git object ID".into());
        }
        let state = match state {
            "clean" => State::Clean,
            "dirty" => State::Dirty,
            _ => return Err("Build worktree state must be clean or dirty".into()),
        };
        Ok(Self::Git {
            commit: commit.into(),
            state,
        })
    }

    fn rust_literal(&self) -> String {
        match self {
            Self::Unavailable => "Source::Unavailable".into(),
            Self::Git { commit, state } => {
                let state = match state {
                    State::Clean => "Clean",
                    State::Dirty => "Dirty",
                };
                format!(
                    "Source::Git {{ commit: {commit:?}, worktree: viewer_diagnostics::build_info::WorktreeState::{state} }}"
                )
            }
        }
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn source(root: &Path) -> Result<Source, Box<dyn Error>> {
    match std::env::var("NAGAMETV_BUILD_SOURCE") {
        Ok(value) => return Source::parse(&value),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("NAGAMETV_BUILD_SOURCE is not UTF-8".into());
        }
        Err(std::env::VarError::NotPresent) => {}
    }
    // Do not accidentally identify a source archive using an enclosing repository.
    // Linked worktrees have a .git *file*, so never assume it is a directory.
    if !root.join(".git").exists() {
        println!(
            "cargo:warning=Git build identity unavailable; set NAGAMETV_BUILD_SOURCE for source archives"
        );
        return Ok(Source::Unavailable);
    }
    let commit = git(root, &["rev-parse", "--verify", "HEAD"])?;
    let status = git(
        root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ],
    )?;
    Source::parse(&format!(
        "{commit}:{}",
        if status.is_empty() { "clean" } else { "dirty" }
    ))
}

fn timestamp(value: Option<&str>) -> Result<u64, Box<dyn Error>> {
    let seconds = match value {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| "SOURCE_DATE_EPOCH must be nonnegative Unix seconds")?,
        None => SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    };
    if seconds > LAST_UNIX_SECOND_YEAR_9999 {
        return Err("SOURCE_DATE_EPOCH exceeds year 9999".into());
    }
    Ok(seconds)
}

pub fn generate() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=NAGAMETV_BUILD_SOURCE");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Missing OUT_DIR")?);
    // Git status can change without a tracked file's mtime changing (checkout,
    // untracked deletion, submodules, worktrees). Recollect on every Cargo build.
    // This deliberately absent file avoids recursive watching of build outputs.
    println!(
        "cargo:rerun-if-changed={}",
        output.join("always-recheck-build-info").display()
    );
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing CARGO_MANIFEST_DIR")?);
    let source = source(manifest.parent().ok_or("Missing source root")?)?.rust_literal();
    let epoch = std::env::var("SOURCE_DATE_EPOCH");
    let built = timestamp(match &epoch {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("SOURCE_DATE_EPOCH is not UTF-8".into());
        }
    })?;
    let rustc = Command::new(std::env::var_os("RUSTC").ok_or("Missing RUSTC")?)
        .arg("--version")
        .output()?;
    if !rustc.status.success() {
        return Err("rustc --version failed".into());
    }
    let rustc = String::from_utf8(rustc.stdout)?;
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    let text = format!(
        "pub static INFO: viewer_diagnostics::build_info::BuildInfo = {{\n\
         use viewer_diagnostics::build_info::{{BuildInfo, Source}};\n\
         BuildInfo {{ version: {version:?}, source: {source}, built_unix_seconds: {built},\n\
         target: {target:?}, profile: {profile:?}, rustc: {rustc:?}, features: &{features:?} }}\n}};\n",
        version = std::env::var("CARGO_PKG_VERSION")?,
        target = std::env::var("TARGET")?,
        profile = std::env::var("PROFILE")?,
        rustc = rustc.trim(),
    );
    std::fs::write(output.join("build_info.rs"), text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_overrides_reject_partial_or_malformed_identity() {
        let commit = "a".repeat(SHA1_HEX_LENGTH);
        assert!(matches!(
            Source::parse(&format!("{commit}:clean")).unwrap(),
            Source::Git {
                state: State::Clean,
                ..
            }
        ));
        assert!(matches!(
            Source::parse(&format!("{commit}:dirty")).unwrap(),
            Source::Git {
                state: State::Dirty,
                ..
            }
        ));
        assert!(matches!(
            Source::parse("unavailable").unwrap(),
            Source::Unavailable
        ));
        for invalid in [
            "",
            "abc:clean",
            "HEAD:dirty",
            &commit,
            &format!("{commit}:unknown"),
            &format!("{commit}:clean\n"),
        ] {
            assert!(Source::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn reproducible_timestamp_rejects_invalid_dates() {
        assert_eq!(timestamp(Some("0")).unwrap(), 0);
        assert_eq!(timestamp(Some("1700000000")).unwrap(), 1_700_000_000);
        for invalid in [
            "",
            "-1",
            "yesterday",
            "253402300800",
            "18446744073709551616",
        ] {
            assert!(timestamp(Some(invalid)).is_err());
        }
    }
}
