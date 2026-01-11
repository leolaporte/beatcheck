#!/bin/bash
set -e

VERSION="${1:-$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)}"
DIST_DIR="dist"

echo "Building rss-reader v${VERSION}"

# Create dist directory
mkdir -p "$DIST_DIR"

# Detect current platform
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        PLATFORM="linux-x86_64"
        ;;
    Linux-aarch64)
        TARGET="aarch64-unknown-linux-gnu"
        PLATFORM="linux-aarch64"
        ;;
    Darwin-x86_64)
        TARGET="x86_64-apple-darwin"
        PLATFORM="macos-x86_64"
        ;;
    Darwin-arm64)
        TARGET="aarch64-apple-darwin"
        PLATFORM="macos-aarch64"
        ;;
    *)
        echo "Unsupported platform: $(uname -s)-$(uname -m)"
        exit 1
        ;;
esac

echo "Building for $PLATFORM ($TARGET)..."

# Build release
cargo build --release --target "$TARGET"

# Package
ARCHIVE="rss-reader-v${VERSION}-${PLATFORM}.tar.gz"
cd "target/$TARGET/release"
tar -czvf "../../../$DIST_DIR/$ARCHIVE" rss-reader
cd ../../..

echo ""
echo "Built: $DIST_DIR/$ARCHIVE"
echo ""
echo "To install locally:"
echo "  tar -xzf $DIST_DIR/$ARCHIVE -C ~/.local/bin/"
