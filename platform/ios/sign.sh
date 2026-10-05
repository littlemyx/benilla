#!/usr/bin/env bash
# Xcode post-build phase: sign the app bundle. Xcode skips its own CodeSign step for a target it
# did not link (the binary comes from build_rust.sh), so the bundle is signed here with the
# identity and the profile Xcode resolved, and the entitlements the profile carries.
set -euo pipefail
app="$TARGET_BUILD_DIR/$WRAPPER_NAME"
if [ "${CODE_SIGNING_ALLOWED:-YES}" = NO ] || [ -z "${EXPANDED_CODE_SIGN_IDENTITY:-}" ]; then
    echo "sign: skipped (CODE_SIGNING_ALLOWED=${CODE_SIGNING_ALLOWED:-YES})"
    exit 0
fi
profile="$app/embedded.mobileprovision"
if [ ! -f "$profile" ]; then
    echo "sign: no embedded.mobileprovision in $app" >&2
    exit 1
fi
ents="$TEMP_DIR/benilla.entitlements"
security cms -D -i "$profile" | plutil -extract Entitlements xml1 -o "$ents" -
codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --entitlements "$ents" --timestamp=none "$app"
codesign --verify --verbose=2 "$app"
