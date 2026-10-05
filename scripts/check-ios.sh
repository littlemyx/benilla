#!/usr/bin/env bash
# The iOS seam check: `cargo check` and `cargo clippy -D warnings` of the app for aarch64-apple-ios,
# without default features (no `dev`). A `cfg(target_os)` is invisible to the gates of the platform
# you stand on, so run this after touching one. Not part of gates.sh: CI is Ubuntu-only.
# Clippy runs on the lib and bin targets only; `--all-targets` would build the test and bench
# harnesses, which an iOS target cannot link or run. Needs the aarch64-apple-ios rust target.
# Output goes to target/check-ios.log.
#
#   scripts/check-ios.sh
set -uo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"
export IPHONEOS_DEPLOYMENT_TARGET=16.0
mkdir -p target
log=target/check-ios.log

{
    cargo check --target aarch64-apple-ios -p benilla --no-default-features &&
        cargo clippy --target aarch64-apple-ios -p benilla --no-default-features -- -D warnings
} 2>&1 | tee "$log"
status="${PIPESTATUS[0]}"
if [ "$status" -ne 0 ]; then
    echo "check-ios: FAILED (see $log)" >&2
    exit "$status"
fi
echo "check-ios: ok"
