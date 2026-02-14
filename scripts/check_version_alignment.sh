#!/usr/bin/env bash
set -euo pipefail

cargo_ver="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*\"([^\"]+)\".*/\1/p' Cargo.toml | head -n 1)"
ext_ver="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*\"([^\"]+)\".*/\1/p' extension.toml | head -n 1)"

if [[ -z "${cargo_ver}" || -z "${ext_ver}" ]]; then
  echo "failed to detect versions from Cargo.toml/extension.toml" >&2
  exit 1
fi

if [[ "${cargo_ver}" != "${ext_ver}" ]]; then
  echo "version mismatch:" >&2
  echo "  Cargo.toml:     ${cargo_ver}" >&2
  echo "  extension.toml: ${ext_ver}" >&2
  exit 1
fi

# Optional safety check: changelog should mention the version.
if ! grep -F "## [${cargo_ver}]" CHANGELOG.md >/dev/null; then
  echo "warning: CHANGELOG.md missing heading for ${cargo_ver} (## [${cargo_ver}])" >&2
fi

echo "version ok: ${cargo_ver}"
