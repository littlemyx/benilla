# benilla on iPad

An Xcode host that bundles the Rust `benilla` binary as an iPad app. Xcode only builds (through
`build_rust.sh`), signs and installs; there is no Swift.

## Prerequisites

- Xcode, `xcodegen` (`brew install xcodegen`), and the Rust targets
  `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.
- Your own 1.12.1 client's `Data/` folder (the `*.MPQ` files). Nothing from it is in this repo.

## Build

```sh
cd platform/ios
cp Local.xcconfig.example Local.xcconfig   # set DEVELOPMENT_TEAM, a unique PRODUCT_BUNDLE_IDENTIFIER
xcodegen generate
open benilla.xcodeproj                      # pick your iPad, Run
```

Debug builds the cargo `dev` profile, Release the `play` profile, always
`-p benilla --no-default-features`. A simulator build needs no signing:

```sh
xcodebuild -project benilla.xcodeproj -scheme benilla -configuration Debug \
  -destination 'platform=iOS Simulator,name=iPad Pro 13-inch (M5)' CODE_SIGNING_ALLOWED=NO build
```

A free Apple ID signs an app that stops launching after 7 days; run it from Xcode again.

## Data and state

The app container is `$HOME`. benilla defaults `WOW_DATA` to `Documents/WoW/Data` and
`BENILLA_HOME` (the `benilla-config` folder: `config.toml`, bindings, saved variables) to
`Documents/benilla-config`, unless the variables are already set.

To put the data there: connect the iPad, open it in Finder, Files tab, drag your `Data/` folder
onto *benilla*, so that it ends up as `WoW/Data/*.MPQ` (create the `WoW` folder first). The same
folders show in the Files app under *On My iPad > benilla*. Without an install the app logs
`no WoW install found` and shows an empty window.
