# Krad Spotify

A [Krad ADE](https://github.com/joordih/krad-ade) extension that brings a Control Center–style *Now Playing* panel for the local Spotify desktop player into Krad ADE. Toggle it with a shortcut, or drive Spotify from the command palette.

Built in Rust on [`krad-extension-sdk`](https://crates.io/crates/krad-extension-sdk) 0.3.0. It talks to the Spotify desktop app through each operating system's own media interface, so it needs no Spotify account credentials, OAuth flow, or Web API access.

## Platform support

| Platform | Backend | Status |
| --- | --- | --- |
| macOS | AppleScript (`osascript`) | Tested |
| Linux | MPRIS over D-Bus | Builds; not yet tested against Spotify |
| Windows | System media transport controls (`Windows.Media.Control`) | Builds; not yet tested against Spotify |

Features available on each platform:

| Feature | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Track details and artwork | ✓ | ✓ | ✓ (from the media session thumbnail) |
| Play/pause, next, previous | ✓ | ✓ | ✓ |
| Progress | ✓ | ✓ ¹ | ✓ |
| Shuffle and repeat | ✓ | ✓ ¹ | ✓ |
| Volume | ✓ | ✓ ¹ | — ² |
| Open on the current track | ✓ | ✓ | Opens Spotify ³ |

1. These calls follow the MPRIS specification, but some Spotify versions report a position of 0 or ignore changes to shuffle, repeat, or volume.
2. Windows' media controls have no per-app volume, so the volume tile and commands are hidden on Windows.
3. The Windows media session does not expose the track ID, so Spotify opens on whatever it is showing.

## Features

- **Control Center layout:** a side panel made of rounded tiles that you show or hide with a shortcut.
- **Now playing:** large rounded album artwork, track title, artist, album, track number, and album artist (when it differs from the track artist).
- **Progress:** elapsed time, track duration, and a progress bar.
- **Playback state:** a status badge for *playing*, *paused*, or *stopped*.
- **Transport controls:** previous, play/pause, and next.
- **Playback modes:** shuffle and repeat toggles, highlighted while the mode is on.
- **Volume:** step up or down, with a level bar and percentage.
- **Open in Spotify:** brings Spotify to the front.
- **Commands and shortcuts:** every action is in the command palette and has a shortcut inside Krad ADE.
- **Settings:** polling interval, volume step, and an option to turn artwork off, changeable without restarting.

Features a platform does not support are hidden from the panel (see [Platform support](#platform-support)).

## Requirements

| Component | Version |
| --- | --- |
| OS | macOS, Linux (x64 or arm64, with a D-Bus session bus), or Windows 10/11 (x64 or arm64) |
| Spotify | Desktop app, installed and running |
| Krad ADE | Extension API `1.0` |
| Rust toolchain | Stable, for building from source |
| `curl` | On `PATH` for artwork downloads (macOS and Linux) |

## Installation

1. Build the binary for your machine and install it into `bin/`:

   ```sh
   # macOS and Linux
   scripts/install.sh          # this machine's architecture
   scripts/install.sh all      # arm64 and x64
   ```

   ```powershell
   # Windows (PowerShell)
   scripts\install.ps1         # this machine's architecture
   scripts\install.ps1 -Arch arm64
   ```

   The scripts build in release mode and replace the binary with a new file rather than overwriting it in place. On macOS, overwriting a binary that Krad ADE is running makes the system kill the new one with *Code Signature Invalid*. On Windows, a running `.exe` is locked, so disable the extension in Krad ADE before installing.

   Cross-architecture builds need the matching Rust standard library, available through `rustup target add <target>`. The Homebrew Rust package only includes the host target.

2. Keep `extension.toml` and `bin/` together in one folder. The extension must be able to write to this folder, because it caches the artwork there.
3. In Krad ADE, open the extension manager, add that local folder, and enable **Spotify**.

`extension.toml` declares these binaries; Krad ADE picks the one for the current platform:

| Platform | Binary |
| --- | --- |
| macOS (Apple Silicon / Intel) | `bin/krad-spotify-darwin-arm64` / `bin/krad-spotify-darwin-x64` |
| Linux (x64 / arm64) | `bin/krad-spotify-linux-x64` / `bin/krad-spotify-linux-arm64` |
| Windows (x64 / arm64) | `bin/krad-spotify-win-x64.exe` / `bin/krad-spotify-win-arm64.exe` |

## Usage

Press the *Show / hide panel* shortcut (or run **Spotify: Show / Hide Control Center**) to show or hide the panel. It opens in Krad ADE's side panel and stacks these tiles, from top to bottom:

1. **Header:** playback status badge and an *Open in Spotify* button.
2. **Now Playing:** artwork, track details, progress, and transport controls.
3. **Shuffle** and **Repeat:** side-by-side tiles that are highlighted while the mode is on.
4. **Volume:** −/+ buttons around a level bar.

The panel is also listed among the side panel's tabs. While it is open, it shows the current track and refreshes at the configured interval. When Spotify is not running, the view says so and its controls are disabled. The extension never launches Spotify by itself.

### Commands and keyboard shortcuts

All actions are in the command palette under the `Spotify:` prefix.

| Action | Command ID | macOS | Linux and Windows |
| --- | --- | --- | --- |
| Show / hide panel | `toggle-panel` | `⌃⌥⌘ P` | `Ctrl+Alt+Shift+P` |
| Play / pause | `playpause` | `⌃⌥⌘ Space` | `Ctrl+Alt+Shift+Space` |
| Next track | `next` | `⌃⌥⌘ →` | `Ctrl+Alt+Shift+→` |
| Previous track | `previous` | `⌃⌥⌘ ←` | `Ctrl+Alt+Shift+←` |
| Volume up | `volume-up` | `⌃⌥⌘ ↑` | `Ctrl+Alt+Shift+↑` |
| Volume down | `volume-down` | `⌃⌥⌘ ↓` | `Ctrl+Alt+Shift+↓` |
| Toggle shuffle | `shuffle` | `⌃⌥⌘ S` | `Ctrl+Alt+Shift+S` |
| Toggle repeat | `repeat` | `⌃⌥⌘ R` | `Ctrl+Alt+Shift+R` |
| Open in Spotify | `open` | `⌃⌥⌘ O` | `Ctrl+Alt+Shift+O` |

The shortcuts are chosen not to collide with Krad ADE's built-in bindings. Krad ADE detects conflicts with other shortcuts and does not bind a combination twice. On Windows, the volume commands report that volume is not supported.

## Configuration

These settings are declared in `extension.toml` and edited in Krad ADE's extension settings. The extension reads them at startup and again whenever they change. No restart is needed.

| Key | Default | Range | Description |
| --- | --- | --- | --- |
| `poll-interval` | `1` | 1–30 seconds | How often Spotify's state is read while the view is open. |
| `volume-step` | `10` | 1–50 | Volume points (out of 100) added or removed by the volume commands. |
| `show-cover` | `true` | on / off | Fetch and show the album artwork. When off, the extension makes no network requests. |

- Values outside a range are clamped to the nearest limit.
- Missing, mistyped, or non-finite values fall back to the default.
- If Krad ADE cannot provide the settings, the extension keeps the defaults and logs a warning.

## Permissions

### macOS

The extension sends Apple Events to Spotify (`com.spotify.client`) through `/usr/bin/osascript`. The first query or action triggers macOS's **Automation** consent prompt. Accept it.

The app that macOS lists as the requester depends on how Krad ADE launches the extension. If you denied the prompt, go to **System Settings → Privacy & Security → Automation** and enable Spotify for that app.

No Accessibility, microphone, or music library permissions are needed. `/usr/bin/pgrep` is only used to check whether Spotify is running.

### Linux

The extension connects to the D-Bus **session** bus and talks to `org.mpris.MediaPlayer2.spotify`. No extra permissions are needed. Sandboxed Spotify packages (Flatpak, Snap) must be allowed to own their MPRIS name on the session bus, which they normally are. Opening a track uses `xdg-open` with a `spotify:` URI.

### Windows

The extension reads Spotify's session from the system media controls. No extra permissions are needed. Spotify only appears there while it is running.

### Troubleshooting

| Symptom | Cause | Resolution |
| --- | --- | --- |
| View says Spotify is not available | Spotify is not running (or, on Linux, not on the session bus) | Open Spotify. |
| Automation error (`-1743`, macOS) | Automation permission denied | Grant it in **Privacy & Security → Automation**. |
| Timeout error (`-1712`, macOS) | Spotify did not answer the Apple Event in time | Retry. Check that Spotify is responsive. |
| "Spotify refused the request" (Windows) | Spotify rejected a media control request | Retry; some controls are unavailable during ads or for some content. |
| Placeholder instead of artwork | Fetch failed, `curl` is missing, the extension folder is read-only, or `show-cover` is off | Check network access, `curl`, and folder permissions. |

Any other error is shown as-is in the view or as a toast.

## How it works

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Extension entry point: panel rendering, view events, commands (including the panel toggle), settings, and the polling loop. |
| `src/spotify/mod.rs` | `SpotifyAdapter`, the shared track model, and the per-platform capabilities. |
| `src/spotify/macos.rs` | macOS backend: AppleScript through `osascript`. |
| `src/spotify/linux.rs` | Linux backend: MPRIS over D-Bus with `zbus`. |
| `src/spotify/windows.rs` | Windows backend: `Windows.Media.Control` with the `windows` crate. |
| `src/artwork.rs` | Fetches, resizes, rounds, and caches the album artwork. |

Only the backend for the target OS is compiled, and the Linux and Windows dependencies are target-specific.

### macOS

The backend uses terms from Spotify's scripting dictionary (`Spotify.app/Contents/Resources/Spotify.sdef`):

- **Track:** `current track`, `name`, `artist`, `album`, `album artist`, `track number`, `duration`, `artwork url`, `spotify url`.
- **Player:** `player state`, `player position`, `shuffling`, `repeating`, `sound volume`.
- **Commands:** `playpause`, `next track`, `previous track`.

All properties are read in one script, and each one is guarded separately, so a missing value does not fail the whole read. Numbers are rounded inside AppleScript to avoid decimal separators that depend on the system locale.

### Linux

The backend reads the `org.mpris.MediaPlayer2.Player` properties `Metadata` (`xesam:title`, `xesam:artist`, `xesam:album`, `xesam:albumArtist`, `xesam:trackNumber`, `mpris:length`, `mpris:artUrl`, `mpris:trackid`), `PlaybackStatus`, `Position`, `Shuffle`, `LoopStatus`, and `Volume`. It calls `PlayPause`, `Next`, and `Previous`, and writes `Shuffle`, `LoopStatus`, and `Volume`. Repeat toggles between `None` and `Playlist`.

### Windows

The backend finds the media session whose app ID contains `spotify` and reads its media properties, playback info, and timeline. While playing, the position is advanced by the time since the timeline's last update. Controls use the session's `Try…Async` methods. Repeat toggles between `None` and `List`.

### Artwork

Krad ADE's view renderer only draws images bundled with the extension (`ImageSource::Bundled`). It does not clip images to rounded corners. `src/artwork.rs` works around both limits:

1. Gets the image: downloads the artwork URL with `curl` on macOS and Linux (HTTPS only, 5-second timeout), or reads the media session thumbnail on Windows.
2. Resizes it to 320 × 320 px, twice the 160 px display size so it stays sharp on high-DPI screens.
3. Makes the corners transparent, with anti-aliasing.
4. Saves it as a PNG in `cache/` inside the extension folder.

Only the current track's artwork is kept. The cache directory is git-ignored.

## Limitations

- **Local control only.** The extension controls the Spotify desktop app on this computer. It does not use Spotify Connect, the Web API, or OAuth.
- **Not a floating popup.** The extension API (0.3.0) only places views in fixed zones (sidebar, side panel, utilities, editor tab). It has no floating windows or popovers, so the panel opens in the side panel.
- **Toggle after a manual hide.** Krad ADE does not notify extensions when you only hide the side panel. After hiding it by hand, the first toggle closes the hidden view and the second shows it again.
- **Polling, not events.** State refreshes every 1–30 seconds, so changes made in Spotify appear after a short delay.
- **Coarse bars.** The SDK has no progress or slider element, so the progress and volume bars are built from a grid limited to 12 columns. They move in twelfths and cannot be dragged.
- **Icon set.** The SDK has no pause or shuffle icon. Play/pause shows a *stop* icon while playing, and shuffle uses the *exchange* icon.
- **No "Like".** None of the platform interfaces lets the extension like a track (on macOS, `starred` is read-only).
- **Platform gaps.** See [Platform support](#platform-support).

## Development

```sh
cargo clippy --release --tests
cargo test --release

# Integration tests: need Spotify running and network access
cargo test --release -- --ignored --nocapture
```

To check that the other platforms still compile from one machine, install their standard libraries with `rustup target add` and run clippy for each target:

```sh
cargo clippy --release --tests --target x86_64-unknown-linux-gnu
cargo clippy --release --tests --target x86_64-pc-windows-msvc
```

This type-checks without linking, so it needs no cross linker. Producing binaries for another OS needs that OS's linker; build them on the target system with the install scripts.

After changes, run the install script and reload the extension in Krad ADE.

## License

MIT
