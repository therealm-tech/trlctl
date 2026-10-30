use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_yaml_ng::Value;

use super::config::{Loaded, load};
use super::{Findings, relative};

/// The file names that make a directory a Kustomize source (SPEC.md §8.3).
pub(super) const KUSTOMIZATION_FILES: [&str; 3] =
    ["kustomization.yaml", "kustomization.yml", "Kustomization"];

static JSONNET_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\bimport(?:str|bin)?\s*(?:'([^']*)'|"([^"]*)")"#).expect("valid regex")
});

/// Checks the source a `path` with no `url` designates in the repo catalogue
/// (SPEC.md §8.2, §8.3). `file` is the instance `app.yaml` the effective
/// configuration belongs to.
pub(super) fn check_catalogue_source(
    catalog: &Path,
    app: &str,
    path: &str,
    file: &str,
    out: &mut Findings,
) -> io::Result<()> {
    let manifests = format!("apps/{app}/manifests");
    let charts = format!("apps/{app}/charts/");
    let is_chart = path
        .strip_prefix(&charts)
        .is_some_and(|chart| !chart.is_empty() && !chart.contains('/'));
    if path != manifests && !is_chart {
        out.push(
            file,
            "§8.2",
            format!(
                "effective configuration: `path` `{path}` has no `url`, so it must be \
                 `{charts}<chart>` or `{manifests}`"
            ),
        );
        return Ok(());
    }
    let dir = catalog.join(path);
    if !dir.is_dir() {
        out.warn(
            file,
            None,
            format!("`{path}` does not exist in the repo catalogue: Argo CD will fail to render this source"),
        );
        return Ok(());
    }
    let app_dir = catalog.join("apps").join(app);
    if is_chart {
        check_chart_dependencies(catalog, &app_dir, &dir, out)?;
    } else {
        if dir.join("Chart.yaml").is_file() {
            out.push(
                &relative(catalog, &dir.join("Chart.yaml")),
                "§8.3",
                format!("`{manifests}` must not contain a `Chart.yaml`: only `{charts}<chart>` holds a chart"),
            );
        }
        check_kustomization(catalog, &app_dir, &dir, out)?;
    }
    check_jsonnet_imports(catalog, &app_dir, &app_dir, out)
}

fn check_kustomization(
    catalog: &Path,
    app_dir: &Path,
    dir: &Path,
    out: &mut Findings,
) -> io::Result<()> {
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
) -> io::Result<()> {
    let path = chart_dir.join("Chart.yaml");
    let Loaded::Value(chart) = load(&path)? else {
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

fn check_jsonnet_imports(
    catalog: &Path,
    app_dir: &Path,
    dir: &Path,
    out: &mut Findings,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            check_jsonnet_imports(catalog, app_dir, &path, out)?;
            continue;
        }
        let is_jsonnet = path
            .extension()
            .is_some_and(|ext| ext == "jsonnet" || ext == "libsonnet");
        if !is_jsonnet {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let file_dir = path.parent().unwrap_or(dir);
        for captures in JSONNET_IMPORT.captures_iter(&text) {
            let Some(import) = captures.get(1).or_else(|| captures.get(2)) else {
                continue;
            };
            if !stays_inside(file_dir, import.as_str(), app_dir) {
                out.push(
                    &relative(catalog, &path),
                    "§8.2",
                    outside_message(import.as_str(), catalog, app_dir),
                );
            }
        }
    }
    Ok(())
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
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_remote, stays_inside};

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
