use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::debug;

/// The URL of the `origin` remote of the Git repository containing `dir`, if any.
#[must_use]
pub fn origin_url(dir: &Path) -> Option<String> {
    let url = run_git(dir, &["remote", "get-url", "origin"])?;
    (!url.is_empty()).then_some(url)
}

/// The top-level directory of the Git repository containing `dir`, if any.
#[must_use]
pub fn toplevel(dir: &Path) -> Option<PathBuf> {
    run_git(dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output();
    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            debug!(dir = %dir.display(), ?args, %stdout, "git succeeded");
            Some(stdout)
        }
        Ok(output) => {
            debug!(
                dir = %dir.display(),
                ?args,
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "git failed"
            );
            None
        }
        Err(error) => {
            debug!(dir = %dir.display(), ?args, %error, "git could not be run");
            None
        }
    }
}
