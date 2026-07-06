#!/bin/sh

set -e

# The script should check whether the inputs are set, or print usage
# information otherwise.
if [ -z "$1" ] || [ -z "$2" ]; then
    echo "Usage: $0 <input-wasm> <output-wasm>"
    exit 1
fi

TRIPLE=$(rustc -vV | sed -n 's|host: ||p')
ARCH=$(echo "$TRIPLE" | cut -f 1 -d'-')

# This is a bit of a mess because target triples are pretty inconsistent
OS=$(echo "$TRIPLE" | cut -f 2 -d'-')
# If OS is not currently linux, or apple, then we get it again
if [ "$OS" != "linux" ] && [ "$OS" != "apple" ]; then
    OS=$(echo "$TRIPLE" | cut -f 3 -d'-')
fi
if [ "$OS" != "linux" ] && [ "$OS" != "apple" ]; then
    echo "OS not supported: $OS"
    exit 1
fi
# If OS is apple, change it to macos
if [ "$OS" = "apple" ]; then
    OS="macos"
fi

RELEASES_URL=https://github.com/bytecodealliance/wasm-tools/releases/download
SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
VERIFY_ARTIFACT=$SCRIPT_DIR/verify-artifact.sh

PROGRAM_VERSION=1.0.54

ARTIFACT_NAME=wasm-tools-$PROGRAM_VERSION-$ARCH-$OS.tar.gz
ARTIFACT_URL=$RELEASES_URL/wasm-tools-$PROGRAM_VERSION/$ARTIFACT_NAME

ARTIFACT_DIR=$PWD/target/wasm-tools/$PROGRAM_VERSION
ARTIFACT_PATH=$ARTIFACT_DIR/$ARTIFACT_NAME
ARTIFACT_LABEL="wasm-tools $ARTIFACT_NAME"

EXPECTED_SHA256=
case "$PROGRAM_VERSION/$ARTIFACT_NAME" in
    1.0.54/wasm-tools-1.0.54-aarch64-linux.tar.gz)
        EXPECTED_SHA256=8c323da7af91e19d18942d6cdc38c96ddbf432a05b1269dd31e4b744b74153b0
        ;;
    1.0.54/wasm-tools-1.0.54-aarch64-macos.tar.gz)
        EXPECTED_SHA256=a611497671ff841162f80baaeb54afbc25f7d875e204cf5a9f7255f1beeb17df
        ;;
    1.0.54/wasm-tools-1.0.54-x86_64-linux.tar.gz)
        EXPECTED_SHA256=69b39606b51827fce4b59b26ab7942541f24187c46423cccc0e8253b0a256ecb
        ;;
    1.0.54/wasm-tools-1.0.54-x86_64-macos.tar.gz)
        EXPECTED_SHA256=8dc6bf5d4e78911ef6933fff181091e98271cfd4c4d1c87a164c7cbe1b52dba6
        ;;
esac

if [ -z "$EXPECTED_SHA256" ]; then
    echo "No pinned SHA-256 for $ARTIFACT_LABEL" >&2
    echo "Refusing to download or use an unpinned wasm-tools artifact." >&2
    exit 1
fi

# If the artifact doesn't already exist in the target directory, download it,
# otherwise skip.
if [ ! -f "$ARTIFACT_PATH" ]; then
    echo "Downloading wasm-tools version $PROGRAM_VERSION"
    mkdir -p "$ARTIFACT_DIR"
    curl -fL --remove-on-error "$ARTIFACT_URL" -o "$ARTIFACT_PATH"
    rm -rf "$ARTIFACT_DIR/extracted"
fi

"$VERIFY_ARTIFACT" "$ARTIFACT_PATH" "$EXPECTED_SHA256" "$ARTIFACT_LABEL"

# Extract the tarball, if they aren't already extracted
EXTRACTED_DIR=$ARTIFACT_DIR/extracted

if [ ! -d "$EXTRACTED_DIR" ] || [ "$(cat "$EXTRACTED_DIR/.artifact.sha256" 2>/dev/null || true)" != "$EXPECTED_SHA256" ]; then
    rm -rf "$EXTRACTED_DIR"
    mkdir -p "$EXTRACTED_DIR"
    tar -xzf "$ARTIFACT_PATH" -C "$EXTRACTED_DIR" --strip-components=1
    printf '%s\n' "$EXPECTED_SHA256" > "$EXTRACTED_DIR/.artifact.sha256"
fi

"$EXTRACTED_DIR"/wasm-tools strip -a "$1" -o "$2"
