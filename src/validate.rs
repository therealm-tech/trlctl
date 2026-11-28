mod config;
mod source;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};
use tracing::debug;

use crate::git;
use crate::names::is_valid_name;
use crate::urls::{
    is_chart_repository_url, is_git_url, same_repository, to_https, without_credentials,
};
use config::{Loaded, STRING_FIELDS, app_mapping, cluster_repos, is_set, load, merge};
use source::{KUSTOMIZATION_FILES, check_catalogue_source};

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The layout breaks the specification.
    Error,
    /// The layout follows the specification, but something about it is
    /// likely wrong or will fail once Argo CD renders it.
    Warning,
}

/// One problem `validate` found in a repository.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Finding {
    /// The file or directory at fault, relative to its layout root.
    pub file: String,
    /// Whether the finding breaks the specification or only warns.
    pub severity: Severity,
    /// The section of the specification it relates to, e.g. `§8.2`; `None`
    /// for a warning the specification says nothing about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// What is wrong.
    pub message: String,
}

impl Finding {
    /// Whether the finding breaks the specification.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[derive(Default)]
pub(crate) struct Findings(BTreeSet<Finding>);

impl Findings {
    fn push(&mut self, file: &str, section: &str, message: impl Into<String>) {
        self.insert(file, Severity::Error, Some(section), message.into());
    }

    fn warn(&mut self, file: &str, section: Option<&str>, message: impl Into<String>) {
        self.insert(file, Severity::Warning, section, message.into());
    }

    fn insert(&mut self, file: &str, severity: Severity, section: Option<&str>, message: String) {
        let finding = Finding {
            file: file.to_owned(),
            severity,
            section: section.map(str::to_owned),
            message,
        };
        debug!(file = %finding.file, severity = ?finding.severity, section = ?finding.section, message = %finding.message, "finding");
        self.0.insert(finding);
    }
}

/// Validates the repo catalogue whose layout root is `catalog` and the repo
/// platform whose layout root is `platform` against the layout specification.
///
/// # Errors
///
/// When a directory or file of either layout cannot be read.
pub fn validate(catalog: &Path, platform: &Path) -> Result<Vec<Finding>> {
    let mut out = Findings::default();
    let apps = load_catalogue(catalog, &mut out)?;
    let envs = load_envs(platform, &mut out)?;
    check_clusters(catalog, platform, &apps, &envs, &mut out)?;
    Ok(out.0.into_iter().collect())
}

fn load_catalogue(catalog: &Path, out: &mut Findings) -> Result<BTreeMap<String, Mapping>> {
    let mut apps = BTreeMap::new();
    let apps_dir = catalog.join("apps");
    if !is_dir(&apps_dir)? {
        out.push("apps/", "§4", "the repo catalogue has no `apps/` directory");
        return Ok(apps);
    }
    for (name, dir) in subdirectories(&apps_dir)? {
        let path = dir.join("app.yaml");
        if !is_file(&path)? {
            debug!(dir = %dir.display(), "not an app: no app.yaml");
            continue;
        }
        let file = relative(catalog, &path);
        check_name(&name, &format!("apps/{name}/"), out);
        let app = app_file(&path, &file, out)?;
        let sources = ["chart", "path"].iter().filter(|f| is_set(&app, f)).count();
        if sources != 1 {
            out.push(
                &file,
                "§6.2",
                "the catalogue `app.yaml` must set exactly one of `chart` and `path`",
            );
        }
        apps.insert(name, app);
    }
    debug!(count = apps.len(), "catalogue loaded");
    Ok(apps)
}

fn load_envs(platform: &Path, out: &mut Findings) -> Result<BTreeMap<(String, String), Mapping>> {
    let mut envs = BTreeMap::new();
    for (env, env_dir) in subdirectories(&platform.join("envs"))? {
        check_name(&env, &format!("envs/{env}/"), out);
        for (app, dir) in subdirectories(&env_dir)? {
            check_name(&app, &format!("envs/{env}/{app}/"), out);
            let path = dir.join("app.yaml");
            if !is_file(&path)? {
                debug!(dir = %dir.display(), "no env level: no app.yaml");
                continue;
            }
            let mapping = app_file(&path, &relative(platform, &path), out)?;
            envs.insert((env.clone(), app), mapping);
        }
    }
    Ok(envs)
}

fn check_clusters(
    catalog: &Path,
    platform: &Path,
    apps: &BTreeMap<String, Mapping>,
    envs: &BTreeMap<(String, String), Mapping>,
    out: &mut Findings,
) -> Result<()> {
    let clusters_dir = platform.join("clusters");
    if !is_dir(&clusters_dir)? {
        out.push(
            "clusters/",
            "§4",
            "the repo platform has no `clusters/` directory",
        );
        return Ok(());
    }
    for (cluster, cluster_dir) in subdirectories(&clusters_dir)? {
        check_name(&cluster, &format!("clusters/{cluster}/"), out);
        check_cluster_file(platform, &cluster_dir, out)?;
        check_unread_directories(&cluster, &cluster_dir, apps, out)?;
        let mut cluster_apps = BTreeMap::new();
        for (app, dir) in subdirectories(&cluster_dir.join("apps"))? {
            check_name(&app, &format!("clusters/{cluster}/apps/{app}/"), out);
            let path = dir.join("app.yaml");
            if !is_file(&path)? {
                debug!(dir = %dir.display(), "no cluster×app level: no app.yaml");
                continue;
            }
            cluster_apps.insert(app, app_file(&path, &relative(platform, &path), out)?);
        }
        for (env, env_dir) in subdirectories(&cluster_dir.join("envs"))? {
            let level = Levels {
                catalog,
                platform,
                apps,
                envs,
                cluster_apps: &cluster_apps,
                cluster: &cluster,
                env: &env,
            };
            check_env(&level, &env_dir, out)?;
        }
    }
    Ok(())
}

/// SPEC.md §5: only `apps/` and `envs/` are read under `clusters/<cluster>/`.
/// Any other directory is ignored, but one holding instances or a
/// cluster×app file is almost certainly misplaced, so it gets a warning.
fn check_unread_directories(
    cluster: &str,
    cluster_dir: &Path,
    apps: &BTreeMap<String, Mapping>,
    out: &mut Findings,
) -> Result<()> {
    for (name, dir) in subdirectories(cluster_dir)? {
        if name == "apps" || name == "envs" {
            continue;
        }
        let file = format!("clusters/{cluster}/{name}/");
        if has_instances(&dir)? {
            out.warn(
                &file,
                Some("§5"),
                format!(
                    "not read, so the instances under it are not deployed: \
                     the instances of the env `{name}` live under `clusters/{cluster}/envs/{name}/`"
                ),
            );
        } else if apps.contains_key(&name)
            && (is_file(&dir.join("app.yaml"))? || is_file(&dir.join("values.yaml"))?)
        {
            out.warn(
                &file,
                Some("§5"),
                format!(
                    "not read: the cluster×app level of `{name}` lives under `clusters/{cluster}/apps/{name}/`"
                ),
            );
        }
    }
    Ok(())
}

fn check_cluster_file(platform: &Path, cluster_dir: &Path, out: &mut Findings) -> Result<()> {
    let path = cluster_dir.join("cluster.yaml");
    let file = relative(platform, &path);
    let value = match load(&path)? {
        Loaded::Missing => {
            out.push(
                &file,
                "§7.4",
                "every cluster directory must hold a `cluster.yaml`",
            );
            return Ok(());
        }
        Loaded::Unparsable(error) => {
            out.push(&file, "§9", format!("unparsable YAML: {error}"));
            return Ok(());
        }
        Loaded::Value(value) => value,
    };
    if let Some(repos) = cluster_repos(&value, &file, out) {
        check_platform_repository(
            platform,
            &repos.platform_url,
            &repos.platform_path,
            &file,
            out,
        );
    }
    Ok(())
}

/// SPEC.md §9: `repo.platform` designates the repository and layout root
/// holding the `cluster.yaml`. Checkable only when `platform` is in a Git
/// checkout, and the URL only when that checkout has an `origin` remote. A
/// URL mismatch is only a warning: a fork or a mirror has another `origin`.
fn check_platform_repository(
    platform: &Path,
    url: &str,
    path: &str,
    file: &str,
    out: &mut Findings,
) {
    let Some(toplevel) = git::toplevel(platform) else {
        debug!(platform = %platform.display(), "not a Git checkout: repo.platform not compared");
        return;
    };
    if let Some(origin) = git::origin_url(platform)
        && !same_repository(&origin, url)
    {
        let origin = to_https(&origin).unwrap_or_else(|| without_credentials(&origin));
        let url = without_credentials(url);
        out.warn(
            file,
            Some("§9"),
            format!(
                "`repo.platform.url` `{url}` does not designate the `origin` of this checkout (`{origin}`); \
                 expected when validating a fork or a mirror"
            ),
        );
    }
    let (Ok(toplevel), Ok(root)) = (fs::canonicalize(toplevel), fs::canonicalize(platform)) else {
        return;
    };
    let Ok(layout_root) = root.strip_prefix(&toplevel) else {
        return;
    };
    let layout_root = layout_root.to_string_lossy();
    let declared = path.trim_matches('/');
    if declared != layout_root {
        out.push(
            file,
            "§9",
            format!(
                "`repo.platform.path` `{declared}` does not designate the layout root holding this file (`{layout_root}`)"
            ),
        );
    }
}

struct Levels<'a> {
    catalog: &'a Path,
    platform: &'a Path,
    apps: &'a BTreeMap<String, Mapping>,
    envs: &'a BTreeMap<(String, String), Mapping>,
    cluster_apps: &'a BTreeMap<String, Mapping>,
    cluster: &'a str,
    env: &'a str,
}

fn check_env(level: &Levels, env_dir: &Path, out: &mut Findings) -> Result<()> {
    let (cluster, env) = (level.cluster, level.env);
    for (app, app_dir) in subdirectories(env_dir)? {
        for unread in ["app.yaml", "values.yaml"] {
            if is_file(&app_dir.join(unread))? {
                out.warn(
                    &format!("clusters/{cluster}/envs/{env}/{app}/{unread}"),
                    Some("§7.3"),
                    format!(
                        "not read: no configuration level sits between the env and the instance; \
                         the env level of `{app}` is `envs/{env}/{app}/{unread}`"
                    ),
                );
            }
        }
        for (instance, dir) in subdirectories(&app_dir)? {
            let path = dir.join("app.yaml");
            if !is_file(&path)? {
                debug!(dir = %dir.display(), "not an instance: no app.yaml");
                continue;
            }
            check_name(env, &format!("clusters/{cluster}/envs/{env}/"), out);
            check_name(&app, &format!("clusters/{cluster}/envs/{env}/{app}/"), out);
            check_name(
                &instance,
                &format!("clusters/{cluster}/envs/{env}/{app}/{instance}/"),
                out,
            );
            let file = relative(level.platform, &path);
            let own = app_file(&path, &file, out)?;
            check_instance_manifests(level.platform, &dir.join("manifests"), out)?;
            let Some(catalogue) = level.apps.get(&app) else {
                out.push(
                    &file,
                    "§7.3",
                    format!("`{app}` is not an app of the repo catalogue"),
                );
                continue;
            };
            let mut effective = catalogue.clone();
            for above in [
                level.envs.get(&(env.to_owned(), app.clone())),
                level.cluster_apps.get(&app),
                Some(&own),
            ]
            .into_iter()
            .flatten()
            {
                merge(&mut effective, above);
            }
            check_effective(level.catalog, &app, &effective, &file, out)?;
        }
    }
    Ok(())
}

fn check_effective(
    catalog: &Path,
    app: &str,
    effective: &Mapping,
    file: &str,
    out: &mut Findings,
) -> Result<()> {
    for field in STRING_FIELDS {
        if let Some(Value::String(value)) = effective.get(field)
            && value.is_empty()
        {
            out.push(
                file,
                "§8.1",
                format!("effective configuration: `{field}` must not be empty"),
            );
        }
    }
    let text = |field: &str| effective.get(field).and_then(Value::as_str);
    let (chart, path, url, revision) = (
        is_set(effective, "chart"),
        is_set(effective, "path"),
        is_set(effective, "url"),
        is_set(effective, "revision"),
    );
    if chart == path {
        out.push(
            file,
            "§8.2",
            "effective configuration: exactly one of `chart` and `path` must be set",
        );
    }
    if chart {
        if !url || !revision {
            out.push(
                file,
                "§8.2",
                "effective configuration: `chart` requires `url` and `revision`",
            );
        }
        if let Some(url) = text("url")
            && !is_chart_repository_url(url)
        {
            out.push(
                file,
                "§8.2",
                format!(
                    "effective configuration: `url` `{}` must be a Helm chart repository (`https://…`) \
                     or an OCI registry written without a scheme",
                    without_credentials(url)
                ),
            );
        }
    } else if path && url {
        if let Some(url) = text("url")
            && !is_git_url(url)
        {
            out.push(
                file,
                "§8.2",
                format!(
                    "effective configuration: `url` `{}` must be a Git repository when `path` is set",
                    without_credentials(url)
                ),
            );
        }
        if !revision {
            out.push(
                file,
                "§8.2",
                "effective configuration: `path` with `url` requires `revision`",
            );
        }
    } else if path {
        if revision {
            out.push(
                file,
                "§8.2",
                "effective configuration: `revision` must not be set when `url` is not set",
            );
        }
        if let Some(path) = text("path") {
            check_catalogue_source(catalog, app, path, file, out)?;
        }
    }
    Ok(())
}

fn check_instance_manifests(platform: &Path, dir: &Path, out: &mut Findings) -> Result<()> {
    if !is_dir(dir)? {
        return Ok(());
    }
    for entry in entries(dir)? {
        let path = entry.path();
        if is_dir(&path)? {
            check_instance_manifests(platform, &path, out)?;
        } else if path
            .file_name()
            .is_some_and(|name| KUSTOMIZATION_FILES.iter().any(|k| name == *k))
        {
            out.push(
                &relative(platform, &path),
                "§7.3",
                "an instance's `manifests/` holds plain manifests and must not contain a kustomization file",
            );
        }
    }
    Ok(())
}

fn app_file(path: &Path, file: &str, out: &mut Findings) -> Result<Mapping> {
    Ok(match load(path)? {
        Loaded::Missing => Mapping::new(),
        Loaded::Unparsable(error) => {
            out.push(file, "§8.1", format!("unparsable YAML: {error}"));
            Mapping::new()
        }
        Loaded::Value(value) => app_mapping(&value, file, out),
    })
}

fn check_name(name: &str, file: &str, out: &mut Findings) {
    if !is_valid_name(name) {
        out.push(
            file,
            "§5",
            format!(
                "`{name}` is not a valid name: lowercase letters, digits and `-`, starting and ending \
                 with a letter or a digit, at most 63 characters"
            ),
        );
    }
}

fn has_instances(dir: &Path) -> Result<bool> {
    for (_, app_dir) in subdirectories(dir)? {
        for (_, instance_dir) in subdirectories(&app_dir)? {
            if is_file(&instance_dir.join("app.yaml"))? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn subdirectories(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    if !is_dir(dir)? {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in entries(dir)? {
        let file_type = entry
            .file_type()
            .with_context(|| unreadable(&entry.path()))?;
        if file_type.is_dir() {
            dirs.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            ));
        }
    }
    dirs.sort();
    Ok(dirs)
}

pub(crate) fn entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let context = || unreadable(dir);
    fs::read_dir(dir)
        .with_context(context)?
        .map(|entry| entry.with_context(context))
        .collect()
}

pub(crate) fn is_file(path: &Path) -> Result<bool> {
    Ok(metadata(path)?.is_some_and(|m| m.is_file()))
}

pub(crate) fn is_dir(path: &Path) -> Result<bool> {
    Ok(metadata(path)?.is_some_and(|m| m.is_dir()))
}

/// `Path::is_file` and `Path::is_dir` answer `false` when permission is
/// denied, which would skip an unreadable directory as if it were absent.
fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error).with_context(|| unreadable(path)),
    }
}

pub(crate) fn unreadable(path: &Path) -> String {
    format!("cannot read {}", path.display())
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
