#!/usr/bin/env bash
# Build the Linux half of a TcTidy candidate entirely from the checked-out SHA.
set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "usage: $0 vX.Y.Z EXPECTED_SHA OUTPUT_DIR THIRD-PARTY-NOTICES.md CARGO-DEPENDENCIES.json" >&2
  exit 2
fi

tag="$1"
expected_sha="$2"
output_dir="$3"
notices="$4"
inventory="$5"
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
version="$(cargo xtask release version)"
[[ "$tag" == "v$version" ]] || {
  echo "candidate label $tag does not match source version $version" >&2
  exit 1
}
cargo xtask release check
[[ -z "$(git status --porcelain)" ]] || {
  echo "candidate builds require a clean worktree" >&2
  exit 1
}
[[ -s "$notices" ]] || {
  echo "third-party notices are missing or empty: $notices" >&2
  exit 1
}
[[ -s "$inventory" ]] || {
  echo "Cargo dependency inventory is missing or empty: $inventory" >&2
  exit 1
}
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
notices="$(cd "$(dirname "$notices")" && pwd)/$(basename "$notices")"
inventory="$(cd "$(dirname "$inventory")" && pwd)/$(basename "$inventory")"
base="tctidy-$tag-x86_64-unknown-linux-gnu"
archive="$output_dir/$base.tar.gz"
checksum="$archive.sha256"
provenance="$output_dir/tctidy-linux-provenance.json"
rm -f "$archive" "$checksum" "$provenance"
stage="$(mktemp -d)"
extracted="$(mktemp -d)"
evidence="$(mktemp -d)"
trap 'rm -rf "$stage" "$extracted" "$evidence"' EXIT

cargo xtask licenses generate \
  --target x86_64-unknown-linux-gnu \
  --inventory "$evidence/CARGO-DEPENDENCIES.json" \
  --notice "$evidence/THIRD-PARTY-NOTICES.md" \
  --metadata-output "$evidence/cargo-metadata.json" \
  --about-output "$evidence/cargo-about.json" \
  --tree-output "$evidence/cargo-tree.txt"
cargo xtask licenses compare \
  --supplied-inventory "$inventory" \
  --supplied-notice "$notices" \
  --generated-inventory "$evidence/CARGO-DEPENDENCIES.json" \
  --generated-notice "$evidence/THIRD-PARTY-NOTICES.md"

cargo build -p tctidy-cli --release --locked
cp target/release/tctidy "$stage/"
cp LICENSE-MIT LICENSE-APACHE README.md CHANGELOG.md "$stage/"
cp "$evidence/THIRD-PARTY-NOTICES.md" "$stage/"
cp "$evidence/CARGO-DEPENDENCIES.json" "$stage/"

# Stable ordering, ownership and timestamps keep repeated builds byte-identical.
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git show -s --format=%ct HEAD)}"
tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 --numeric-owner \
  -C "$stage" -cf - . | gzip -n > "$archive"
(
  cd "$output_dir"
  sha256sum "$(basename "$archive")" > "$(basename "$checksum")"
)

tar -C "$extracted" -xzf "$archive"
cargo xtask licenses notice \
  --payload "$extracted" --notice THIRD-PARTY-NOTICES.md
[[ "$("$extracted/tctidy" --version)" == "tctidy $version" ]] || {
  echo "packaged Linux CLI does not report tctidy $version" >&2
  exit 1
}

printf '{"schema":1,"component":"linux","tag":"%s","version":"%s","source_sha":"%s"}\n' \
  "$tag" "$version" "$actual_sha" > "$provenance"
echo "Linux candidate ready at $output_dir (source $actual_sha)"
