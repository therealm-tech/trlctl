use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use tracing::{debug, info};

use crate::git;
use crate::names::is_valid_name;
use crate::urls::{is_git_url, to_https};

/// Which roles of the layout the initialised directory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Both the repo catalogue and the repo platform.
    Full,
    /// The repo catalogue only: `apps/`.
    CatalogOnly,
    /// The repo platform only: `envs/` and `clusters/`.
    PlatformOnly,
}

/// What `trlctl init` creates, as given on the command line.
#[derive(Debug, Clone)]
pub struct Options {
    pub dir: PathBuf,
    pub cluster: Option<String>,
    pub catalog_url: Option<String>,
    pub platform_url: Option<String>,
    pub revision: String,
    pub scope: Scope,
    pub force: bool,
}

/// Asks the user for a value the command line did not give.
pub trait Prompt {
    /// The answer to `question`.
    ///
    /// # Errors
    ///
    /// When no answer can be obtained, e.g. stdin is not a terminal.
    fn ask(&mut self, question: &str, missing_flag: &str) -> Result<String>;
}

/// Scaffolds the layout described by `options` and returns the files written.
///
/// # Errors
///
/// When a value is missing or invalid, when a file exists and `force` is
/// not set, or when the directory cannot be written.
pub fn run(options: &Options, prompt: &mut impl Prompt) -> Result<Vec<PathBuf>> {
    let origin = git::origin_url(&options.dir).and_then(|url| to_https(&url));
    debug!(dir = %options.dir.display(), ?origin, scope = ?options.scope, "initialising");
    let cluster_file = if options.scope == Scope::CatalogOnly {
        None
    } else {
        let cluster = match &options.cluster {
            Some(cluster) => cluster.clone(),
            None => prompt.ask("Cluster name", "--cluster")?,
        };
        ensure!(
            is_valid_name(&cluster),
            "`{cluster}` is not a valid cluster name: lowercase letters, digits and `-`, \
             starting and ending with a letter or a digit, at most 63 characters"
        );
        let (catalog_url, platform_url) = repository_urls(options, origin, prompt)?;
        for (flag, url) in [
            ("--catalog-url", &catalog_url),
            ("--platform-url", &platform_url),
        ] {
            ensure!(
                is_git_url(url),
                "{flag} `{url}` is not a Git repository URL"
            );
        }
        let path = options
            .dir
            .join("clusters")
            .join(&cluster)
            .join("cluster.yaml");
        if path.exists() && !options.force {
            bail!(
                "{} already exists; pass --force to overwrite it",
                path.display()
            );
        }
        Some((
            path,
            cluster_yaml(&catalog_url, &platform_url, &options.revision)?,
        ))
    };
    let mut files = Vec::new();
    if options.scope != Scope::PlatformOnly {
        files.push(keep_file(&options.dir.join("apps"))?);
    }
    if let Some((path, content)) = cluster_file {
        files.push(keep_file(&options.dir.join("envs"))?);
        files.push(Some(write(&path, &content)?));
    }
    let written: Vec<PathBuf> = files.into_iter().flatten().collect();
    info!(count = written.len(), "layout initialised");
    Ok(written)
}

fn repository_urls(
    options: &Options,
    origin: Option<String>,
    prompt: &mut impl Prompt,
) -> Result<(String, String)> {
    let mut ask = |question, flag| prompt.ask(question, flag);
    if options.scope == Scope::PlatformOnly {
        let platform = match options.platform_url.clone().or(origin) {
            Some(url) => url,
            None => ask("Platform repository URL", "--platform-url")?,
        };
        let catalog = match &options.catalog_url {
            Some(url) => url.clone(),
            None => ask("Catalogue repository URL", "--catalog-url")?,
        };
        return Ok((catalog, platform));
    }
    let catalog = match options.catalog_url.clone().or(origin) {
        Some(url) => url,
        None => ask("Catalogue repository URL", "--catalog-url")?,
    };
    let platform = options
        .platform_url
        .clone()
        .unwrap_or_else(|| catalog.clone());
    Ok((catalog, platform))
}

fn cluster_yaml(catalog_url: &str, platform_url: &str, revision: &str) -> Result<String> {
    ensure!(!revision.is_empty(), "--revision must not be empty");
    let quote = |value: &str| serde_json::to_string(value).context("quoting a YAML value");
    Ok(format!(
        "repo:\n  catalog:\n    url: {}\n    revision: {}\n  platform:\n    url: {}\n    revision: {}\n",
        quote(catalog_url)?,
        quote(revision)?,
        quote(platform_url)?,
        quote(revision)?,
    ))
}

/// Creates `dir` with a `.gitkeep` so Git tracks it while it is empty. A
/// directory that already holds anything is left as it is.
fn keep_file(dir: &Path) -> Result<Option<PathBuf>> {
    if dir.is_dir() && fs::read_dir(dir)?.next().is_some() {
        debug!(dir = %dir.display(), "directory not empty: no .gitkeep");
        return Ok(None);
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let keep = dir.join(".gitkeep");
    fs::write(&keep, "").with_context(|| format!("writing {}", keep.display()))?;
    Ok(Some(keep))
}

fn write(path: &Path, content: &str) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    debug!(path = %path.display(), "written");
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::cluster_yaml;

    #[test]
    fn quotes_every_value() {
        let yaml = cluster_yaml("https://h/c.git", "git@h:p.git", "1.10").unwrap();
        assert_eq!(
            yaml,
            "repo:\n  catalog:\n    url: \"https://h/c.git\"\n    revision: \"1.10\"\n  \
             platform:\n    url: \"git@h:p.git\"\n    revision: \"1.10\"\n"
        );
    }
}
