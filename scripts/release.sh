#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/release.sh [--dry-run] <version>

Cut the trlctl release <version>: set it in Cargo.toml and Cargo.lock, commit,
create the annotated tag v<version>, then ask before pushing both. Pushing the
tag starts the release workflow, which publishes the release.

<version> is semver, without the leading v. A pre-release version (with a
`-`) is published to crates.io only.

Options:
  --dry-run   Print what would change, then stop. Changes nothing.
  -h, --help  Show this help.

Examples:
  scripts/release.sh 0.2.0
  scripts/release.sh --dry-run 0.2.0
  scripts/release.sh 0.3.0-rc.1
EOF
}

die() {
  echo "release: $*" >&2
  exit 1
}

dry_run=false
version=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --dry-run) dry_run=true ;;
    -*) die "unknown option $1 (see --help)" ;;
    *)
      [ -z "$version" ] || die "one version only (see --help)"
      version="$1"
      ;;
  esac
  shift
done

[ -n "$version" ] || die "missing the version (see --help)"
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$ ]] \
  || die "${version} is not a semver version such as 1.2.3 or 1.2.3-rc.1"

cd "$(git rev-parse --show-toplevel)"

command -v cargo-set-version > /dev/null \
  || die "cargo set-version is missing: cargo install cargo-edit --locked"

tag="v${version}"
branch="main"

[ -z "$(git status --porcelain)" ] || die "the working tree is not clean: commit or stash first"
[ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] || die "releases are cut from ${branch}: git switch ${branch}"
git fetch --quiet --tags origin "$branch"
[ "$(git rev-list --count "HEAD..origin/${branch}")" -eq 0 ] \
  || die "${branch} is behind origin/${branch}: git pull --rebase"
! git rev-parse --quiet --verify "refs/tags/${tag}" > /dev/null || die "tag ${tag} already exists"

current="$(cargo pkgid | sed 's/.*[#@]//')"
[ "$current" != "$version" ] || die "Cargo.toml is already at ${version}"

subject="release ${version}"
echo "version: ${current} -> ${version}"
echo "commit:  ${subject}"
echo "tag:     ${tag}"

if [ "$dry_run" = true ]; then
  exit 0
fi

cargo set-version "$version"
git add Cargo.toml Cargo.lock
git commit --quiet --message "$subject"
git tag --annotate "$tag" --message "trlctl ${version}"

push=(git push --atomic origin "$branch" "$tag")
if [ ! -t 0 ]; then
  echo "No terminal: not pushing. Publish with: ${push[*]}"
  exit 0
fi
printf 'push %s and %s to origin? this publishes the release. [y/N] ' "$branch" "$tag"
read -r reply
case "$reply" in
  y | Y | yes) "${push[@]}" ;;
  *) echo "Not pushed. Publish with: ${push[*]}" ;;
esac
