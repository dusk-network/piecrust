#!/bin/sh

set -e

# The script should check whether the inputs are set, or print usage
# information otherwise.
if [ -z "$1" ]; then
    echo "Usage: $0 <compiler-version>"
    exit 1
fi

RELEASES_URL=https://github.com/dusk-network/rust/releases/download
SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
VERIFY_ARTIFACT=$SCRIPT_DIR/verify-artifact.sh

COMPILER_VERSION=$1
COMPILER_ARCH=$(rustc -vV | sed -n 's|host: ||p')

ARTIFACT_NAME=duskc-$COMPILER_ARCH.zip
ARTIFACT_URL=$RELEASES_URL/$COMPILER_VERSION/$ARTIFACT_NAME

ARTIFACT_DIR=$PWD/target/dusk/$COMPILER_VERSION
ARTIFACT_PATH=$ARTIFACT_DIR/$ARTIFACT_NAME
ARTIFACT_LABEL="compiler $COMPILER_VERSION $ARTIFACT_NAME"

EXPECTED_SHA256=
# Add each supported compiler archive here as:
#   <compiler-version>/<artifact-name>) EXPECTED_SHA256=<sha256> ;;
case "$COMPILER_VERSION/$ARTIFACT_NAME" in
    v0.3.0-rc/duskc-aarch64-apple-darwin.zip)
        EXPECTED_SHA256=860abf704d430b817a2b8f8d9d996e7f3c8be28887476e86095a4bcf955acb7e
        ;;
    v0.3.0-rc/duskc-aarch64-unknown-linux-gnu.zip)
        EXPECTED_SHA256=d8223f4447616dc9c12cfd0fb49a223962d4c23fa5e9b4b32cc91eeebd209e12
        ;;
    v0.3.0-rc/duskc-x86_64-apple-darwin.zip)
        EXPECTED_SHA256=7834fcccf8194d33fe3c2f454be28e49a3f6a82f7783453cf5c2df8dccfdd6e9
        ;;
    v0.3.0-rc/duskc-x86_64-unknown-linux-gnu.zip)
        EXPECTED_SHA256=60bab7c9624071796f0e39cd57e20df3292550119bf6ca413721811c791e0013
        ;;
esac

if [ -z "$EXPECTED_SHA256" ]; then
    echo "No pinned SHA-256 for $ARTIFACT_LABEL" >&2
    echo "Refusing to download or use an unpinned compiler artifact." >&2
    exit 1
fi

# If the artifact doesn't already exist in the target directory, download it,
# otherwise skip.
if [ ! -f "$ARTIFACT_PATH" ]; then
    echo "Downloading compiler version $COMPILER_VERSION"
    mkdir -p "$ARTIFACT_DIR"
    curl -fL --remove-on-error "$ARTIFACT_URL" -o "$ARTIFACT_PATH"
    rm -rf "$ARTIFACT_DIR/unzipped" "$ARTIFACT_DIR/extracted"
fi

"$VERIFY_ARTIFACT" "$ARTIFACT_PATH" "$EXPECTED_SHA256" "$ARTIFACT_LABEL"

# Unzip the artifact, if it isn't already unzipped
UNZIPPED_DIR=$ARTIFACT_DIR/unzipped

if [ ! -d "$UNZIPPED_DIR" ] || [ "$(cat "$UNZIPPED_DIR/.artifact.sha256" 2>/dev/null || true)" != "$EXPECTED_SHA256" ]; then
    echo "Extracting compiler..."
    rm -rf "$UNZIPPED_DIR"
    mkdir -p "$UNZIPPED_DIR"
    unzip "$ARTIFACT_PATH" -d "$UNZIPPED_DIR" >> /dev/null
fi

printf '%s\n' "$EXPECTED_SHA256" > "$UNZIPPED_DIR/.artifact.sha256"

# Extract the tarballs, if they aren't already extracted
EXTRACTED_DIR=$ARTIFACT_DIR/extracted

if [ ! -d "$EXTRACTED_DIR" ] || [ "$(cat "$EXTRACTED_DIR/.artifact.sha256" 2>/dev/null || true)" != "$EXPECTED_SHA256" ]; then
    rm -rf "$EXTRACTED_DIR"
    mkdir -p "$EXTRACTED_DIR"
    tarballs=$(find "$UNZIPPED_DIR" -name '*.tar.gz')
    for tarball in $tarballs; do
        tar -xzf "$tarball" -C "$EXTRACTED_DIR" --strip-components=2
    done
    # We don't require the, clearly clobbered at this point, file manifest
    rm "$EXTRACTED_DIR/manifest.in"
fi

printf '%s\n' "$EXPECTED_SHA256" > "$EXTRACTED_DIR/.artifact.sha256"

# Ensure that the extracted compiler is symlinked in the toolchain directory
TOOLCHAIN_DIR=$HOME/.rustup/toolchains
TOOLCHAIN_LINK=$TOOLCHAIN_DIR/dusk

rm -f "$TOOLCHAIN_LINK"
ln -s "$EXTRACTED_DIR" "$TOOLCHAIN_LINK"
