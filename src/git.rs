use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::debug;

/// The URL is returned as Git holds it, credentials included. Any Git failure
/// is `None`.
#[must_use]
pub fn origin_url(dir: &Path) -> Option<String> {
    let url = run_git(dir, &["remote", "get-url", "origin"])?;
    (!url.is_empty()).then_some(url)
}

/// Any Git failure is `None`, a missing `git` binary included.
#[must_use]
pub fn toplevel(dir: &Path) -> Option<PathBuf> {
    run_git(dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output();
    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            // Never log `stdout` itself: a remote URL may carry a token.
            debug!(dir = %dir.display(), ?args, stdout_len = stdout.len(), "git succeeded");
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
