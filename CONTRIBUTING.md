# Contributing

Issues and pull requests go to
[therealm-tech/trlctl](https://github.com/therealm-tech/trlctl). The short
version: install the toolchain and the hooks, keep `cargo test` and
`pre-commit run --all-files` green, and update the docs in the same change.

## Development setup

- **Rust**, through [rustup](https://rustup.rs). The toolchain is pinned in
  [`rust-toolchain.toml`](rust-toolchain.toml); rustup installs that version,
  with `rustfmt` and `clippy`, the first time `cargo` runs in the repository.
- **Git**, used by `trlctl init` and by the tests.
- **pre-commit**:

  ```sh
  pipx install pre-commit
  ```

- **Go**, which pre-commit needs to build the `actionlint` hook. Install it
  from <https://go.dev/doc/install>.

Then, from a fresh clone:

```sh
pre-commit install
```

```sh
cargo test
```

A green `cargo test` means the setup works. See
[README.md](README.md#getting-started) for running `trlctl` itself.

## Running the tests

The whole suite:

```sh
cargo test
```

A single test:

```sh
cargo test --test validate rejects_a_revision_without_url
```

- **Unit tests** live next to the code they cover, in `src/`.
- **`tests/validate.rs`** runs `validate` on a temporary copy of
  [`tests/fixtures/example/`](tests/fixtures/example/), the worked example of
  the specification. The unchanged copy must produce no finding; every other
  test changes one file and asserts the finding it causes, with its section.
- **`tests/init.rs`** runs the `trlctl` binary in temporary directories,
  including temporary Git repositories with an `origin` remote.

No test needs a network, a container runtime or a cluster.

A new rule comes with a test that breaks it in the fixture; a fix comes with
a test that reproduces the bug. When the specification's worked example
changes, copy it into `tests/fixtures/example/` again.

## Pre-commit hooks

The hooks in [`.pre-commit-config.yaml`](.pre-commit-config.yaml) must pass
before a change is pushed:

```sh
pre-commit run --all-files
```

```sh
pre-commit run cargo-clippy --all-files
```

| Hook | Checks | Fix |
| --- | --- | --- |
| `trailing-whitespace`, `end-of-file-fixer` | Whitespace | Fixed in place; re-run |
| `check-yaml`, `yamllint` | YAML syntax and style ([`.yamllint.yaml`](.yamllint.yaml)) | Edit the file |
| `check-added-large-files`, `check-merge-conflict`, `detect-private-key` | Accidental commits | Remove the file or marker |
| `actionlint` | GitHub Actions workflows | Edit the workflow |
| `cargo-fmt` | Rust formatting | `cargo fmt` |
| `cargo-clippy` | Clippy, pedantic, warnings denied | Edit the code |
| `no-co-authors` (commit-msg) | No `Co-Authored-By:` or `Generated with` line | Rewrite the message |

A red hook is a real finding: `--no-verify` and `SKIP=` are not the fix. If a
rule is wrong for this repository, change its configuration in the same pull
request and say why. CI runs the same hooks.

## Continuous integration

| Workflow | Triggers on | What it does | Reproduce locally |
| --- | --- | --- | --- |
| [`quality`](.github/workflows/quality.yaml) | every pull request, push to `main` | `pre-commit run --all-files`; `cargo test --locked` | `pre-commit run --all-files`, `cargo test` |
| [`security`](.github/workflows/security.yaml) | every pull request, push to `main`, daily, manual | `trivy fs` on the repository: fails on fixable `HIGH`/`CRITICAL` vulnerabilities, reports every finding to code scanning | `trivy fs --severity HIGH,CRITICAL --ignore-unfixed .` |

Neither workflow is path-filtered, so both can be required checks in branch
protection. They need no secret; the code-scanning
upload uses the workflow's own token and is skipped on pull requests from
forks.

[Dependabot](.github/dependabot.yaml) proposes Cargo and GitHub Actions
updates weekly, grouping minor and patch updates.

## Submitting a change

- Commit subjects are lowercase and imperative (`add a check for …`); keep
  them short, and explain the *why* in the body when the diff doesn't.
- A pull request needs green CI and the README or this file updated when it
  changes what they describe. Label it with one of the categories of
  [`.github/release.yaml`](.github/release.yaml) so it lands in the right
  section of the release notes.
