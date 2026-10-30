#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/homebrew-formula.sh <version> <dir>

Print the Homebrew formula for trlctl <version> on stdout. <dir> holds
trlctl-<version>-<target>.tar.gz.sha256 for each released target.

Example:
  scripts/homebrew-formula.sh 0.1.0 dist > Formula/trlctl.rb
EOF
}

die() {
  echo "homebrew-formula: $*" >&2
  exit 1
}

case "${1:-}" in
  -h | --help)
    usage
    exit 0
    ;;
esac
[ $# -eq 2 ] || { usage >&2; exit 1; }

version="$1"
dir="$2"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "${version} is not a semver version"
template="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)/packaging/homebrew/trlctl.rb.in"

args=(-e "s/@VERSION@/${version}/g")
for target in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
  file="${dir}/trlctl-${version}-${target}.tar.gz.sha256"
  [ -f "$file" ] || die "missing ${file}"
  read -r sha256 _ < "$file"
  [[ "$sha256" =~ ^[0-9a-f]{64}$ ]] || die "${file} holds no SHA-256 digest"
  placeholder="SHA256_$(tr 'a-z-' 'A-Z_' <<< "$target")"
  args+=(-e "s/@${placeholder}@/${sha256}/g")
done

sed "${args[@]}" "$template"
