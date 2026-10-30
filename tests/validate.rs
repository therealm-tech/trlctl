use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use tempfile::TempDir;
use trlctl::validate::{Finding, Severity, validate};

/// A temporary copy of the worked example, outside any Git checkout.
struct Example {
    _dir: TempDir,
    root: PathBuf,
}

impl Example {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("example");
        copy_dir(Path::new("tests/fixtures/example"), &root);
        Self { _dir: dir, root }
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.root.join(path)).unwrap();
    }

    fn findings(&self) -> Vec<Finding> {
        validate(&self.root, &self.root).unwrap()
    }

    /// Asserts that an error of `section` on `file` mentions `fragment`.
    fn assert_finding(&self, file: &str, section: &str, fragment: &str) {
        self.assert_severity(Severity::Error, file, Some(section), fragment);
    }

    /// Asserts that a warning on `file`, of `section` when given, mentions
    /// `fragment`, and that the layout has no error.
    fn assert_warning(&self, file: &str, section: Option<&str>, fragment: &str) {
        self.assert_severity(Severity::Warning, file, section, fragment);
        let findings = self.findings();
        assert!(
            !findings.iter().any(Finding::is_error),
            "unexpected error in {findings:#?}"
        );
    }

    fn assert_severity(
        &self,
        severity: Severity,
        file: &str,
        section: Option<&str>,
        fragment: &str,
    ) {
        let findings = self.findings();
        assert!(
            findings.iter().any(|f| f.severity == severity
                && f.file == file
                && f.section.as_deref() == section
                && f.message.contains(fragment)),
            "no {severity:?} {section:?} finding on {file} mentioning {fragment:?} in {findings:#?}"
        );
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The binary, isolated from any `TRLCTL_*` variable of the calling shell.
fn trlctl() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("trlctl");
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("TRLCTL_")) {
        cmd.env_remove(name);
    }
    cmd
}

const WHOAMI_DEV: &str = "clusters/nonprod-1/dev/whoami/default/app.yaml";
const ARGOCD_PROD: &str = "clusters/mgmt-1/prod/argocd/default/app.yaml";

#[test]
fn the_worked_example_follows_the_specification() {
    assert_eq!(Example::new().findings(), Vec::new());
}

#[test]
fn rejects_invalid_names() {
    let example = Example::new();
    example.write(
        "apps/Bad_App/app.yaml",
        "app:\n  path: apps/Bad_App/manifests\n",
    );
    example.write("clusters/nonprod-1/QA/whoami/default/app.yaml", "");
    example.assert_finding("apps/Bad_App/", "§5", "not a valid name");
    example.assert_finding("clusters/nonprod-1/QA/", "§5", "not a valid name");
}

#[test]
fn rejects_an_env_named_like_an_app() {
    let example = Example::new();
    example.write("clusters/nonprod-1/katib/whoami/default/app.yaml", "");
    example.assert_finding(
        "clusters/nonprod-1/katib/",
        "§5",
        "must not be named like an app",
    );
}

#[test]
fn requires_cluster_yaml() {
    let example = Example::new();
    example.remove("clusters/prod-eu-1/cluster.yaml");
    example.assert_finding(
        "clusters/prod-eu-1/cluster.yaml",
        "§7.4",
        "must hold a `cluster.yaml`",
    );
}

#[test]
fn checks_cluster_yaml() {
    let example = Example::new();
    example.write(
        "clusters/prod-eu-1/cluster.yaml",
        "repo:\n  catalog:\n    url: ghcr.io/org/charts\n  platform:\n    url: \
         https://git.example.com/platform/gitops.git\n    revision: main\nextra: true\n",
    );
    let file = "clusters/prod-eu-1/cluster.yaml";
    example.assert_finding(file, "§9", "root key `extra` is not allowed");
    example.assert_finding(file, "§9", "`repo.catalog.revision` is required");
    example.assert_finding(file, "§9", "is not a Git repository URL");
}

#[test]
fn compares_repo_platform_with_the_git_checkout() {
    let example = Example::new();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&example.root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    git(&[
        "remote",
        "add",
        "origin",
        "git@git.example.com:platform/gitops.git",
    ]);
    assert_eq!(example.findings(), Vec::new());
    git(&[
        "remote",
        "set-url",
        "origin",
        "git@git.example.com:platform/other.git",
    ]);
    example.assert_warning(
        "clusters/mgmt-1/cluster.yaml",
        Some("§9"),
        "does not designate the `origin` of this checkout",
    );
}

#[test]
fn ignores_equivalent_forms_of_the_git_origin() {
    let example = Example::new();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&example.root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    git(&[
        "remote",
        "add",
        "origin",
        "ssh://git@GIT.example.com:2222/platform/gitops",
    ]);
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn reports_unparsable_yaml() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app: [\n");
    example.assert_finding(WHOAMI_DEV, "§8.1", "unparsable YAML");
}

#[test]
fn checks_app_yaml_structure() {
    let example = Example::new();
    example.write(
        WHOAMI_DEV,
        "app:\n  namespace: 3\n  syncOptions:\n    - 1\nother: x\n",
    );
    example.assert_finding(WHOAMI_DEV, "§8.1", "root key `other` is not allowed");
    example.assert_finding(
        WHOAMI_DEV,
        "§8.1",
        "`namespace` must be a string, found a number",
    );
    example.assert_finding(
        WHOAMI_DEV,
        "§8.1",
        "`syncOptions` must be a sequence of strings",
    );
}

#[test]
fn rejects_an_unquoted_numeric_revision() {
    let example = Example::new();
    example.write(ARGOCD_PROD, "app:\n  revision: 1.10\n");
    example.assert_finding(ARGOCD_PROD, "§8.1", "`revision` must be a string");
}

#[test]
fn requires_one_source_in_the_catalogue() {
    let example = Example::new();
    example.write(
        "apps/katib/app.yaml",
        "app:\n  path: apps/katib/manifests\n  chart: katib\n",
    );
    example.assert_finding(
        "apps/katib/app.yaml",
        "§6.2",
        "exactly one of `chart` and `path`",
    );
}

#[test]
fn requires_one_source_in_the_effective_configuration() {
    let example = Example::new();
    example.write(ARGOCD_PROD, "app:\n  path: apps/argocd/manifests\n");
    example.assert_finding(ARGOCD_PROD, "§8.2", "exactly one of `chart` and `path`");
}

#[test]
fn a_null_value_does_not_override() {
    let example = Example::new();
    example.write(ARGOCD_PROD, "app:\n  chart: null\n  namespace: ~\n");
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn rejects_empty_strings_in_the_effective_configuration() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  namespace: ''\n");
    example.assert_finding(WHOAMI_DEV, "§8.1", "`namespace` must not be empty");
}

#[test]
fn requires_url_and_revision_with_chart() {
    let example = Example::new();
    example.write(
        "apps/argocd/app.yaml",
        "app:\n  url: git@github.com:argoproj/argo-helm.git\n  chart: argo-cd\n",
    );
    example.assert_finding(ARGOCD_PROD, "§8.2", "`chart` requires `url` and `revision`");
    example.assert_finding(ARGOCD_PROD, "§8.2", "must be a Helm chart repository");
}

#[test]
fn rejects_an_oci_scheme() {
    let example = Example::new();
    example.write(
        ARGOCD_PROD,
        "app:\n  url: oci://ghcr.io/argoproj/argo-helm\n",
    );
    example.assert_finding(ARGOCD_PROD, "§8.2", "OCI registry written without a scheme");
}

#[test]
fn rejects_a_revision_without_url() {
    let example = Example::new();
    example.write(
        "envs/prod/whoami/app.yaml",
        "app:\n  revision: whoami-0.1.0\n",
    );
    example.assert_finding(
        "clusters/prod-eu-1/prod/whoami/default/app.yaml",
        "§8.2",
        "`revision` must not be set when `url` is not set",
    );
}

#[test]
fn requires_a_git_url_and_a_revision_with_path_and_url() {
    let example = Example::new();
    example.write(
        "clusters/nonprod-1/dev/katib/default/app.yaml",
        "app:\n  url: ghcr.io/org/charts\n",
    );
    let file = "clusters/nonprod-1/dev/katib/default/app.yaml";
    example.assert_finding(file, "§8.2", "must be a Git repository when `path` is set");
    example.assert_finding(file, "§8.2", "`path` with `url` requires `revision`");
}

#[test]
fn limits_a_catalogue_path_to_the_instance_app() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  path: apps/katib/manifests\n");
    example.assert_finding(
        WHOAMI_DEV,
        "§8.2",
        "must be `apps/whoami/charts/<chart>` or `apps/whoami/manifests`",
    );
}

#[test]
fn warns_about_a_missing_designated_directory() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  path: apps/whoami/charts/missing\n");
    example.assert_warning(
        WHOAMI_DEV,
        None,
        "`apps/whoami/charts/missing` does not exist in the repo catalogue",
    );
}

#[test]
fn rejects_an_app_that_is_not_in_the_catalogue() {
    let example = Example::new();
    let file = "clusters/nonprod-1/dev/ghost/default/app.yaml";
    example.write(file, "");
    example.assert_finding(file, "§7.3", "`ghost` is not an app of the repo catalogue");
}

#[test]
fn rejects_a_kustomization_in_instance_manifests() {
    let example = Example::new();
    let file = "clusters/prod-eu-1/prod/whoami/default/manifests/extra/kustomization.yaml";
    example.write(file, "resources:\n  - ../pod-disruption-budget.yaml\n");
    example.assert_finding(file, "§7.3", "must not contain a kustomization file");
}

#[test]
fn rejects_a_chart_in_manifests() {
    let example = Example::new();
    example.write(
        "apps/katib/manifests/Chart.yaml",
        "apiVersion: v2\nname: katib\nversion: 0.1.0\n",
    );
    example.assert_finding(
        "apps/katib/manifests/Chart.yaml",
        "§8.3",
        "must not contain a `Chart.yaml`",
    );
}

#[test]
fn keeps_kustomize_sources_within_their_app() {
    let example = Example::new();
    example.write(
        "apps/katib/manifests/kustomization.yaml",
        "resources:\n  - https://github.com/kubeflow/manifests//applications/katib?ref=v1.11.0\n  \
         - ../../whoami/charts/whoami\n",
    );
    example.assert_finding(
        "apps/katib/manifests/kustomization.yaml",
        "§8.2",
        "`../../whoami/charts/whoami` reads files outside `apps/katib/`",
    );
}

#[test]
fn keeps_jsonnet_imports_within_their_app() {
    let example = Example::new();
    example.write(
        "apps/team-quotas/manifests/main.jsonnet",
        "local other = import '../../whoami/lib/x.libsonnet';\n[]\n",
    );
    example.assert_finding(
        "apps/team-quotas/manifests/main.jsonnet",
        "§8.2",
        "reads files outside `apps/team-quotas/`",
    );
}

#[test]
fn keeps_chart_dependencies_within_their_app() {
    let example = Example::new();
    example.write(
        "apps/whoami/charts/whoami/Chart.yaml",
        "apiVersion: v2\nname: whoami\nversion: 0.1.0\ndependencies:\n  - name: common\n    \
         version: 0.1.0\n    repository: file://../../../katib/charts/common\n",
    );
    example.assert_finding(
        "apps/whoami/charts/whoami/Chart.yaml",
        "§8.2",
        "reads files outside `apps/whoami/`",
    );
}

#[test]
fn cli_exits_zero_on_a_valid_layout() {
    let example = Example::new();
    trlctl()
        .args(["validate", "--catalog"])
        .arg(&example.root)
        .arg("--platform")
        .arg(&example.root)
        .assert()
        .success()
        .stderr(contains("the layout follows the specification"));
}

#[test]
fn cli_exits_one_and_prints_json_on_violations() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  namespace: ''\n");
    let output = trlctl()
        .args(["validate", "--output", "json", "--catalog"])
        .arg(&example.root)
        .arg("--platform")
        .arg(&example.root)
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["findings"][0]["file"], WHOAMI_DEV);
    assert_eq!(json["findings"][0]["severity"], "error");
    assert_eq!(json["findings"][0]["section"], "§8.1");
}

#[test]
fn cli_exits_zero_on_warnings_only() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  path: apps/whoami/charts/missing\n");
    trlctl()
        .args(["validate", "--catalog"])
        .arg(&example.root)
        .arg("--platform")
        .arg(&example.root)
        .assert()
        .success()
        .stdout(contains(format!(
            "{WHOAMI_DEV}: warning: `apps/whoami/charts/missing` does not exist"
        )))
        .stderr(contains("the layout follows the specification, 1 warning"));
    let output = trlctl()
        .args(["validate", "--output", "json", "--catalog"])
        .arg(&example.root)
        .arg("--platform")
        .arg(&example.root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["findings"][0]["severity"], "warning");
    assert!(json["findings"][0].get("section").is_none());
}

#[test]
fn cli_exits_two_on_a_usage_error() {
    trlctl()
        .args(["validate", "--output", "xml"])
        .assert()
        .code(2);
}
