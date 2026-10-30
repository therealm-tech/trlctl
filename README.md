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
  fault and the section of the specification it breaks.

`trlctl` works offline, on local checkouts. It does not render the layout into
Argo CD objects, talk to Argo CD or a cluster, or fetch the repositories a
`cluster.yaml` names.

## Getting started

### Prerequisites

- Rust and Cargo, installed with [rustup](https://rustup.rs);
- Git, which `trlctl init` uses to read the repository's `origin` remote.

### Installation

```sh
cargo install --locked --git https://github.com/therealm-tech/trlctl
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
section of the specification it breaks:

```text
clusters/nonprod-1/dev/whoami/default/app.yaml: §8.2: effective configuration: `revision` must not be set when `url` is not set
```

`trlctl` exits with:

| Status | Meaning |
| --- | --- |
| `0` | Success; for `validate`, the layout follows the specification |
| `1` | `validate` found at least one violation |
| `2` | Usage error, unreadable file, or `init` refused to write |

When the repo platform is a Git checkout, `validate` also checks that
`repo.platform` in each `cluster.yaml` designates that checkout: its `origin`
remote and the layout root's path inside it.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
