<p align="center">
  <a href="https://therealm.tech"><img src="docs/assets/therealm-mark.svg" alt="THEREALM logo" width="120"></a>
</p>

# trlctl

[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/therealm-tech/trlctl/badge)](https://scorecard.dev/viewer/?uri=github.com/therealm-tech/trlctl)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/17223/badge)](https://www.bestpractices.dev/projects/17223)

`trlctl` scaffolds and validates Git repositories that follow the therealm
Argo CD layout.

Built and maintained by [THEREALM](https://therealm.tech).

## Description

`trlctl` implements the
[therealm Argo CD layout specification](https://github.com/therealm-tech/layout-spec/blob/main/SPEC.md),
which says where the files describing Argo CD deployments live.

- `trlctl init` creates a layout's skeleton, with a cluster's `cluster.yaml`;
- `trlctl validate` checks a layout against every rule of the specification
  that can be checked on files, and warns about what Argo CD would fail on.

It works offline, on local checkouts: it does not render Argo CD objects, talk
to a cluster, or fetch the repositories a `cluster.yaml` names. See
[ARCHITECTURE.md](ARCHITECTURE.md) for the design and the reasoning behind it.

## The need

A platform team reviews pull requests on its GitOps repository, where a file's
place decides what gets deployed, and where. Without a checker, the rules live
in reviewers' heads. A `values.yaml` one level off, an `app.yaml` setting
`revision` without `url`, or a source path pointing at a chart that does not
exist all look fine in a diff. They surface after the merge: as an app Argo CD
fails to render, or as an override that silently never applies. Starting a new
cluster means copying another one's directory and editing its repository URLs
by hand.

`trlctl validate` runs the specification's checks on the files, before the
merge. Each finding names the file and the section it breaks, and the command
exits non-zero, so it fits a pull-request pipeline:

```sh
trlctl validate --output json
```

`trlctl init` creates a cluster's skeleton with its URLs taken from the
repository's `origin` remote.

## Getting started

### Prerequisites

- macOS or Linux, on arm64 or x86-64;
- Git, optional: `trlctl init` reads the repository's `origin` remote with
  it, and `trlctl validate` checks `repo.platform` against the checkout.

### Installation

**Homebrew**, on macOS and on Linux:

```sh
brew install therealm-tech/tap/trlctl
```

**Cargo**, with Rust 1.88 or later installed through [rustup](https://rustup.rs):

```sh
cargo install --locked trlctl
```

**Prebuilt archives.** Each
[GitHub release](https://github.com/therealm-tech/trlctl/releases) carries a
`trlctl-<version>-<target>.tar.gz` archive per target, holding the `trlctl`
binary, the licence and this README, next to its `.sha256` checksum. The
release also carries its SBOM, `trlctl-<version>.cdx.json` (CycloneDX), and
`trlctl-<version>.sigstore.json`, the Sigstore bundle of its provenance
attestation. The Linux binaries are static (musl), so they run on any
distribution:

| Target | Platform |
| --- | --- |
| `aarch64-apple-darwin` | macOS, Apple silicon |
| `x86_64-apple-darwin` | macOS, Intel |
| `aarch64-unknown-linux-musl` | Linux, arm64 |
| `x86_64-unknown-linux-musl` | Linux, x86-64 |

Set `TARGET` to your platform's target from the table, for instance:

**macOS**, Apple silicon:

```sh
TARGET=aarch64-apple-darwin
```

**Linux**, x86-64:

```sh
TARGET=x86_64-unknown-linux-musl
```

Download the latest archive with the [GitHub CLI](https://cli.github.com):

```sh
gh release download --repo therealm-tech/trlctl --pattern "trlctl-*-${TARGET}.tar.gz"
```

Verify its build provenance attestation, which proves the archive was built
from this repository by its release workflow:

```sh
gh attestation verify trlctl-*-"${TARGET}".tar.gz --repo therealm-tech/trlctl
```

Without access to GitHub's attestation API, check it against the bundle
attached to the release instead; that form cannot tell a renamed repository
from the original:

```sh
gh release download --repo therealm-tech/trlctl --pattern 'trlctl-*.sigstore.json'
```

```sh
gh attestation verify trlctl-*-"${TARGET}".tar.gz --bundle trlctl-*.sigstore.json --repo therealm-tech/trlctl
```

Extract it into `trlctl-<version>-<target>/`:

```sh
tar -xzf trlctl-*-"${TARGET}".tar.gz
```

Install `trlctl` into a directory on your `PATH`, here `~/.local/bin`:

```sh
mkdir -p ~/.local/bin
```

```sh
install -m 0755 trlctl-*-"${TARGET}"/trlctl ~/.local/bin/
```

### Configuration

Every option is also read from an environment variable; a command-line flag
takes precedence over the variable.

| Variable | Flag | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `TRLCTL_DIR` | `init --dir` | no | `.` | Directory to initialise |
| `TRLCTL_CLUSTER` | `init --cluster` | when stdin is not a terminal, except with `--catalog-only` | prompted | Name of the cluster to create |
| `TRLCTL_CATALOG_URL` | `init --catalog-url` | no | `origin` remote, else prompted; with `--platform-only`, prompted | URL of the repository holding the repo catalogue |
| `TRLCTL_PLATFORM_URL` | `init --platform-url` | no | the catalogue URL; with `--platform-only`, `origin` remote, else prompted | URL of the repository holding the repo platform |
| `TRLCTL_REVISION` | `init --revision` | no | `main` | Revision both repositories are read at |
| `TRLCTL_CATALOG_ONLY` | `init --catalog-only` | no | `false` | Create the repo catalogue only (`apps/`) |
| `TRLCTL_PLATFORM_ONLY` | `init --platform-only` | no | `false` | Create the repo platform only (`envs/`, `clusters/`) |
| `TRLCTL_FORCE` | `init --force` | no | `false` | Overwrite an existing `cluster.yaml` |
| `TRLCTL_CATALOG` | `validate --catalog` | no | `.` | Layout root of the repo catalogue |
| `TRLCTL_PLATFORM` | `validate --platform` | no | `.` | Layout root of the repo platform |
| `TRLCTL_OUTPUT` | `validate --output` | no | `human` | Format of the findings: `human` or `json` |
| `TRLCTL_LOG_FILTER` | `--log-filter` | no | `warn` | Filter of the diagnostics on stderr, e.g. `trlctl=debug` |
| `TRLCTL_LOG_FORMAT` | `--log-format` | no | `text` | Format of the diagnostics: `text` or `json` |

A *prompted* value is asked for when stdin is a terminal; otherwise, without
its flag or variable, `trlctl init` fails.

### Usage

Initialise a layout in the current Git repository, for the cluster
`prod-eu-1`:

```sh
trlctl init --cluster prod-eu-1
```

The repository URL comes from the `origin` remote, converted to `https://`
(`git@github.com:org/gitops.git` becomes `https://github.com/org/gitops.git`),
and is prompted for without one. An existing `cluster.yaml` is never
overwritten without `--force`.

When the repo catalogue and the repo platform live in separate repositories,
initialise each one on its own. With `--platform-only`, `origin` gives the
platform URL only: the catalogue URL comes from `--catalog-url` or the prompt.

```sh
trlctl init --catalog-only
```

```sh
trlctl init --platform-only --cluster prod-eu-1 --catalog-url https://github.com/org/catalogue.git
```

Validate a repository holding both roles:

```sh
trlctl validate
```

Validate a repo catalogue and a repo platform checked out side by side, with
JSON output for CI:

```sh
trlctl validate --catalog ../catalogue --platform . --output json
```

Each finding names the file at fault, relative to its layout root, and the
section of the specification it breaks. A warning is marked as such, and names
a section only when it relates to one:

```text
clusters/nonprod-1/envs/dev/whoami/default/app.yaml: §8.2: effective configuration: `revision` must not be set when `url` is not set
clusters/nonprod-1/envs/dev/whoami/default/app.yaml: warning: `apps/whoami/charts/whoami` does not exist in the repo catalogue: Argo CD will fail to render this source
```

In JSON, each finding carries `file`, `severity` (`error` or `warning`),
`message`, and `section` when it has one.

`trlctl` exits with:

| Status | Meaning |
| --- | --- |
| `0` | Success; for `validate`, the layout follows the specification, warnings or not |
| `1` | `validate` found at least one violation |
| `2` | Usage error, unreadable file or directory, or `init` refused to write |

What `validate` reads, how it builds each instance's effective
configuration, and which checks depend on Git are described in
[ARCHITECTURE.md](ARCHITECTURE.md#validate).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
