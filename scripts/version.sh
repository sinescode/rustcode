#!/usr/bin/env bash
# Version management script for RustCode
#
# Usage:
#   ./scripts/version.sh              # Print current version
#   ./scripts/version.sh bump major   # Bump major version (0.1.0 -> 1.0.0)
#   ./scripts/version.sh bump minor   # Bump minor version (0.1.0 -> 0.2.0)
#   ./scripts/version.sh bump patch   # Bump patch version (0.1.0 -> 0.1.1)
#   ./scripts/version.sh set 0.2.0    # Set specific version
#   ./scripts/version.sh tag          # Create git tag for current version
#   ./scripts/version.sh changelog    # Generate changelog from git log
#   ./scripts/version.sh release      # Full release: bump + changelog + tag

set -euo pipefail

CARGO_TOML="Cargo.toml"

# ---------------------------------------------------------------------------
# Color helpers
# ---------------------------------------------------------------------------
MUTED='\033[0;2m'
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

info()  { echo -e "${MUTED}$*${NC}"; }
ok()    { echo -e "${GREEN}$*${NC}"; }
error() { echo -e "${RED}$*${NC}" >&2; }

# ---------------------------------------------------------------------------
# Read current version from Cargo.toml
# ---------------------------------------------------------------------------
read_version() {
    if [[ ! -f "$CARGO_TOML" ]]; then
        error "Error: $CARGO_TOML not found in current directory"
        exit 1
    fi
    local ver
    ver=$(grep -E '^version\s*=' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    if [[ -z "$ver" ]]; then
        error "Error: Could not read version from $CARGO_TOML"
        exit 1
    fi
    echo "$ver"
}

# ---------------------------------------------------------------------------
# Update version in Cargo.toml
# ---------------------------------------------------------------------------
write_version() {
    local new_version="$1"

    if [[ "$(uname -s)" == "Darwin" ]]; then
        sed -i '' -E "s/^(version\s*=\s*\")[0-9]+\.[0-9]+\.[0-9]+(\")/\1${new_version}\2/" "$CARGO_TOML"
    else
        sed -i -E "s/^(version\s*=\s*\")[0-9]+\.[0-9]+\.[0-9]+(\")/\1${new_version}\2/" "$CARGO_TOML"
    fi

    ok "Version updated to ${new_version}"
}

# ---------------------------------------------------------------------------
# Version bump logic
# ---------------------------------------------------------------------------
bump_version() {
    local current="$1"
    local part="${2:-patch}"

    IFS='.' read -r major minor patch <<< "$current" || true

    case "$part" in
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        patch)
            patch=$((patch + 1))
            ;;
        *)
            error "Error: Unknown bump part '$part'. Use major, minor, or patch."
            exit 1
            ;;
    esac

    echo "${major}.${minor}.${patch}"
}

# ---------------------------------------------------------------------------
# Create git tag
# ---------------------------------------------------------------------------
create_tag() {
    local version="$1"
    local tag="v${version}"

    if git tag | grep -q "^${tag}$"; then
        error "Error: Tag '${tag}' already exists"
        exit 1
    fi

    git tag -a "${tag}" -m "Release ${tag}"
    ok "Created git tag: ${tag}"
    info "Push with: git push origin ${tag}"
}

# ---------------------------------------------------------------------------
# Generate changelog from git log
# ---------------------------------------------------------------------------
generate_changelog() {
    local version="$1"
    local tag="v${version}"

    # Find previous tag
    local prev_tag
    prev_tag=$(git tag --sort=-v:refname | head -2 | tail -1 || echo "")

    local changelog_file="CHANGELOG.md"

    # Create header if file doesn't exist
    if [[ ! -f "$changelog_file" ]]; then
        cat > "$changelog_file" <<- EOF
# Changelog

All notable changes to RustCode will be documented in this file.

EOF
    fi

    # Generate new changelog entry
    local tmp_file
    tmp_file=$(mktemp)

    {
        echo "## [${version}] - $(date +%Y-%m-%d)"
        echo ""

        if [[ -n "$prev_tag" ]]; then
            echo "### Changes since ${prev_tag}"
            echo ""
            git log --oneline --no-decorate "${prev_tag}..HEAD" 2>/dev/null || echo "  - No changes"
        else
            echo "### Initial release"
            echo ""
            git log --oneline --no-decorate "$(git rev-list --max-parents=0 HEAD)..HEAD" 2>/dev/null || echo "  - Initial release"
        fi
        echo ""
    } > "$tmp_file"

    # Prepend to existing changelog
    if [[ -f "$changelog_file" ]]; then
        cat "$changelog_file" >> "$tmp_file"
    fi
    mv "$tmp_file" "$changelog_file"

    ok "Changelog generated in ${changelog_file}"
}

# ===========================================================================
# Main
# ===========================================================================

CURRENT_VERSION=$(read_version)
info "Current version: ${CURRENT_VERSION}"

case "${1:-}" in
    bump)
        PART="${2:-patch}"
        NEW_VERSION=$(bump_version "$CURRENT_VERSION" "$PART")
        info "Bumping ${PART}: ${CURRENT_VERSION} -> ${NEW_VERSION}"
        write_version "$NEW_VERSION"
        echo "$NEW_VERSION"
        ;;
    set)
        if [[ -z "${2:-}" ]]; then
            error "Error: 'set' requires a version argument (e.g., 0.2.0)"
            exit 1
        fi
        NEW_VERSION="${2#v}"
        if [[ ! "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            error "Error: Invalid version format '${2}'. Expected X.Y.Z"
            exit 1
        fi
        info "Setting version: ${CURRENT_VERSION} -> ${NEW_VERSION}"
        write_version "$NEW_VERSION"
        echo "$NEW_VERSION"
        ;;
    tag)
        info "Creating tag for version ${CURRENT_VERSION}..."
        create_tag "$CURRENT_VERSION"
        ;;
    changelog)
        generate_changelog "$CURRENT_VERSION"
        ;;
    release)
        PART="${2:-patch}"
        NEW_VERSION=$(bump_version "$CURRENT_VERSION" "$PART")
        info "Bumping ${PART}: ${CURRENT_VERSION} -> ${NEW_VERSION}"
        write_version "$NEW_VERSION"
        generate_changelog "$NEW_VERSION"
        create_tag "$NEW_VERSION"
        ok "Release v${NEW_VERSION} ready!"
        info "Push with: git push origin HEAD && git push origin v${NEW_VERSION}"
        echo "$NEW_VERSION"
        ;;
    *)
        echo "$CURRENT_VERSION"
        ;;
esac
