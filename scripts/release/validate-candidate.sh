#!/usr/bin/env bash
# Validate the consolidated Linux and Windows release bundle before it is published.
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 vX.Y.Z EXPECTED_SHA CANDIDATE_DIR" >&2
  exit 2
fi

tag="$1"
expected_sha="$2"
candidate_dir="$3"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

[[ "$expected_sha" =~ ^[0-9a-f]{40}$ ]] || {
  echo "EXPECTED_SHA must be a full lowercase 40-hex commit ID" >&2
  exit 1
}
actual_sha="$(git rev-parse HEAD)"
[[ "$actual_sha" == "$expected_sha" ]] || {
  echo "checked-out source is $actual_sha, expected $expected_sha" >&2
  exit 1
}
[[ -z "$(git status --porcelain)" ]] || {
  echo "candidate validation requires a clean worktree" >&2
  exit 1
}
version="$(cargo xtask release version)"
[[ "$tag" == "v$version" ]] || {
  echo "candidate label $tag does not match source version $version" >&2
  exit 1
}
cargo xtask release check
candidate_dir="$(cd "$candidate_dir" && pwd)"
linux_provenance="$candidate_dir/tctidy-linux-provenance.json"
windows_provenance="$candidate_dir/tctidy-windows-provenance.json"
cargo xtask release verify-provenance \
  "$tag" "$expected_sha" "$linux_provenance" "$windows_provenance"

provenance_dir="$(mktemp -d)"
extracted="$(mktemp -d)"
cleanup() {
  status=$?
  if (( status != 0 )); then
    # Keep the evidence available when a candidate fails validation.
    for provenance in "$provenance_dir"/*.json; do
      [[ ! -f "$provenance" ]] || mv "$provenance" "$candidate_dir/"
    done
  fi
  rm -rf "$extracted" "$provenance_dir"
}
trap cleanup EXIT
mv "$linux_provenance" "$provenance_dir/"
mv "$windows_provenance" "$provenance_dir/"
cargo xtask release check-assets "$candidate_dir" --tag "$tag"

tar -C "$extracted" -xzf \
  "$candidate_dir/tctidy-$tag-x86_64-unknown-linux-gnu.tar.gz"
[[ "$("$extracted/tctidy" --version)" == "tctidy $version" ]] || {
  echo "packaged Linux CLI does not report tctidy $version" >&2
  exit 1
}

echo "Candidate bundle is complete and valid for $tag (source $actual_sha)."
