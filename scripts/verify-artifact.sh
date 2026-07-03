#!/bin/sh

set -e

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <artifact-path> <expected-sha256> <artifact-label>" >&2
    exit 1
fi

ARTIFACT_PATH=$1
EXPECTED_SHA256=$2
ARTIFACT_LABEL=$3

if [ -z "$EXPECTED_SHA256" ]; then
    echo "No expected SHA-256 configured for $ARTIFACT_LABEL" >&2
    exit 1
fi

if [ ! -f "$ARTIFACT_PATH" ]; then
    echo "Artifact not found for $ARTIFACT_LABEL: $ARTIFACT_PATH" >&2
    exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    SHA_LINE=$(sha256sum "$ARTIFACT_PATH")
    ACTUAL_SHA256=${SHA_LINE%%[[:space:]]*}
elif command -v shasum >/dev/null 2>&1; then
    SHA_LINE=$(shasum -a 256 "$ARTIFACT_PATH")
    ACTUAL_SHA256=${SHA_LINE%%[[:space:]]*}
else
    echo "Neither sha256sum nor shasum is available to verify $ARTIFACT_LABEL" >&2
    exit 1
fi

if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
    echo "SHA-256 mismatch for $ARTIFACT_LABEL" >&2
    echo "  expected: $EXPECTED_SHA256" >&2
    echo "  actual:   $ACTUAL_SHA256" >&2
    exit 1
fi
