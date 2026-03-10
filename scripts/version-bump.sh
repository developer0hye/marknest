#!/usr/bin/env bash
# Bump version across all Cargo.toml and npm package.json files.
# Usage: ./scripts/version-bump.sh 0.2.0

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "Example: $0 0.2.0"
  exit 1
fi

NEW_VERSION="$1"

# Validate semver format
if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: version must be semver (e.g., 0.2.0 or 1.0.0-beta.1)"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Bumping version to ${NEW_VERSION}..."

# Update workspace Cargo.toml version
sed -i.bak "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "${REPO_ROOT}/Cargo.toml"
rm -f "${REPO_ROOT}/Cargo.toml.bak"

# Update workspace dependency versions for internal crates
sed -i.bak "s/marknest-core = { version = \"[^\"]*\"/marknest-core = { version = \"${NEW_VERSION}\"/" "${REPO_ROOT}/Cargo.toml"
sed -i.bak "s/marknest = { version = \"[^\"]*\"/marknest = { version = \"${NEW_VERSION}\"/" "${REPO_ROOT}/Cargo.toml"
rm -f "${REPO_ROOT}/Cargo.toml.bak"

# Update all npm package.json files
for pkg_json in "${REPO_ROOT}"/npm/*/package.json; do
  sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"${NEW_VERSION}\"/" "$pkg_json"
  rm -f "${pkg_json}.bak"
done

# Update optionalDependencies versions in umbrella package
UMBRELLA="${REPO_ROOT}/npm/marknest/package.json"
sed -i.bak "s/\"marknest-darwin-arm64\": \"[^\"]*\"/\"marknest-darwin-arm64\": \"${NEW_VERSION}\"/" "$UMBRELLA"
sed -i.bak "s/\"marknest-darwin-x64\": \"[^\"]*\"/\"marknest-darwin-x64\": \"${NEW_VERSION}\"/" "$UMBRELLA"
sed -i.bak "s/\"marknest-linux-x64\": \"[^\"]*\"/\"marknest-linux-x64\": \"${NEW_VERSION}\"/" "$UMBRELLA"
sed -i.bak "s/\"marknest-linux-arm64\": \"[^\"]*\"/\"marknest-linux-arm64\": \"${NEW_VERSION}\"/" "$UMBRELLA"
sed -i.bak "s/\"marknest-win32-x64\": \"[^\"]*\"/\"marknest-win32-x64\": \"${NEW_VERSION}\"/" "$UMBRELLA"
rm -f "${UMBRELLA}.bak"

echo "Updated Cargo.lock..."
cd "${REPO_ROOT}" && cargo generate-lockfile 2>/dev/null || true

echo "Done. Version bumped to ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  1. Review changes: git diff"
echo "  2. Commit: git commit -am 'chore: bump version to ${NEW_VERSION}'"
echo "  3. Tag: git tag v${NEW_VERSION}"
echo "  4. Push: git push && git push --tags"
