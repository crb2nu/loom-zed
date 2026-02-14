# Releasing loom-zed

This repo is a Zed extension published via the Zed extensions registry (`zed-industries/extensions`).

## Preflight

1. Verify versions are aligned:

```bash
bash scripts/check_version_alignment.sh
```

2. Run checks:

```bash
make check
```

3. Build the extension WASM locally (optional, but recommended):

```bash
make wasm
```

## Version Bump

Update all of:

- `Cargo.toml` (`[package].version`)
- `extension.toml` (`version`)
- `CHANGELOG.md` (add/update a `## [X.Y.Z] - YYYY-MM-DD` section)

Then re-run:

```bash
bash scripts/check_version_alignment.sh
make check
```

## License (Required For Publishing)

Zed requires an accepted open source license and a `LICENSE` file in the extension repository.

Before submitting to the registry, add a `LICENSE` file using one of Zed's accepted licenses
(see Zed docs for the current list).

## Tag + GitHub Release Artifact

This repo has a GitHub Actions workflow (`.github/workflows/release.yml`) that triggers on tags like `vX.Y.Z`.
It will build `loom_zed.wasm` for `wasm32-wasip2` and attach it to a GitHub Release.

```bash
git tag vX.Y.Z
git push github vX.Y.Z
```

If you also want tags in GitLab:

```bash
git push origin vX.Y.Z
```

## Publish / Update In Zed Registry

Zed's extension registry is `https://github.com/zed-industries/extensions`.

High-level flow:

1. Fork the registry repo.
2. Add this repo as a git submodule under `extensions/` (new extensions only).
3. Add/update the registry entry in `extensions.toml`:

```toml
[loom]
submodule = "extensions/loom"
version = "X.Y.Z"
```

4. Update the submodule pointer to the commit you want released (typically the `vX.Y.Z` tag).
5. Open a PR to the registry repo.

Notes:

- Updating an existing extension usually means:
  - bumping `version = "X.Y.Z"` in `extensions.toml`
  - advancing the submodule pointer to the new commit/tag

## Troubleshooting

- If Zed doesn't show prompt recipes/resources, verify the wrapper is enabled in settings:
  - `settings.mcp.wrapper.enabled = true`
  - `settings.mcp.prompts.enabled = true`
  - `settings.mcp.resources.enabled = true`

