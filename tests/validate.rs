use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
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

    /// The worked example, with `team-quotas` as a Jsonnet source using `lib/`.
    fn jsonnet() -> Self {
        let example = Self::new();
        example.write(
            "apps/team-quotas/app.yaml",
            "app:\n  path: apps/team-quotas/jsonnet\n",
        );
        example.write(
            "apps/team-quotas/jsonnet/main.jsonnet",
            "local team = import 'team.libsonnet';\n[]\n",
        );
        example.write("lib/team.libsonnet", "{}\n");
        example
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.root.join(path)).unwrap();
    }

    /// Runs `f` while `path` is unreadable, then restores its mode; `None`
    /// when permissions are not enforced, as for root.
    #[cfg(unix)]
    fn while_unreadable<T>(&self, path: &str, f: impl FnOnce() -> T) -> Option<T> {
        use std::os::unix::fs::PermissionsExt;

        let path = self.root.join(path);
        let permissions = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let enforced = if path.is_dir() {
            fs::read_dir(&path).is_err()
        } else {
            fs::read(&path).is_err()
        };
        let result = enforced.then(f);
        fs::set_permissions(&path, permissions).unwrap();
        if result.is_none() {
            eprintln!("skipped: permissions are not enforced for this user");
        }
        result
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

const WHOAMI_DEV: &str = "clusters/nonprod-1/envs/dev/whoami/default/app.yaml";
const ARGOCD_PROD: &str = "clusters/mgmt-1/envs/prod/argocd/default/app.yaml";

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
    example.write("clusters/nonprod-1/envs/QA/whoami/default/app.yaml", "");
    example.write("clusters/nonprod-1/apps/Who_Ami/values.yaml", "");
    example.assert_finding("apps/Bad_App/", "§5", "not a valid name");
    example.assert_finding("clusters/nonprod-1/envs/QA/", "§5", "not a valid name");
    example.assert_finding("clusters/nonprod-1/apps/Who_Ami/", "§5", "not a valid name");
}

#[test]
fn accepts_an_env_named_like_an_app() {
    let example = Example::new();
    example.write("clusters/nonprod-1/envs/katib/whoami/default/app.yaml", "");
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn applies_the_cluster_app_level() {
    let example = Example::new();
    example.write(
        "clusters/mgmt-1/apps/argocd/app.yaml",
        "app:\n  path: apps/argocd/manifests\n",
    );
    example.assert_finding(ARGOCD_PROD, "§8.2", "exactly one of `chart` and `path`");
}

#[test]
fn ignores_other_directories_under_a_cluster() {
    let example = Example::new();
    example.write("clusters/nonprod-1/notes/README.md", "# notes\n");
    example.write("clusters/nonprod-1/ghost/values.yaml", "replicas: 2\n");
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn warns_about_instances_outside_envs() {
    let example = Example::new();
    example.write("clusters/nonprod-1/qa/whoami/default/app.yaml", "");
    example.assert_warning(
        "clusters/nonprod-1/qa/",
        Some("§5"),
        "live under `clusters/nonprod-1/envs/qa/`",
    );
}

#[test]
fn warns_about_a_cluster_app_level_outside_apps() {
    let example = Example::new();
    example.write("clusters/nonprod-1/whoami/values.yaml", "replicas: 2\n");
    example.assert_warning(
        "clusters/nonprod-1/whoami/",
        Some("§5"),
        "lives under `clusters/nonprod-1/apps/whoami/`",
    );
}

#[test]
fn warns_about_files_between_env_and_instance() {
    let example = Example::new();
    let file = "clusters/nonprod-1/envs/dev/whoami/values.yaml";
    example.write(file, "replicas: 2\n");
    example.assert_warning(file, Some("§7.3"), "`envs/dev/whoami/values.yaml`");
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
fn never_prints_the_credentials_of_a_url() {
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
    git(&["remote", "add", "origin", "placeholder"]);
    example.write(
        "clusters/mgmt-1/cluster.yaml",
        "repo:\n  catalog:\n    url: https://user:s3cr3t@git.example.com\n    revision: main\n  \
         platform:\n    url: https://user:s3cr3t@git.example.com/platform/gitops.git\n    revision: main\n",
    );
    example.write(
        WHOAMI_DEV,
        "app:\n  path: manifests\n  url: https://user:s3cr3t@git.example.com\n  revision: main\n",
    );
    // The last three are origins `to_https` cannot rewrite.
    for origin in [
        "https://user:s3cr3t@git.example.com/platform/other.git",
        "https://user:s3cr3t@git.example.com:port/platform/gitops.git",
        "ftp://user:s3cr3t@git.example.com/platform/gitops.git",
        "user:s3cr3t@git.example.com:platform/gitops.git",
    ] {
        git(&["remote", "set-url", "origin", origin]);
        let mut cmd = cargo_bin_cmd!("trlctl");
        for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("TRLCTL_")) {
            cmd.env_remove(name);
        }
        cmd.arg("validate")
            .arg("--catalog")
            .arg(&example.root)
            .arg("--platform")
            .arg(&example.root)
            .args(["--log-filter", "debug"])
            .assert()
            .stdout(contains("does not designate the `origin`"))
            .stdout(contains("is not a Git repository URL"))
            .stdout(contains("must be a Git repository when `path` is set"))
            .stdout(contains("s3cr3t").not())
            .stderr(contains("git succeeded"))
            .stderr(contains("s3cr3t").not());
    }
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
fn checks_ignore_differences_type() {
    let example = Example::new();
    example.write(
        WHOAMI_DEV,
        "app:\n  ignoreDifferences:\n    - /spec/replicas\n",
    );
    example.assert_finding(
        WHOAMI_DEV,
        "§8.1",
        "`ignoreDifferences` must be a sequence of mappings",
    );
}

#[test]
fn accepts_null_ignore_differences() {
    let example = Example::new();
    example.write(WHOAMI_DEV, "app:\n  ignoreDifferences: null\n");
    assert_eq!(example.findings(), Vec::new());
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
        "clusters/prod-eu-1/envs/prod/whoami/default/app.yaml",
        "§8.2",
        "`revision` must not be set when `url` is not set",
    );
}

#[test]
fn requires_a_git_url_and_a_revision_with_path_and_url() {
    let example = Example::new();
    example.write(
        "clusters/nonprod-1/envs/dev/katib/default/app.yaml",
        "app:\n  url: ghcr.io/org/charts\n",
    );
    let file = "clusters/nonprod-1/envs/dev/katib/default/app.yaml";
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
        "must be `apps/whoami/charts/<chart>`, `apps/whoami/jsonnet` or \
         `apps/whoami/manifests`",
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
    let file = "clusters/nonprod-1/envs/dev/ghost/default/app.yaml";
    example.write(file, "");
    example.assert_finding(file, "§7.3", "`ghost` is not an app of the repo catalogue");
}

#[test]
fn rejects_a_kustomization_in_instance_manifests() {
    let example = Example::new();
    let file = "clusters/prod-eu-1/envs/prod/whoami/default/manifests/extra/kustomization.yaml";
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
fn accepts_a_jsonnet_source() {
    let example = Example::jsonnet();
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn requires_a_jsonnet_file_directly_in_a_jsonnet_source() {
    let example = Example::jsonnet();
    example.remove("apps/team-quotas/jsonnet/main.jsonnet");
    example.write("apps/team-quotas/jsonnet/nested/main.jsonnet", "[]\n");
    example.assert_finding(
        "apps/team-quotas/jsonnet/",
        "§6.6",
        "at least one `*.jsonnet` file directly in it",
    );
}

#[test]
fn rejects_a_kustomization_directly_in_a_jsonnet_source() {
    for name in ["kustomization.yaml", "kustomization.yml", "Kustomization"] {
        let example = Example::jsonnet();
        let file = format!("apps/team-quotas/jsonnet/{name}");
        example.write(&file, "resources: []\n");
        example.assert_finding(&file, "§6.6", "must not contain a kustomization file");
    }
}

#[test]
fn ignores_a_kustomization_below_a_jsonnet_source() {
    let example = Example::jsonnet();
    example.write(
        "apps/team-quotas/jsonnet/vendor/kustomization.yaml",
        "resources: []\n",
    );
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn rejects_a_chart_in_a_jsonnet_source() {
    let example = Example::jsonnet();
    let file = "apps/team-quotas/jsonnet/Chart.yaml";
    example.write(file, "apiVersion: v2\n");
    example.assert_finding(file, "§8.3", "`apps/team-quotas/jsonnet` must not contain");
}

#[test]
fn follows_imports_through_the_libraries() {
    let example = Example::jsonnet();
    example.write("lib/team.libsonnet", "import 'nested/names.libsonnet'\n");
    example.write(
        "lib/nested/names.libsonnet",
        "[import 'labels.libsonnet', import 'local.libsonnet']\n",
    );
    example.write("lib/labels.libsonnet", "{}\n");
    // Found on the search path only, through `apps/<app>/jsonnet/`.
    example.write("apps/team-quotas/jsonnet/local.libsonnet", "{}\n");
    assert_eq!(example.findings(), Vec::new());

    example.write(
        "lib/labels.libsonnet",
        "import '../apps/katib/x.libsonnet'\n",
    );
    example.assert_finding(
        "lib/labels.libsonnet",
        "§8.2",
        "reads files outside `apps/team-quotas/` and `lib/`",
    );
}

#[test]
fn resolves_an_import_to_the_first_existing_file() {
    let example = Example::jsonnet();
    example.write(
        "apps/team-quotas/jsonnet/main.jsonnet",
        "import '../shared.libsonnet'\n",
    );
    example.write("shared.libsonnet", "{}\n");
    // Missing next to the importing file, so `lib/../shared.libsonnet` is read.
    example.assert_finding(
        "apps/team-quotas/jsonnet/main.jsonnet",
        "§8.2",
        "`../shared.libsonnet` reads files outside",
    );
    example.write("apps/team-quotas/shared.libsonnet", "{}\n");
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn follows_code_imports_only() {
    let example = Example::jsonnet();
    example.write("lib/script.txt", "import '../../outside'\n");
    example.write(
        "apps/team-quotas/jsonnet/main.jsonnet",
        "importstr 'script.txt'\n",
    );
    assert_eq!(example.findings(), Vec::new());
    example.write(
        "apps/team-quotas/jsonnet/main.jsonnet",
        "import 'script.txt'\n",
    );
    example.assert_finding("lib/script.txt", "§8.2", "`../../outside` reads files");
}

#[test]
fn stops_on_an_import_cycle() {
    let example = Example::jsonnet();
    example.write("lib/team.libsonnet", "import 'other.libsonnet'\n");
    example.write("lib/other.libsonnet", "import 'team.libsonnet'\n");
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn keeps_the_libraries_off_the_search_path_of_manifests() {
    let example = Example::new();
    example.write("lib/shared.libsonnet", "{}\n");
    let file = "apps/team-quotas/manifests/main.jsonnet";
    example.write(file, "import 'shared.libsonnet'\n");
    example.assert_warning(file, Some("§6.5"), "only found in `lib/`");
    example.write(file, "import '../../../lib/shared.libsonnet'\n");
    example.assert_finding(file, "§8.2", "reads files outside `apps/team-quotas/`");
}

#[test]
fn reads_only_the_designated_source() {
    let example = Example::new();
    example.write("apps/whoami/jsonnet/kustomization.yaml", "resources: []\n");
    example.write(
        "apps/whoami/jsonnet/main.jsonnet",
        "import '../../katib/x.libsonnet'\n",
    );
    assert_eq!(example.findings(), Vec::new());
}

#[test]
fn follows_imports_out_of_the_source_within_the_app() {
    let example = Example::new();
    example.write(
        "apps/team-quotas/lib/team.libsonnet",
        "import '../../katib/x.libsonnet'\n",
    );
    example.assert_finding(
        "apps/team-quotas/lib/team.libsonnet",
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

#[cfg(unix)]
#[test]
fn names_an_unreadable_file() {
    let example = Example::new();
    let Some(result) = example.while_unreadable("apps/whoami/app.yaml", || {
        validate(&example.root, &example.root)
    }) else {
        return;
    };
    let error = format!("{:#}", result.unwrap_err());
    let path = example.root.join("apps/whoami/app.yaml");
    assert!(
        error.starts_with(&format!("cannot read {}: ", path.display())),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn cli_exits_two_on_an_unreadable_directory() {
    let example = Example::new();
    let Some(assert) = example.while_unreadable("apps/whoami", || {
        trlctl()
            .args(["validate", "--catalog"])
            .arg(&example.root)
            .arg("--platform")
            .arg(&example.root)
            .assert()
    }) else {
        return;
    };
    let path = example.root.join("apps/whoami/app.yaml");
    assert
        .code(2)
        .stderr(contains(format!("error: cannot read {}: ", path.display())));
}

#[test]
fn cli_exits_two_on_a_usage_error() {
    trlctl()
        .args(["validate", "--output", "xml"])
        .assert()
        .code(2);
}
