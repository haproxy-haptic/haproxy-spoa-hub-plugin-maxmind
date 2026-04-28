# Releasing

## Prerequisites

- Write access to the repository
- `glab` CLI authenticated, or ability to push tags
- **Generic packages "Reject duplicates" enabled** under
  Settings > Packages and Registries > Generic packages. Without this, a
  re-run of the release pipeline on the same tag silently overwrites the
  bytes uploaded earlier.

## Steps

1. **Update the changelog**: Move items from `## [Unreleased]` to a new version section:

   ```markdown
   ## [Unreleased]

   ## [0.1.0] - 2026-04-15

   ### Added
   - ...
   ```

   Follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) conventions. Use categories: Added, Changed, Deprecated, Removed, Fixed, Security.

2. **Bump the version** in `Cargo.toml`:

   ```toml
   [package]
   version = "0.1.0"
   ```

3. **Commit and merge**: Create a branch, commit the changelog and version bump, merge to main via MR.

4. **Tag the release**: From the merged main:

   ```bash
   git checkout main && git pull
   git tag v0.1.0
   git push origin v0.1.0
   ```

   Tags must match the pattern `v*` (e.g., `v0.1.0`, `v1.0.0-beta.1`).

5. **CI handles the rest**: The release pipeline automatically:
   - Cross-compiles the plugin `.so` for amd64, arm64, and armv7 with
     `cargo auditable` (binaries embed a compressed `Cargo.lock`-equivalent
     so SBOM tools can enumerate Rust deps without source access)
   - Generates per-file `.sha256` sidecars and a combined `SHA256SUMS`
   - Cosign keyless-signs every `.so` using the GitLab OIDC id token,
     producing `.cosign.bundle` files
   - Uploads everything to the Generic Package Registry under
     `packages/generic/maxmind/<tag>/`
   - Creates a GitLab release with all artifacts linked as release assets

## Verifying a release

After CI finishes, verify the release end-to-end from a clean machine:

```sh
make verify-release VERSION=v0.2.1
```

This downloads `SHA256SUMS`, every artifact named in it, and every artifact's
cosign bundle from the package registry, then runs `sha256sum -c` and
`cosign verify-blob` against the project's tag identity.

## Versioning

This project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html):

- **MAJOR**: Breaking changes to the plugin API, output variable names, or config parameters
- **MINOR**: New database types, new config options, new output variables
- **PATCH**: Bug fixes, performance improvements, documentation updates

Pre-release versions use suffixes: `v0.1.0-alpha.1`, `v0.1.0-beta.1`, `v0.1.0-rc.1`.
