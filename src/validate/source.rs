use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde_yaml_ng::Value;
use tracing::debug;

use super::config::{Loaded, load};
use super::{Findings, entries, is_dir, is_file, relative, unreadable};

/// The file names that make a directory a Kustomize source (layout specification §8.3).
pub(super) const KUSTOMIZATION_FILES: [&str; 3] =
    ["kustomization.yaml", "kustomization.yml", "Kustomization"];

static JSONNET_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\bimport(?<raw>str|bin)?\s*(?:'(?<single>[^']*)'|"(?<double>[^"]*)")"#)
        .expect("valid regex")
});

/// The shared Jsonnet libraries of the repo catalogue (layout specification §6.7).
const LIBRARIES: &str = "lib";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Chart,
    Jsonnet,
    Manifests,
}

/// Checks the source a `path` with no `url` designates in the repo catalogue
/// (layout specification §8.2, §8.3). `file` is the instance `app.yaml` the effective
/// configuration belongs to.
pub(super) fn check_catalogue_source(
    catalog: &Path,
    app: &str,
    path: &str,
    file: &str,
    out: &mut Findings,
) -> Result<()> {
    let manifests = format!("apps/{app}/manifests");
    let jsonnet = format!("apps/{app}/jsonnet");
    let charts = format!("apps/{app}/charts/");
    let is_chart = path
        .strip_prefix(&charts)
        .is_some_and(|chart| !chart.is_empty() && !chart.contains('/'));
    let kind = if is_chart {
        Kind::Chart
    } else if path == jsonnet {
        Kind::Jsonnet
    } else if path == manifests {
        Kind::Manifests
    } else {
        out.push(
            file,
            "§8.2",
            format!(
                "effective configuration: `path` `{path}` has no `url`, so it must be \
                 `{charts}<chart>`, `{jsonnet}` or `{manifests}`"
            ),
        );
        return Ok(());
    };
    let dir = catalog.join(path);
    if !is_dir(&dir)? {
        out.warn(
            file,
            None,
            format!("`{path}` does not exist in the repo catalogue: Argo CD will fail to render this source"),
        );
        return Ok(());
    }
    let app_dir = catalog.join("apps").join(app);
    if kind == Kind::Chart {
        check_chart_dependencies(catalog, &app_dir, &dir, out)?;
    } else {
        if is_file(&dir.join("Chart.yaml"))? {
            out.push(
                &relative(catalog, &dir.join("Chart.yaml")),
                "§8.3",
                format!(
                    "`{path}` must not contain a `Chart.yaml`: only `{charts}<chart>` holds a chart"
                ),
            );
        }
        if kind == Kind::Jsonnet {
            check_jsonnet_directory(catalog, path, &dir, out)?;
        } else {
            check_kustomization(catalog, &app_dir, &dir, out)?;
        }
    }
    check_jsonnet_imports(catalog, app, kind, Path::new(path), out)
}

/// Only the files directly in the directory count, as for the source type (§8.3).
fn check_jsonnet_directory(
    catalog: &Path,
    path: &str,
    dir: &Path,
    out: &mut Findings,
) -> Result<()> {
    let mut has_jsonnet = false;
    for entry in entries(dir)? {
        let entry = entry.path();
        has_jsonnet |= entry.extension().is_some_and(|ext| ext == "jsonnet") && is_file(&entry)?;
    }
    if !has_jsonnet {
        out.push(
            &format!("{path}/"),
            "§6.6",
            format!("`{path}` must contain at least one `*.jsonnet` file directly in it"),
        );
    }
    for name in KUSTOMIZATION_FILES {
        let kustomization = dir.join(name);
        if is_file(&kustomization)? {
            out.push(
                &relative(catalog, &kustomization),
                "§6.6",
                format!("`{path}` must not contain a kustomization file"),
            );
        }
    }
    Ok(())
}

fn check_kustomization(
    catalog: &Path,
    app_dir: &Path,
    dir: &Path,
    out: &mut Findings,
) -> Result<()> {
    for name in KUSTOMIZATION_FILES {
        let path = dir.join(name);
        let file = relative(catalog, &path);
        let value = match load(&path)? {
            Loaded::Missing => continue,
            Loaded::Unparsable(error) => {
                out.push(&file, "§8.3", format!("unparsable YAML: {error}"));
                continue;
            }
            Loaded::Value(value) => value,
        };
        let mut references: Vec<&str> = Vec::new();
        for key in ["resources", "bases", "components", "patchesStrategicMerge"] {
            if let Some(Value::Sequence(items)) = value.get(key) {
                references.extend(items.iter().filter_map(Value::as_str));
            }
        }
        if let Some(Value::Sequence(patches)) = value.get("patches") {
            references.extend(
                patches
                    .iter()
                    .filter_map(|patch| patch.get("path")?.as_str()),
            );
        }
        for reference in references.into_iter().filter(|r| !is_remote(r)) {
            if !stays_inside(dir, reference, app_dir) {
                out.push(&file, "§8.2", outside_message(reference, catalog, app_dir));
            }
        }
    }
    Ok(())
}

fn check_chart_dependencies(
    catalog: &Path,
    app_dir: &Path,
    chart_dir: &Path,
    out: &mut Findings,
) -> Result<()> {
    let path = chart_dir.join("Chart.yaml");
    let Loaded::Value(chart) = load(&path)? else {
        debug!(path = %path.display(), "Chart.yaml missing or unparsable: chart dependencies not checked");
        return Ok(());
    };
    let Some(Value::Sequence(dependencies)) = chart.get("dependencies") else {
        return Ok(());
    };
    for repository in dependencies
        .iter()
        .filter_map(|dependency| dependency.get("repository")?.as_str())
    {
        if let Some(local) = repository.strip_prefix("file://")
            && !stays_inside(chart_dir, local, app_dir)
        {
            out.push(
                &relative(catalog, &path),
                "§8.2",
                outside_message(repository, catalog, app_dir),
            );
        }
    }
    Ok(())
}

/// Checks the imports of the Jsonnet files under `source`, and of every file
/// they reach. Paths are relative to the catalogue.
fn check_jsonnet_imports(
    catalog: &Path,
    app: &str,
    kind: Kind,
    source: &Path,
    out: &mut Findings,
) -> Result<()> {
    let app_dir = Path::new("apps").join(app);
    let libraries = Path::new(LIBRARIES);
    let mut pending = Vec::new();
    collect_jsonnet_files(catalog, source, &mut pending)?;
    let mut checked = BTreeSet::new();
    while let Some(file) = pending.pop() {
        if !checked.insert(file.clone()) {
            continue;
        }
        let path = catalog.join(&file);
        let text = fs::read_to_string(&path).with_context(|| unreadable(&path))?;
        debug!(path = %path.display(), "checking jsonnet imports");
        let file_dir = file.parent().unwrap_or(source);
        let mut search_path = vec![file_dir];
        if kind == Kind::Jsonnet {
            search_path.extend([libraries, source]);
        }
        for captures in JSONNET_IMPORT.captures_iter(&text) {
            let Some(import) = captures.name("single").or_else(|| captures.name("double")) else {
                continue;
            };
            let import = import.as_str();
            let resolved = resolve_import(catalog, &search_path, import)?;
            let target = resolved
                .clone()
                .unwrap_or_else(|| normalise(&file_dir.join(import)));
            let is_library = kind == Kind::Jsonnet && target.starts_with(libraries);
            if !is_library && !target.starts_with(&app_dir) {
                let app_dir = catalog.join(&app_dir);
                let message = if kind == Kind::Jsonnet {
                    format!(
                        "`{import}` reads files outside `{}/` and `lib/`: a source `apps/{app}/jsonnet` \
                         must stay within its app and the shared libraries",
                        relative(catalog, &app_dir)
                    )
                } else {
                    outside_message(import, catalog, &app_dir)
                };
                out.push(&relative(catalog, &path), "§8.2", message);
            } else if let Some(resolved) = resolved {
                // `importstr` and `importbin` read data, which imports nothing.
                if captures.name("raw").is_none() {
                    pending.push(resolved);
                }
            } else if kind == Kind::Manifests && is_file(&catalog.join(libraries).join(import))? {
                out.warn(
                    &relative(catalog, &path),
                    Some("§6.5"),
                    format!(
                        "`{import}` is only found in `lib/`, which is not on the library search \
                         path of `manifests/`: a source importing it lives in `apps/{app}/jsonnet`"
                    ),
                );
            }
        }
    }
    Ok(())
}

fn collect_jsonnet_files(catalog: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in entries(&catalog.join(dir))? {
        let path = dir.join(entry.file_name());
        if is_dir(&entry.path())? {
            collect_jsonnet_files(catalog, &path, files)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "jsonnet" || ext == "libsonnet")
        {
            files.push(path);
        }
    }
    Ok(())
}

/// The first file `import` designates, trying each directory of `search_path` in order.
fn resolve_import(catalog: &Path, search_path: &[&Path], import: &str) -> Result<Option<PathBuf>> {
    for dir in search_path {
        let candidate = normalise(&dir.join(import));
        if is_file(&catalog.join(&candidate))? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn outside_message(reference: &str, catalog: &Path, app_dir: &Path) -> String {
    format!(
        "`{reference}` reads files outside `{}/`: a source held by the repo catalogue must stay within its app",
        relative(catalog, app_dir)
    )
}

fn is_remote(reference: &str) -> bool {
    reference.contains("//") || reference.contains("?ref=") || reference.starts_with("git@")
}

fn stays_inside(from: &Path, reference: &str, root: &Path) -> bool {
    let reference = Path::new(reference);
    if reference.is_absolute() {
        return false;
    }
    normalise(&from.join(reference)).starts_with(normalise(root))
}

fn normalise(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normal.components().next_back() {
                Some(Component::Normal(_)) => {
                    normal.pop();
                }
                Some(Component::RootDir) => {}
                _ => normal.push(component),
            },
            other => normal.push(other),
        }
    }
    normal
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_remote, normalise, stays_inside};

    #[test]
    fn resolves_references_lexically() {
        let root = Path::new("/c/apps/a");
        let dir = Path::new("/c/apps/a/manifests");
        assert!(stays_inside(dir, "../lib/x.libsonnet", root));
        assert!(stays_inside(dir, "base/deployment.yaml", root));
        assert!(!stays_inside(dir, "../../b/manifests", root));
        assert!(!stays_inside(dir, "/etc/passwd", root));
        assert!(!stays_inside(dir, "../../a-other/x", root));
    }

    #[test]
    fn keeps_the_parent_directories_it_cannot_resolve() {
        assert_eq!(
            normalise(Path::new("apps/../../apps/a")),
            Path::new("../apps/a")
        );
        assert_eq!(normalise(Path::new("../../a")), Path::new("../../a"));
        assert_eq!(normalise(Path::new("/../a")), Path::new("/a"));
    }

    #[test]
    fn recognises_remote_kustomize_targets() {
        assert!(is_remote(
            "https://github.com/kubeflow/manifests//apps?ref=v1.11.0"
        ));
        assert!(is_remote("github.com/kubeflow/manifests//apps?ref=v1"));
        assert!(is_remote("git@github.com:org/repo.git"));
        assert!(!is_remote("../base"));
        assert!(!is_remote("deployment.yaml"));
    }
}
