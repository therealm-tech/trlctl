<p align="center">
  <a href="https://therealm.tech"><img src="docs/assets/therealm-mark.svg" alt="THEREALM logo" width="120"></a>
</p>

# trlctl

`trlctl` scaffolds and validates Git repositories that follow the therealm
Argo CD layout.

Built and maintained by [THEREALM](https://therealm.tech).

## Description

The [therealm Argo CD layout](https://github.com/therealm-tech/layout-spec)
specifies where the files describing Argo CD deployments live: a catalogue of
apps (`apps/`), environment overrides (`envs/`) and the instances deployed on
each cluster (`clusters/`). `trlctl` implements the specification in
[`SPEC.md`](https://github.com/therealm-tech/layout-spec/blob/main/SPEC.md):

- `trlctl init` creates the skeleton of a layout: `apps/`, `envs/` and a
  cluster's `clusters/<cluster>/cluster.yaml`;
- `trlctl validate` checks a layout against every rule of the specification
  that can be checked on files, and reports each violation with the file at
  fault and the section of the specification it breaks. It also warns about
  what the specification allows but Argo CD would fail on.

`trlctl` works offline, on local checkouts. It does not render the layout into
Argo CD objects, talk to Argo CD or a cluster, or fetch the repositories a
`cluster.yaml` names.

## Getting started

### Prerequisites

- macOS or Linux, on arm64 or x86-64;
- Git, which `trlctl init` uses to read the repository's `origin` remote.

### Installation

**Homebrew**, on macOS and on Linux:

```sh
brew install therealm-tech/tap/trlctl
```

**Cargo**, with Rust installed through [rustup](https://rustup.rs):

```sh
cargo install --locked trlctl
```

**Prebuilt archives.** Each
[GitHub release](https://github.com/therealm-tech/trlctl/releases) carries a
`trlctl-<version>-<target>.tar.gz` archive per target, holding the `trlctl`
binary, the licence and this README, next to its `.sha256` checksum:

| Target | Platform |
| --- | --- |
| `aarch64-apple-darwin` | macOS, Apple silicon |
| `x86_64-apple-darwin` | macOS, Intel |
| `aarch64-unknown-linux-gnu` | Linux, arm64, glibc |
| `x86_64-unknown-linux-gnu` | Linux, x86-64, glibc |

Download the latest archive with the [GitHub CLI](https://cli.github.com),
here for Linux on x86-64:

```sh
gh release download --repo therealm-tech/trlctl --pattern 'trlctl-*-x86_64-unknown-linux-gnu.tar.gz'
```

Verify its build provenance attestation, which proves the archive was built
from this repository by its release workflow:

```sh
gh attestation verify trlctl-*-x86_64-unknown-linux-gnu.tar.gz --repo therealm-tech/trlctl
```

Extract it, then move `trlctl` from the extracted directory to a directory on
your `PATH`:

```sh
tar -xzf trlctl-*-x86_64-unknown-linux-gnu.tar.gz
```

### Configuration

Every option is also read from an environment variable; a command-line flag
takes precedence over the variable.

| Variable | Flag | Default | Description |
| --- | --- | --- | --- |
| `TRLCTL_DIR` | `init --dir` | `.` | Directory to initialise |
| `TRLCTL_CLUSTER` | `init --cluster` | prompted | Name of the cluster to create |
| `TRLCTL_CATALOG_URL` | `init --catalog-url` | `origin` remote, else prompted | URL of the repository holding the repo catalogue |
| `TRLCTL_PLATFORM_URL` | `init --platform-url` | the catalogue URL | URL of the repository holding the repo platform |
| `TRLCTL_REVISION` | `init --revision` | `main` | Revision both repositories are read at |
| `TRLCTL_CATALOG_ONLY` | `init --catalog-only` | `false` | Create the repo catalogue only (`apps/`) |
| `TRLCTL_PLATFORM_ONLY` | `init --platform-only` | `false` | Create the repo platform only (`envs/`, `clusters/`) |
| `TRLCTL_FORCE` | `init --force` | `false` | Overwrite an existing `cluster.yaml` |
| `TRLCTL_CATALOG` | `validate --catalog` | `.` | Layout root of the repo catalogue |
| `TRLCTL_PLATFORM` | `validate --platform` | `.` | Layout root of the repo platform |
| `TRLCTL_OUTPUT` | `validate --output` | `human` | Format of the findings: `human` or `json` |
| `TRLCTL_LOG_FILTER` | `--log-filter` | `warn` | Filter of the diagnostics on stderr, e.g. `trlctl=debug` |
| `TRLCTL_LOG_FORMAT` | `--log-format` | `text` | Format of the diagnostics: `text` or `json` |

### Usage

Initialise a layout in the current Git repository, for the cluster
`prod-eu-1`:

```sh
trlctl init --cluster prod-eu-1
```

The repository URL comes from the `origin` remote, converted to `https://`
(`git@github.com:org/gitops.git` becomes `https://github.com/org/gitops.git`).
Without a remote and without `--catalog-url`, `trlctl` asks for it when run
in a terminal, and fails otherwise. An existing `cluster.yaml` is never
overwritten without `--force`.

When the repo catalogue and the repo platform live in separate repositories,
initialise each one on its own:

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
clusters/nonprod-1/dev/whoami/default/app.yaml: §8.2: effective configuration: `revision` must not be set when `url` is not set
clusters/nonprod-1/dev/whoami/default/app.yaml: warning: `apps/whoami/charts/whoami` does not exist in the repo catalogue: Argo CD will fail to render this source
```

In JSON, each finding carries `file`, `severity` (`error` or `warning`),
`message`, and `section` when it has one.

`trlctl` exits with:

| Status | Meaning |
| --- | --- |
| `0` | Success; for `validate`, the layout follows the specification, warnings or not |
| `1` | `validate` found at least one violation |
| `2` | Usage error, unreadable file, or `init` refused to write |

When the repo platform is a Git checkout, `validate` also checks that
`repo.platform` in each `cluster.yaml` designates that checkout. The layout
root's path inside it must match; an `origin` remote that designates another
repository is only a warning, since a fork or a mirror has its own. URLs are
compared once normalised, so `git@host:org/repo.git` and
`https://host/org/repo` designate the same repository.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
