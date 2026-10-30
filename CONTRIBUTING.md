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
- **ShellCheck**, which the `shellcheck` hook runs on the scripts:

  **macOS**

  ```sh
  brew install shellcheck
  ```

  **Linux**

  ```sh
  sudo apt install -y shellcheck
  ```

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
| `zizmor` | Security of the workflows and the Dependabot configuration, offline audits | Edit the workflow |
| `cargo-fmt` | Rust formatting | `cargo fmt` |
| `cargo-clippy` | Clippy, pedantic, warnings denied | Edit the code |
| `shellcheck` | Shell scripts in [`scripts/`](scripts/) | Edit the script |
| `no-co-authors` (commit-msg) | No `Co-Authored-By:` or `Generated with` line | Rewrite the message |

A red hook is a real finding: `--no-verify` and `SKIP=` are not the fix. If a
rule is wrong for this repository, change its configuration in the same pull
request and say why. CI runs the same hooks.

## Continuous integration

| Workflow | Triggers on | What it does | Reproduce locally |
| --- | --- | --- | --- |
| [`quality`](.github/workflows/quality.yaml) | every pull request, push to `main` | `pre-commit run --all-files`; `cargo test --locked` | `pre-commit run --all-files`, `cargo test` |
| [`security`](.github/workflows/security.yaml) | every pull request, push to `main`, daily, manual | `trivy fs` on the repository: fails on fixable `HIGH`/`CRITICAL` vulnerabilities, reports every finding to code scanning | `trivy fs --severity HIGH,CRITICAL --ignore-unfixed .` |
| [`build`](.github/workflows/build.yaml) | pull requests and pushes to `main` that touch the code, the toolchain, the packaging or the workflow; manual; called by `release` | builds the four release archives on native runners, renders the Homebrew formula and checks it with `brew style` | `cargo build --release --locked` |
| [`release`](.github/workflows/release.yaml) | tag `v*` | see [Releasing](#releasing) | none: it publishes |

`quality` and `security` are not path-filtered, so both can be required checks
in branch protection; `build` is path-filtered, so it must not be one. These three need no secret;
the code-scanning upload uses the workflow's own token and is skipped on pull
requests from forks.

[Dependabot](.github/dependabot.yaml) proposes Cargo and GitHub Actions
updates weekly, grouping minor and patch updates.

## Submitting a change

- Commit subjects are lowercase and imperative (`add a check for …`); keep
  them short, and explain the *why* in the body when the diff doesn't.
- A pull request needs green CI and the README or this file updated when it
  changes what they describe. Label it with one of the categories of
  [`.github/release.yaml`](.github/release.yaml) so it lands in the right
  section of the release notes.

## Releasing

[`scripts/release.sh`](scripts/release.sh) is the only supported way to cut a
release. It needs `cargo set-version`, from cargo-edit:

```sh
cargo install --locked cargo-edit
```

From a clean `main`, up to date with `origin`:

```sh
scripts/release.sh 0.2.0
```

The script refuses a dirty tree, another branch, a `main` behind `origin` and
an existing tag. It sets the version in `Cargo.toml` and `Cargo.lock`, commits
`release 0.2.0`, creates the annotated tag `v0.2.0`, then asks before pushing
both; answering no prints the push command to run later. `--dry-run` prints
what it would do and changes nothing.

Pushing the tag starts the [`release`](.github/workflows/release.yaml)
workflow:

1. **check the version** fails when `Cargo.toml` disagrees with the tag, so a
   hand-made tag publishes nothing;
2. **build** runs the [`build`](.github/workflows/build.yaml) workflow: the
   four archives and their checksums, and the Homebrew formula rendered from
   them;
3. **GitHub release** attests the build provenance of the archives, then
   creates the release with the archives and checksums. Its notes are
   generated from the pull requests merged since the previous stable tag,
   sorted into the categories of [`.github/release.yaml`](.github/release.yaml)
   by their labels;
4. **Homebrew formula** commits `Formula/trlctl.rb` to
   [`therealm-tech/homebrew-tap`](https://github.com/therealm-tech/homebrew-tap)
   on `main`. It runs only once the release exists, so the formula never
   points at archives that are not published;
5. **crates.io** publishes the crate, in parallel with the last two. A version
   crates.io already has is skipped, so a re-run is harmless.

A pre-release version, such as `0.3.0-rc.1`, goes to crates.io only: it gets
no GitHub release and no Homebrew formula.

### Credentials

The workflow holds no long-lived token:

- **GitHub release** uses the workflow's own `GITHUB_TOKEN`, with
  `contents: write`, and its OIDC token to sign the attestation.
- **Homebrew formula** mints a one-hour installation token of the release
  GitHub App, which is installed on `therealm-tech/homebrew-tap` only, with
  *Contents: read and write*. The token is restricted to that repository and
  to `contents: write`, and the formula commits are authored by the App's bot.
  The App's client ID is the Actions variable `RELEASE_APP_CLIENT_ID`; its
  private key is the Actions secret `RELEASE_APP_PRIVATE_KEY`.
- **crates.io** exchanges the workflow's OIDC token for a 30-minute crates.io
  token through
  [trusted publishing](https://crates.io/docs/trusted-publishing). The trusted
  publisher on crates.io is the repository `therealm-tech/trlctl`, the
  workflow `release.yaml` and the environment `release`.

Both publishing jobs run in the `release` GitHub environment, whose deployment
rule (*Settings → Environments → release*) accepts the tags matching `v*`
only.

### The first crates.io version

Trusted publishing can only be configured on a crate that exists, so the very
first version of `trlctl` is published by hand, with an API token:

1. Cut the release with `scripts/release.sh` and answer no to the push
   question.
2. Create an API token with the `publish-new` scope at
   <https://crates.io/settings/tokens>, then, from the release commit:

   ```sh
   cargo login
   ```

   ```sh
   cargo publish --locked
   ```

   ```sh
   cargo logout
   ```

3. On <https://crates.io/crates/trlctl/settings>, under *Trusted Publishing*,
   add a GitHub publisher: owner `therealm-tech`, repository `trlctl`,
   workflow filename `release.yaml`, environment `release`. Tick *Require
   trusted publishing for all new versions*, and revoke the API token.
4. Push with the command the script printed. The **crates.io** job finds the
   version already published and skips it; the rest of the release runs as
   usual.

### The Homebrew formula

The formula is rendered from
[`packaging/homebrew/trlctl.rb.in`](packaging/homebrew/trlctl.rb.in) by
[`scripts/homebrew-formula.sh`](scripts/homebrew-formula.sh), which reads the
checksum of each archive from a directory of `.sha256` files:

```sh
scripts/homebrew-formula.sh 0.2.0 dist
```

The `build` workflow renders it and runs `brew style` on it in every pull
request that changes the template or the script, and uploads the result as the
`homebrew-formula` artifact of the run.
