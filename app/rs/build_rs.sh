#!/bin/bash

# Exit on error
set -e

# Use ANDROID_NDK_HOME from the environment (set by GitHub Actions or your local machine)
if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "Error: ANDROID_NDK_HOME is not set."
    exit 1
fi

# Configuration
TARGET="aarch64-linux-android"
OUTPUT_DIR="../app/src/main/jniLibs/arm64-v8a"

echo "--- Building Rust binaries for $TARGET ---"

# Ensure the output directory exists
mkdir -p "$OUTPUT_DIR"

# Run the build using cargo-ndk
# We use NDK 25+ style toolchains
cargo ndk -t "$TARGET" -o "$OUTPUT_DIR" build --release

echo "--- Rust Build Complete ---"
echo "Binary location: $OUTPUT_DIR"
