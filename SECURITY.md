# Security policy

## Reporting a vulnerability

Report a vulnerability privately, through
[GitHub's private vulnerability reporting](https://github.com/therealm-tech/trlctl/security/advisories/new).
The report opens a draft advisory that only the maintainers and you can see.

Never report a vulnerability in a public issue, pull request or discussion: it
would be disclosed before it is fixed.

A useful report holds:

- the affected version of `trlctl` (`trlctl --version`) and how it was
  installed;
- the steps to reproduce, with the layout files involved when they matter;
- the impact as you see it.

## Supported versions

Fixes ship in a new release; only the latest release is supported.

## Scope

In scope:

- `trlctl` itself, reading a layout it does not trust: a crafted
  `cluster.yaml`, `app.yaml` or `values.yaml`, or a crafted Git remote, that
  makes `validate` or `init` read or write outside the directories it is
  given, run a command, or crash in a way that hides a finding;
- what `trlctl` publishes: the release archives, their attestations, the
  crate on crates.io and the Homebrew formula.

Out of scope:

- a vulnerability in a dependency that is already public upstream: report it
  there; the dependency updates bring the fix in;
- a finding that needs an already-compromised host or account;
- scanner output with no path to exploitation.

## What happens next

Reports are handled on a best-effort basis:

1. The report is acknowledged.
2. It is assessed, with you, in the draft advisory.
3. The fix is prepared in the advisory's private fork.
4. A release ships the fix.
5. The advisory is published, with a CVE requested through GitHub when the
   impact warrants one, and you are credited unless you prefer otherwise.
