use std::fs;
use std::io;
use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

use super::Findings;
use crate::urls::is_git_url;

/// The `app.yaml` fields whose type is a string (SPEC.md §8.1).
pub(super) const STRING_FIELDS: [&str; 6] =
    ["url", "chart", "path", "revision", "project", "namespace"];

pub(super) enum Loaded {
    Missing,
    Unparsable(String),
    Value(Value),
}

pub(super) fn load(path: &Path) -> io::Result<Loaded> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(error) => return Err(error),
    };
    if text.trim().is_empty() {
        return Ok(Loaded::Value(Value::Null));
    }
    Ok(match serde_yaml_ng::from_str(&text) {
        Ok(value) => Loaded::Value(value),
        Err(error) => Loaded::Unparsable(error.to_string()),
    })
}

/// The `app` mapping of an `app.yaml` document, after reporting what breaks
/// SPEC.md §8.1. A document that contributes nothing yields an empty mapping.
pub(super) fn app_mapping(value: &Value, file: &str, out: &mut Findings) -> Mapping {
    let root = match value {
        Value::Null => return Mapping::new(),
        Value::Mapping(root) => root,
        _ => {
            out.push(
                file,
                "§8.1",
                "the document must be a mapping whose only root key is `app`",
            );
            return Mapping::new();
        }
    };
    for key in root.keys().filter(|key| key.as_str() != Some("app")) {
        out.push(
            file,
            "§8.1",
            format!(
                "root key `{}` is not allowed: `app` is the only root key",
                display(key)
            ),
        );
    }
    match root.get("app") {
        None | Some(Value::Null) => Mapping::new(),
        Some(Value::Mapping(app)) => {
            check_field_types(app, file, out);
            app.clone()
        }
        Some(other) => {
            out.push(
                file,
                "§8.1",
                format!("`app` must be a mapping, found {}", kind(other)),
            );
            Mapping::new()
        }
    }
}

fn check_field_types(app: &Mapping, file: &str, out: &mut Findings) {
    for field in STRING_FIELDS {
        match app.get(field) {
            None | Some(Value::Null | Value::String(_)) => {}
            Some(other) => out.push(
                file,
                "§8.1",
                format!("`{field}` must be a string, found {}", kind(other)),
            ),
        }
    }
    match app.get("syncOptions") {
        None | Some(Value::Null) => {}
        Some(Value::Sequence(items)) if items.iter().all(Value::is_string) => {}
        Some(_) => out.push(file, "§8.1", "`syncOptions` must be a sequence of strings"),
    }
    match app.get("managedNamespaceMetadata") {
        None | Some(Value::Null | Value::Mapping(_)) => {}
        Some(other) => out.push(
            file,
            "§8.1",
            format!(
                "`managedNamespaceMetadata` must be a mapping, found {}",
                kind(other)
            ),
        ),
    }
}

/// Overrides `base` with `over` as SPEC.md §10.2 describes: mappings combine
/// key by key, anything else set replaces, and a null value is not set.
pub(super) fn merge(base: &mut Mapping, over: &Mapping) {
    for (key, value) in over {
        if value.is_null() {
            continue;
        }
        match (base.get_mut(key), value) {
            (Some(Value::Mapping(below)), Value::Mapping(above)) => merge(below, above),
            _ => {
                base.insert(key.clone(), without_nulls(value));
            }
        }
    }
}

fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Mapping(mapping) => Value::Mapping(
            mapping
                .iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), without_nulls(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Whether `field` is set in `app`: present with a non-null value (SPEC.md §3).
pub(super) fn is_set(app: &Mapping, field: &str) -> bool {
    app.get(field).is_some_and(|value| !value.is_null())
}

pub(super) struct ClusterRepos {
    pub platform_url: String,
    pub platform_path: String,
}

/// The repositories a `cluster.yaml` names, after reporting what breaks
/// SPEC.md §9. `None` when the platform repository cannot be read from it.
pub(super) fn cluster_repos(value: &Value, file: &str, out: &mut Findings) -> Option<ClusterRepos> {
    let Value::Mapping(root) = value else {
        out.push(
            file,
            "§9",
            "the document must be a mapping whose only root key is `repo`",
        );
        return None;
    };
    for key in root.keys().filter(|key| key.as_str() != Some("repo")) {
        out.push(
            file,
            "§9",
            format!(
                "root key `{}` is not allowed: `repo` is the only root key",
                display(key)
            ),
        );
    }
    let Some(Value::Mapping(repo)) = root.get("repo") else {
        out.push(
            file,
            "§9",
            "`repo` must be a mapping holding `catalog` and `platform`",
        );
        return None;
    };
    let catalog = check_role(repo, "catalog", file, out);
    let platform = check_role(repo, "platform", file, out);
    catalog?;
    platform
}

fn check_role(repo: &Mapping, role: &str, file: &str, out: &mut Findings) -> Option<ClusterRepos> {
    let Some(Value::Mapping(fields)) = repo.get(role) else {
        out.push(
            file,
            "§9",
            format!("`repo.{role}` is required and must be a mapping"),
        );
        return None;
    };
    let required = |name: &str, out: &mut Findings| match fields.get(name) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => {
            out.push(
                file,
                "§9",
                format!("`repo.{role}.{name}` is required and must be a non-empty string"),
            );
            None
        }
    };
    let url = required("url", out);
    let revision = required("revision", out);
    if let Some(url) = &url
        && !is_git_url(url)
    {
        out.push(
            file,
            "§9",
            format!("`repo.{role}.url` `{url}` is not a Git repository URL (Section 8.2)"),
        );
    }
    let path = match fields.get("path") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(path)) => path.clone(),
        Some(other) => {
            out.push(
                file,
                "§9",
                format!("`repo.{role}.path` must be a string, found {}", kind(other)),
            );
            String::new()
        }
    };
    revision?;
    Some(ClusterRepos {
        platform_url: url?,
        platform_path: path,
    })
}

fn display(key: &Value) -> String {
    key.as_str().map_or_else(
        || serde_yaml_ng::to_string(key).map_or_else(|_| "?".to_owned(), |s| s.trim().to_owned()),
        ToOwned::to_owned,
    )
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a sequence",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

#[cfg(test)]
mod tests {
    use serde_yaml_ng::{Mapping, Value};

    use super::merge;

    fn mapping(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn merges_mappings_recursively() {
        let mut base = mapping("managedNamespaceMetadata: {labels: {a: '1'}}\nnamespace: x");
        merge(
            &mut base,
            &mapping("managedNamespaceMetadata: {labels: {b: '2'}}"),
        );
        assert_eq!(
            base,
            mapping("managedNamespaceMetadata: {labels: {a: '1', b: '2'}}\nnamespace: x")
        );
    }

    #[test]
    fn replaces_scalars_and_sequences_whole() {
        let mut base = mapping("syncOptions: [A, B]\nrevision: main");
        merge(&mut base, &mapping("syncOptions: [C]\nrevision: v1"));
        assert_eq!(base, mapping("syncOptions: [C]\nrevision: v1"));
    }

    #[test]
    fn empty_values_override_and_nulls_do_not() {
        let mut base = mapping("syncOptions: [A]\nnamespace: x\nchart: c");
        merge(
            &mut base,
            &mapping("syncOptions: []\nnamespace: ''\nchart: null"),
        );
        assert_eq!(base, mapping("syncOptions: []\nnamespace: ''\nchart: c"));
    }

    #[test]
    fn nested_nulls_are_not_set() {
        let mut base = Mapping::new();
        merge(
            &mut base,
            &mapping("managedNamespaceMetadata: {labels: {a: null, b: '2'}}"),
        );
        assert_eq!(
            base,
            mapping("managedNamespaceMetadata: {labels: {b: '2'}}")
        );
        assert!(!base.contains_key(Value::from("chart")));
    }
}
