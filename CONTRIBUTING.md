# Contributing

Issues and pull requests go to
[therealm-tech/trlctl](https://github.com/therealm-tech/trlctl). The short
version: install the tools with mise, keep `task test` and `task lint` green,
and update the docs in the same change.

## Development setup

Install [mise](https://mise.jdx.dev/installing-mise.html), which installs
every other tool at the version pinned in [`mise.toml`](mise.toml): the Rust
toolchain, pre-commit, Task, and what the hooks and the release script call.
You also need **Git**, used by `trlctl init` and by the tests.

Then, from a fresh clone:

```sh
mise trust
```

```sh
mise install
```

```sh
task setup
```

`task setup` installs the git hooks. Check the setup with:

```sh
task test
```

A green run means the setup works. `task --list` shows the other tasks; see
[README.md](README.md#getting-started) for running `trlctl` itself.

## Running the tests

The whole suite:

```sh
task test
```

A single test, with the arguments after `--` passed to `cargo test`:

```sh
task test -- --test validate rejects_a_revision_without_url
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
task lint
```

```sh
pre-commit run cargo-clippy --all-files
```

Most hooks fix the file or name the line at fault. Those worth a word:

| Hook | Checks | Fix |
| --- | --- | --- |
| `cargo-clippy` | Clippy with `pedantic`, warnings denied | Edit the code; a lint that must be allowed gets a scoped `#[allow]` with its reason |
| `mise-fmt` | Formatting of [`mise.toml`](mise.toml) | Rewrites the file; re-run |
| `zizmor` | Security of the workflows, offline audits | Edit the workflow |
| `renovate-config-validator` | [`renovate.json`](renovate.json), migrated keys included | `npx --package renovate renovate-config-validator --strict --migrate` |
| `no-co-authors` (commit-msg) | No `Co-Authored-By:` or `Generated with` line; `task lint` skips it | Rewrite the message |

A red hook is a real finding: `--no-verify` and `SKIP=` are not the fix. If a
rule is wrong for this repository, change its configuration in the same pull
request and say why. CI runs the same hooks.

## Continuous integration

| Workflow | Purpose | Reproduce locally |
| --- | --- | --- |
| [`quality`](.github/workflows/quality.yaml) | Quality gate: the pre-commit hooks and the tests | `task lint`, `task test` |
| [`security`](.github/workflows/security.yaml) | `trivy fs` on the lockfiles: fails on fixable `HIGH`/`CRITICAL` vulnerabilities, reports every finding to code scanning | `trivy fs --severity HIGH,CRITICAL --ignore-unfixed .` |
| [`build`](.github/workflows/build.yaml) | The four release archives on native runners, and the Homebrew formula checked with `brew style` | `task build` |
| [`release`](.github/workflows/release.yaml) | Publishes a release, see [Releasing](#releasing) | none: it publishes |
| [`scorecard`](.github/workflows/scorecard.yaml) | Grades the repository's supply-chain practices with OpenSSF Scorecard, publishes the score behind the README badge and reports the findings to code scanning | none: it grades the repository on GitHub |

The jobs of `quality` and `security` are required to merge into `main`.
`security` also runs daily, so it can go red on `main` when a new CVE lands
with no change of yours. `build` only runs when the code, the toolchain, the
packaging or the workflow changes, which is why it is not required. None of
these needs a secret; the code-scanning upload uses the workflow's own token
and is skipped on pull requests from forks.

[Renovate](renovate.json) proposes the dependency updates: crates, action
pins, hook revisions and dev tools, with a weekly lockfile
refresh and security fixes as soon as an advisory lands. It runs weekly from
the [`renovate`](.github/workflows/renovate.yaml) workflow, which also takes a
manual run with a dry-run switch; it writes with the `RENOVATE_TOKEN` secret,
a token whose pull requests trigger CI, which `GITHUB_TOKEN`'s do not. Its
pull requests are read and merged by hand, never automerged, and the
[Dependency dashboard](https://github.com/therealm-tech/trlctl/issues?q=is%3Aissue+%22Dependency+dashboard%22)
issue lists what it holds back.

## Submitting a change

- Commit subjects are lowercase and imperative (`add a check for …`); keep
  them short, and explain the *why* in the body when the diff doesn't.
- A pull request needs green CI and the README or this file updated when it
  changes what they describe. Label it with one of the categories of
  [`.github/release.yaml`](.github/release.yaml) so it lands in the right
  section of the release notes.

## Releasing

[`scripts/release.sh`](scripts/release.sh), run through `task release`, is the
only supported way to cut a release. From a clean `main`, up to date with
`origin`:

```sh
task release -- 0.2.0
```

The script refuses a dirty tree, another branch, a `main` behind `origin` and
an existing tag. It sets the version in `Cargo.toml` and `Cargo.lock`, commits
`release 0.2.0`, creates the annotated tag `v0.2.0`, then asks before pushing
both; answering no prints the push command to run later. When `Cargo.toml`
already holds the version, it tags the current commit without a release
commit. `--dry-run` prints what it would do and changes nothing.

Pushing the tag starts the [`release`](.github/workflows/release.yaml)
workflow:

1. **check the version** fails when `Cargo.toml` disagrees with the tag, so a
   hand-made tag publishes nothing;
2. **build** runs the [`build`](.github/workflows/build.yaml) workflow: the
   four archives and their checksums, and the Homebrew formula rendered from
   them;
3. **GitHub release** generates a CycloneDX SBOM from the lockfile, attests
   the build provenance of the archives and checksums and the SBOM against
   them, then creates the release with the archives, the checksums, the SBOM
   and the Sigstore bundle of the provenance attestation. Its notes are
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

1. Cut the release with `task release` and answer no to the push
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
