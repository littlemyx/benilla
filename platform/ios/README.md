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

## Reading the log

The device console does not reach `idevicesyslog`, so the app also writes its log to
`Documents/benilla-config/Logs/client.log` in its container, truncated at every launch. Pull it:

```sh
xcrun devicectl device copy from --device <udid> --source Documents/benilla-config/Logs/client.log \
  --destination ./client.log --domain-type appDataContainer --domain-identifier <bundle id> --user mobile
```

## Checking input on a device

Needs an iPad with a hardware keyboard and a mouse or trackpad paired. The app logs no per-event
input trace, so the checks are behavioural.

1. Launch; the log shows `clipboard: UIPasteboard` after the first paste or copy.
2. Log in to the server (type the account name and password with the keyboard; the characters
   must appear once each, no doubles, no drops).
3. In the world, WASD moves, Space jumps, Esc opens the game menu.
4. Hold the right mouse button and drag: the view turns, the pointer is hidden and stays put.
   Release: the pointer returns where the drag began, with no error lines in the log.
5. Scroll the wheel: the camera zooms in and out.
6. Type in a chat box, select the text, Cmd+C, then Cmd+V twice: the text is pasted twice. Copy
   text in another app and paste it here.
7. Switch to another app and back during a drag: the look ends cleanly, no stuck keys or buttons.
8. Move the pointer over UI buttons and NPCs in the world: they highlight; a click lands where the
   pointer is drawn.
9. Drag an action-bar button to another slot, holding the left button: it follows the pointer and
   drops where released. Move a frame by its title the same way.
10. The 1.12 cursor draws at the pointer (the arrow on the login screen, the sword over a hostile),
    a picked-up item's icon follows it, and the system arrow is hidden over the game. The log has
    `system pointer hidden over the view`; hover still moves the cursor, and it vanishes in a
    right-button look.
