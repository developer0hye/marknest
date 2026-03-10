# Release CI + npm Binary Distribution

## Reference
- **esbuild**: Platform-specific npm packages with JS shim bin entry
- **Biome**: `@biomejs/cli-*` optional dependency pattern
- **Pattern**: Umbrella npm package with `optionalDependencies` for platform-specific binary packages

## Structure
- Umbrella `marknest` npm package detects `process.platform`/`process.arch`
- Platform packages (`marknest-darwin-arm64`, etc.) contain only the binary
- `os` + `cpu` fields in `package.json` let npm auto-select the right package
- CI cross-compiles via GitHub Actions matrix (native + `cross` for linux-arm64)

## Publish Order (crates.io)
1. `marknest-core` (no internal deps)
2. `marknest` (depends on core)
3. `marknest-server` (depends on both)
4. `marknest-wasm` → `publish = false` (browser-only, no crates.io)

## Version Sync
- `scripts/version-bump.sh` updates all Cargo.toml + npm package.json files
- Tag push `v*` triggers the release workflow
