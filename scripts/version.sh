#!/bin/sh
# Read, check or bump the application version.
#
# The version lives in three files that have no way to notice each other:
# Cargo.toml (the Rust workspace), package.json (the frontend) and tauri.conf.json
# (what the installer and the About box show). They drifted apart the moment anyone
# edited one by hand, so nothing edits them by hand.
#
#   scripts/version.sh              print the version, or fail if the three disagree
#   scripts/version.sh patch        0.1.0 -> 0.1.1
#   scripts/version.sh minor        0.1.0 -> 0.2.0
#   scripts/version.sh major        0.1.0 -> 1.0.0
#   scripts/version.sh 0.4.2        set it exactly
set -eu

cd "$(dirname "$0")/.."

cargo_version() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1; }
npm_version()   { sed -n 's/^  "version": "\(.*\)",/\1/p' package.json | head -1; }
tauri_version() { sed -n 's/^  "version": "\(.*\)",/\1/p' src-tauri/tauri.conf.json | head -1; }

current=$(cargo_version)
for other in "$(npm_version)" "$(tauri_version)"; do
    if [ "$other" != "$current" ]; then
        echo "version mismatch: Cargo.toml has $current, another file has $other" >&2
        exit 1
    fi
done

if [ $# -eq 0 ]; then
    echo "$current"
    exit 0
fi

case "$1" in
    major|minor|patch)
        major=${current%%.*}
        rest=${current#*.}
        minor=${rest%%.*}
        patch=${rest#*.}
        case "$1" in
            major) next="$((major + 1)).0.0" ;;
            minor) next="$major.$((minor + 1)).0" ;;
            patch) next="$major.$minor.$((patch + 1))" ;;
        esac
        ;;
    [0-9]*.[0-9]*.[0-9]*)
        next="$1"
        ;;
    *)
        echo "usage: $0 [major|minor|patch|X.Y.Z]" >&2
        exit 1
        ;;
esac

# Only the first `version = ` in Cargo.toml — that is [workspace.package]; the pinned
# dependency versions below it must not move.
/usr/bin/sed -i '' "1,/^version = /s/^version = \"$current\"/version = \"$next\"/" Cargo.toml
/usr/bin/sed -i '' "s/^  \"version\": \"$current\",/  \"version\": \"$next\",/" package.json
/usr/bin/sed -i '' "s/^  \"version\": \"$current\",/  \"version\": \"$next\",/" src-tauri/tauri.conf.json

# Cargo.lock records the workspace crates' versions too.
cargo metadata --format-version 1 >/dev/null 2>&1 || true

echo "$current -> $next"
