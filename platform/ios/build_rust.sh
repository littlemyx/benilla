#!/usr/bin/env bash
# Xcode run-script phase: build the Rust `benilla` binary for the SDK being built and drop it where
# Xcode expects the app's executable. Debug -> cargo `dev`, Release -> `play`.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

case "${PLATFORM_NAME:-iphoneos}" in
    iphoneos) triple=aarch64-apple-ios ;;
    iphonesimulator) triple=aarch64-apple-ios-sim ;;
    *) echo "build_rust: unsupported PLATFORM_NAME=$PLATFORM_NAME" >&2; exit 1 ;;
esac
if [ "${CONFIGURATION:-Debug}" = Release ]; then
    profile=play; dir=play
else
    profile=dev; dir=debug
fi

# BENILLA_CARGO_FEATURES (an xcconfig build setting, empty by default) adds cargo features, e.g.
# `dev` for a development device build that honours the dev switches; a player build leaves it empty.
features=()
if [ -n "${BENILLA_CARGO_FEATURES:-}" ]; then
    features=(--features "$BENILLA_CARGO_FEATURES")
fi

cargo build --target "$triple" --profile "$profile" -p benilla --no-default-features ${features[@]+"${features[@]}"}

out="$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
mkdir -p "$(dirname "$out")"
cp "target/$triple/$dir/benilla" "$out"
