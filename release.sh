#!/usr/bin/env bash
set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

VERSION="$1"

sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

git add Cargo.toml
git commit -m "release v$VERSION"
git tag "v$VERSION"
git push origin master
git push origin "v$VERSION"

echo "Released v$VERSION"
