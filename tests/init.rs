use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use trlctl::validate::validate;

/// The binary, isolated from any `TRLCTL_*` variable of the calling shell.
fn trlctl() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("trlctl");
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("TRLCTL_")) {
        cmd.env_remove(name);
    }
    cmd
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn discovers_an_ssh_origin_as_https() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:therealm-tech/gitops.git",
        ],
    );
    trlctl()
        .current_dir(dir.path())
        .args(["init", "--cluster", "prod-eu-1"])
        .assert()
        .success()
        .stdout(contains("clusters/prod-eu-1/cluster.yaml"));
    assert_eq!(
        read(&dir.path().join("clusters/prod-eu-1/cluster.yaml")),
        "repo:\n  catalog:\n    url: \"https://github.com/therealm-tech/gitops.git\"\n    revision: \"main\"\n  \
         platform:\n    url: \"https://github.com/therealm-tech/gitops.git\"\n    revision: \"main\"\n"
    );
    assert!(dir.path().join("apps/.gitkeep").is_file());
    assert!(dir.path().join("envs/.gitkeep").is_file());
    assert_eq!(validate(dir.path(), dir.path()).unwrap(), Vec::new());
}

#[test]
fn platform_url_defaults_to_the_catalogue_url() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--cluster",
            "c1",
            "--catalog-url",
            "https://git.example.com/catalog.git",
            "--revision",
            "v1",
        ])
        .assert()
        .success();
    let yaml = read(&dir.path().join("clusters/c1/cluster.yaml"));
    assert_eq!(
        yaml.matches("https://git.example.com/catalog.git").count(),
        2
    );
    assert_eq!(yaml.matches("revision: \"v1\"").count(), 2);
}

#[test]
fn refuses_to_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let init = |url: &str, force: bool| {
        let mut cmd = trlctl();
        cmd.current_dir(dir.path())
            .args(["init", "--cluster", "c1", "--catalog-url", url]);
        if force {
            cmd.arg("--force");
        }
        cmd.assert()
    };
    init("https://git.example.com/a.git", false).success();
    init("https://git.example.com/b.git", false)
        .code(2)
        .stderr(contains("already exists; pass --force"));
    let file = dir.path().join("clusters/c1/cluster.yaml");
    assert!(read(&file).contains("a.git"));
    init("https://git.example.com/b.git", true).success();
    assert!(read(&file).contains("b.git"));
}

#[test]
fn force_comes_from_the_environment_too() {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        trlctl()
            .current_dir(dir.path())
            .env("TRLCTL_FORCE", "true")
            .args([
                "init",
                "--cluster",
                "c1",
                "--catalog-url",
                "https://git.example.com/a.git",
            ])
            .assert()
            .success();
    }
}

#[test]
fn needs_a_url_when_not_interactive() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args(["init", "--cluster", "c1"])
        .assert()
        .code(2)
        .stderr(contains(
            "--catalog-url is required when stdin is not a terminal",
        ));
    assert!(!dir.path().join("apps").exists());
}

#[test]
fn needs_a_cluster_when_not_interactive() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args(["init", "--catalog-url", "https://git.example.com/a.git"])
        .assert()
        .code(2)
        .stderr(contains("--cluster is required"));
}

#[test]
fn rejects_an_invalid_cluster_name() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--cluster",
            "Prod_EU",
            "--catalog-url",
            "https://git.example.com/a.git",
        ])
        .assert()
        .code(2)
        .stderr(contains("not a valid cluster name"));
}

#[test]
fn rejects_a_url_that_is_not_git() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--cluster",
            "c1",
            "--catalog-url",
            "ghcr.io/org/charts",
        ])
        .assert()
        .code(2)
        .stderr(contains("is not a Git repository URL"));
}

#[test]
fn catalog_only_creates_apps_only() {
    let dir = tempfile::tempdir().unwrap();
    trlctl()
        .current_dir(dir.path())
        .args(["init", "--catalog-only"])
        .assert()
        .success();
    assert!(dir.path().join("apps/.gitkeep").is_file());
    assert!(!dir.path().join("envs").exists());
    assert!(!dir.path().join("clusters").exists());
}

#[test]
fn platform_only_uses_origin_for_the_platform() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "ssh://git@git.example.com:2222/org/platform.git",
        ],
    );
    trlctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--platform-only",
            "--cluster",
            "c1",
            "--catalog-url",
            "https://github.com/org/catalog.git",
        ])
        .assert()
        .success();
    assert!(!dir.path().join("apps").exists());
    let yaml = read(&dir.path().join("clusters/c1/cluster.yaml"));
    assert!(yaml.contains("url: \"https://github.com/org/catalog.git\""));
    assert!(yaml.contains("url: \"https://git.example.com/org/platform.git\""));
}

#[test]
fn leaves_non_empty_directories_alone() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("apps/existing")).unwrap();
    trlctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--cluster",
            "c1",
            "--catalog-url",
            "https://git.example.com/a.git",
        ])
        .assert()
        .success();
    assert!(!dir.path().join("apps/.gitkeep").exists());
}
