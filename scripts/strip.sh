#!/bin/sh

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

PROGRAM_VERSION=1.0.54

ARTIFACT_NAME=wasm-tools-$PROGRAM_VERSION-$ARCH-$OS.tar.gz
ARTIFACT_URL=$RELEASES_URL/wasm-tools-$PROGRAM_VERSION/$ARTIFACT_NAME

ARTIFACT_DIR=$PWD/target/wasm-tools/$PROGRAM_VERSION
ARTIFACT_PATH=$ARTIFACT_DIR/$ARTIFACT_NAME

# If the artifact doesn't already exist in the target directory, download it,
# otherwise skip.
if [ ! -f "$ARTIFACT_PATH" ]; then
    echo "Downloading wasm-tools version $PROGRAM_VERSION"
    mkdir -p "$ARTIFACT_DIR"
    curl -L "$ARTIFACT_URL" -o "$ARTIFACT_PATH"
fi

# Extract the tarball, if they aren't already extracted
EXTRACTED_DIR=$ARTIFACT_DIR/extracted

if [ ! -d "$EXTRACTED_DIR" ]; then
    mkdir -p "$EXTRACTED_DIR"
    tar -xzf "$ARTIFACT_PATH" -C "$EXTRACTED_DIR" --strip-components=1
fi

"$EXTRACTED_DIR"/wasm-tools strip -a "$1" -o "$2"
