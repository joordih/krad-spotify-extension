# Krad Spotify

A [Krad ADE](https://github.com/joordih/krad-ade) extension that adds a Control Center–style *Now Playing* popover for the local Spotify desktop player to Krad ADE's titlebar. Open it from the titlebar or a shortcut, or drive Spotify from the command palette.

It also shows and controls Apple Music, media playing in a browser (YouTube and other sites), and other media players on this computer. Each of them is a *source*; when several play, the popover follows the one that started last, or you pick one with its chip.

Built in Rust on [`krad-extension-sdk`](https://crates.io/crates/krad-extension-sdk) 0.4.0 (extension API 1.1). It talks to the Spotify desktop app through each operating system's own media interface, so it needs no Spotify account credentials, OAuth flow, or Web API access.

## Platform support

| Platform | Backend | Status |
| --- | --- | --- |
| macOS | AppleScript (`osascript`) for Spotify and Music; MediaRemote for other apps ⁴ | Tested |
| Linux | MPRIS over D-Bus | Builds; not yet tested against Spotify |
| Windows | System media transport controls (`Windows.Media.Control`) | Builds; not yet tested against Spotify |

Features available on each platform:

| Feature | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Sources | Spotify, Music, and the app in Control Center's *Now Playing* (browsers, QuickTime Player, VLC…) ⁴ | Every MPRIS player: Spotify, VLC, Rhythmbox, Cider, Chrome/Chromium, Firefox… | Every app with a media session: Spotify, Apple Music, Media Player, browsers, VLC… |
| Apple Music | Music.app | Cider, or the browser playing music.apple.com | The Apple Music app from the Microsoft Store |
| YouTube and other web media | Browsers, through MediaRemote ⁴ | Browsers, through MPRIS | Browsers, through their media session |
| Track details and artwork | ✓ | ✓ | ✓ (from the media session thumbnail) |
| Play/pause, next, previous | ✓ | ✓ | ✓ |
| Timeline and seeking | ✓ | ✓ ¹ | ✓ |
| Shuffle and repeat | ✓ ⁵ | ✓ ¹ ⁵ | ✓ ⁵ |
| Volume | Spotify, Music | ✓ ¹, not browsers | — ² |
| Open on the current track | ✓ ⁶ | ✓ ⁶ | Opens Spotify ³ ⁶ |

1. These calls follow the MPRIS specification, but some Spotify versions report a position of 0 or ignore seeking and changes to shuffle, repeat, or volume.
2. Windows' media controls have no per-app volume, so the volume tile and commands are hidden on Windows.
3. The Windows media session does not expose the track ID, so Spotify opens on whatever it is showing.
4. MediaRemote is the private framework behind Control Center's *Now Playing*. The extension reaches it through `/usr/bin/perl` with the embedded [mediaremote-adapter](#license) (see [ADR 0001](docs/adr/0001-hybrid-macos-backend.md)). It only reports the one app Control Center shows, and being private, a macOS update may break it; the extension then keeps working with Spotify and Music.
5. Every source has its own capabilities: Spotify and Music support everything; other sources only offer seeking, shuffle, repeat, their own volume, or opening their app when their media interface reports it. Browsers usually offer play/pause, next, and previous only.
6. For Spotify; other sources bring their app to the front where the platform allows it.

## Features

- **Titlebar buttons:** a play/pause button and a button that opens the *Now Playing* popover. The popover button's tooltip names the current track and it is highlighted while music plays; play/pause hides while no source is available.
- **Control Center layout:** a popover made of rounded tiles.
- **Source selector:** when several sources are available, a row of chips shows them; the highlighted one is the active source. Click a chip to pin that source, or *Auto* to follow whatever started playing last. **Spotify: Next Source** steps through them from the keyboard.
- **Now playing:** large rounded album artwork, track title, artist, album, track number, and album artist (when it differs from the track artist).
- **Timeline:** a slider with elapsed and remaining time that advances on its own while playing. Drag it to seek.
- **Playback state:** a status badge for *playing*, *paused*, or *stopped*.
- **Transport controls:** previous, play/pause, and next.
- **Playback modes:** shuffle and repeat toggles, highlighted while the mode is on.
- **Volume:** a draggable slider with its percentage, plus −/+ step buttons.
- **Open in Spotify:** brings Spotify (or the active source's app) to the front.
- **Commands and shortcuts:** every action is in the command palette and has a shortcut inside Krad ADE.
- **Settings:** polling interval, volume step, an option to turn artwork off, and apps to ignore, changeable without restarting.

Features a platform or a source does not support are hidden from the panel (see [Platform support](#platform-support)).

## Requirements

| Component | Version |
| --- | --- |
| OS | macOS, Linux (x64 or arm64, with a D-Bus session bus), or Windows 10/11 (x64 or arm64) |
| Spotify | Desktop app, installed and running (or Apple Music, a browser, or another supported media player) |
| Krad ADE | 0.3.2 or later (extension API `1.1`) |
| Rust toolchain | Stable, for building from source |
| `curl` | For artwork downloads: `/usr/bin/curl` on macOS, `curl` on `PATH` on Linux, and on Windows 10/11 the preinstalled `System32\curl.exe`, found on `PATH` |
| `perl` | macOS only, for MediaRemote: `/usr/bin/perl`, preinstalled with macOS |
| Xcode Command Line Tools | macOS only, to build from source (`build.rs` compiles the MediaRemote adapter) |

## Installation

### From GitHub (recommended)

1. In Krad ADE, open the extension manager and add the repository URL:

   ```
   https://github.com/raloonsoc/krad-spotify-extension
   ```

2. Trust and enable **Spotify**.

Krad ADE clones the default branch (`main`) into `~/.config/krad-ade/extensions/spotify`. On first start it downloads the binary for your platform from the latest GitHub Release, checks it against the SHA-256 in `extension.toml`, and keeps it in its program cache. Nothing is built on your machine. **Update** in the extension manager pulls `main` and asks for trust again.

### From a local folder (development)

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

   They also point `extension.toml` at the binaries in `bin/` (`scripts/binaries.py local`), because after a release it points at the release downloads. Do not commit that change; `git checkout extension.toml` restores the release manifest.

   Cross-architecture builds need the matching Rust standard library, available through `rustup target add <target>`. The Homebrew Rust package only includes the host target.

2. In Krad ADE, open the extension manager, add the project folder, and enable **Spotify**. The extension must be able to write to the folder, because it creates the `cache` link there.

Krad ADE refuses extension folders over 256 MB and asks for trust again whenever a file in the folder changes. `.cargo/config.toml` therefore builds into `../krad-spotify-extension-target`, next to the project, rather than into `target/` inside it.

### Binaries

`extension.toml` declares one binary per platform, between the `# BEGIN BINARIES` and `# END BINARIES` markers, and Krad ADE picks the one for the current platform. Released manifests download them from the GitHub Release (`url` + `sha256`); local manifests use `bin/`:

| Platform | Binary |
| --- | --- |
| macOS (Apple Silicon / Intel) | `krad-spotify-darwin-arm64` / `krad-spotify-darwin-x64` |
| Linux (x64 / arm64) | `krad-spotify-linux-x64` / `krad-spotify-linux-arm64` |
| Windows (x64 / arm64) | `krad-spotify-win-x64.exe` / `krad-spotify-win-arm64.exe` |

## Usage

The extension adds two buttons to the right of Krad ADE's titlebar, next to the ADE/Editor switch:

- **▶ Play / pause** toggles playback. Its icon follows the playback state, and it hides while no source is available.
- **⌄ Now Playing** opens the popover. Its tooltip shows the current track, and it is highlighted while music plays.

The popover also opens and closes with the *Show / hide Now Playing* shortcut or **Spotify: Show / Hide Now Playing**, and closes with `Esc`. It stacks these tiles, from top to bottom:

1. **Sources:** an *Auto* chip and one chip per source, shown only when more than one source is available.
2. **Now Playing:** a header with the playback status badge, the source's name, and an *Open* button for its app; the artwork beside the track details; the timeline slider; and the transport controls, flanked by the **Shuffle** and **Repeat** toggles, which are highlighted while the mode is on.
3. **Volume:** a volume slider between −/+ buttons, for sources with their own volume.

Controls the active source does not support are left out.

Drag the timeline to seek, or the volume slider to set the volume; the change is sent when you release it (or with the arrow keys while the slider has focus). While the popover is open it refreshes at the configured interval; the timeline moves smoothly in between. When nothing is playing, the popover says so and its controls are disabled. The extension never launches Spotify, or any other media app, by itself.

### Commands and keyboard shortcuts

All actions are in the command palette under the `Spotify:` prefix and act on the active source.

| Action | Command ID | macOS | Linux and Windows |
| --- | --- | --- | --- |
| Show / hide Now Playing | `toggle-panel` | `⌃⌥⌘ P` | `Ctrl+Alt+Shift+P` |
| Play / pause | `playpause` | `⌃⌥⌘ Space` | `Ctrl+Alt+Shift+Space` |
| Next track | `next` | `⌃⌥⌘ →` | `Ctrl+Alt+Shift+→` |
| Previous track | `previous` | `⌃⌥⌘ ←` | `Ctrl+Alt+Shift+←` |
| Volume up | `volume-up` | `⌃⌥⌘ ↑` | `Ctrl+Alt+Shift+↑` |
| Volume down | `volume-down` | `⌃⌥⌘ ↓` | `Ctrl+Alt+Shift+↓` |
| Toggle shuffle | `shuffle` | `⌃⌥⌘ S` | `Ctrl+Alt+Shift+S` |
| Toggle repeat | `repeat` | `⌃⌥⌘ R` | `Ctrl+Alt+Shift+R` |
| Open in Spotify | `open` | `⌃⌥⌘ O` | `Ctrl+Alt+Shift+O` |
| Next source | `next-source` | `⌃⌥⌘ N` | `Ctrl+Alt+Shift+N` |

*Next source* pins the source after the active one, then each following source in the order of the chips, and after the last one returns to automatic selection; a toast names the result.

The shortcuts are chosen not to collide with Krad ADE's built-in bindings. Krad ADE detects conflicts with other shortcuts and does not bind a combination twice. On Windows, the volume commands report that volume is not supported, as do commands the active source does not support.

## Configuration

These settings are declared in `extension.toml` and edited in Krad ADE's extension settings. The extension reads them at startup and again whenever they change. No restart is needed.

| Key | Default | Range | Description |
| --- | --- | --- | --- |
| `poll-interval` | `1` | 1–30 seconds | How often the media sources are read. It keeps the titlebar buttons and the open popover current. |
| `volume-step` | `10` | 1–50 | Volume points (out of 100) added or removed by the volume commands. |
| `show-cover` | `true` | on / off | Fetch and show the album artwork. When off, the extension makes no network requests. |
| `ignored-apps` | empty | text | Comma-separated app names never to show or control, e.g. `Chrome, Safari`. An entry ignores every source whose app name contains it, or whose id has it as a whole part (`vlc` in `org.mpris.MediaPlayer2.vlc`), ignoring case. |

- Values outside a range are clamped to the nearest limit.
- Missing, mistyped, or non-finite values fall back to the default.
- If Krad ADE cannot provide the settings, the extension keeps the defaults and logs a warning.

## Permissions

### macOS

The extension sends Apple Events to Spotify (`com.spotify.client`) and Music (`com.apple.Music`) through `/usr/bin/osascript`. The first query or action triggers macOS's **Automation** consent prompt, once for each app. Accept it.

The app that macOS lists as the requester depends on how Krad ADE launches the extension. If you denied the prompt, go to **System Settings → Privacy & Security → Automation** and enable Spotify and Music for that app.

Other apps are read through MediaRemote with `/usr/bin/perl`, which needs no prompt. The extension writes the adapter script and its helper library to `~/Library/Caches/krad-spotify/` and runs them from there.

No Accessibility, microphone, or music library permissions are needed. `/usr/bin/pgrep` is only used to check whether Spotify and Music are running.

### Linux

The extension connects to the D-Bus **session** bus and talks to every `org.mpris.MediaPlayer2.*` player, such as `org.mpris.MediaPlayer2.spotify`. No extra permissions are needed. Sandboxed Spotify packages (Flatpak, Snap) must be allowed to own their MPRIS name on the session bus, which they normally are. Opening a track uses `xdg-open` with a `spotify:` URI.

### Windows

The extension reads the media sessions from the system media controls. No extra permissions are needed. An app only appears there while it has a media session.

### Troubleshooting

| Symptom | Cause | Resolution |
| --- | --- | --- |
| View says nothing is playing | No media app is running (or, on Linux, none is on the session bus), or all are in `ignored-apps` | Start playback in Spotify or another media app, or check `ignored-apps`. |
| Automation error (`-1743`, macOS) | Automation permission denied | Grant it in **Privacy & Security → Automation**. |
| Timeout error (`-1712`, macOS) | Spotify or Music did not answer the Apple Event in time | Retry. Check that the app is responsive. |
| Only Spotify and Music appear (macOS) | MediaRemote is not available, for example after a macOS update | No fix in the extension; Spotify and Music keep working. |
| "Spotify refused the request" (Windows) | Spotify rejected a media control request | Retry; some controls are unavailable during ads or for some content. |
| Placeholder instead of artwork | Fetch failed, `curl` is missing, the extension folder is read-only, or `show-cover` is off | Check network access, `curl`, and folder permissions. |
| "The extension folder is larger than an extension may be" | Build output inside the folder (over 256 MB) | Delete `target/`; builds go to `../krad-spotify-extension-target`. |

Any other error is shown as-is in the view or as a toast.

## How it works

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Extension entry point: polling, titlebar buttons, popover lifecycle, view events, and commands. |
| `src/view.rs`, `src/view/` | Builds the popover's tiles, source chips, sliders, and buttons. |
| `src/settings.rs` | Reads the user settings. |
| `src/media.rs` | The shared source model, the `Backend` trait every OS implements ([ADR 0003](docs/adr/0003-backend-contract.md)), and capability-checked controls; `media/select.rs` chooses the active source and `media/ignore.rs` matches `ignored-apps`. |
| `src/media/macos.rs`, `src/media/macos/` | macOS backend: AppleScript through `osascript`, and MediaRemote through the embedded adapter. |
| `build.rs`, `vendor/mediaremote-adapter/` | On macOS targets, compiles the vendored adapter into the dylib the binary embeds. |
| `src/media/linux.rs` | Linux backend: MPRIS over D-Bus with `zbus`. |
| `src/media/windows.rs` | Windows backend: `Windows.Media.Control` with the `windows` crate. |
| `src/artwork.rs` | Fetches, resizes, rounds, and caches the album artwork. |

Only the backend for the target OS is compiled, and the Linux and Windows dependencies are target-specific.

### macOS

The backend uses terms from Spotify's scripting dictionary (`Spotify.app/Contents/Resources/Spotify.sdef`):

- **Track:** `current track`, `name`, `artist`, `album`, `album artist`, `track number`, `duration`, `artwork url`, `spotify url`.
- **Player:** `player state`, `player position`, `shuffling`, `repeating`, `sound volume`.
- **Commands:** `playpause`, `next track`, `previous track`.

Music is read the same way, in the same script (`shuffle enabled`, `song repeat`, `sound volume`, and its artwork's raw data). All properties are read in one script, and each one is guarded separately, so a missing value does not fail the whole read. Numbers are rounded inside AppleScript to avoid decimal separators that depend on the system locale.

Every other app comes from MediaRemote: the binary embeds the mediaremote-adapter perl script and the dylib built by `build.rs`, writes them to the cache directory, and keeps one `perl … stream` process running that reports *Now Playing* changes. Commands run as short one-off `perl` calls.

### Linux

The backend lists every `org.mpris.MediaPlayer2.*` player on the session bus and reads its `org.mpris.MediaPlayer2.Player` properties `Metadata` (`xesam:title`, `xesam:artist`, `xesam:album`, `xesam:albumArtist`, `xesam:trackNumber`, `mpris:length`, `mpris:artUrl`, `mpris:trackid`), `PlaybackStatus`, `Position`, `Shuffle`, `LoopStatus`, and `Volume`. It calls `PlayPause`, `Next`, and `Previous`, and writes `Shuffle`, `LoopStatus`, and `Volume`. Repeat toggles between `None` and `Playlist`.

### Windows

Every session the system media controls list is a source, identified by its app ID. The backend reads each session's media properties, playback info (including which controls the app accepts), and timeline. While playing, the position is advanced by the time since the timeline's last update. Controls use the session's `Try…Async` methods. Repeat toggles between `None` and `List`.

### Artwork

Krad ADE's view renderer only draws images bundled with the extension (`ImageSource::Bundled`). It does not clip images to rounded corners. `src/artwork.rs` works around both limits:

1. Gets the image: downloads the artwork URL with `curl` (HTTPS only, 5-second timeout), reads a local artwork file (such as a browser's `file://` art on Linux), or takes the image bytes the backend already holds (Music's artwork, MediaRemote, or the media session thumbnail on Windows).
2. Resizes it to 320 × 320 px, twice the 160 px display size so it stays sharp on high-DPI screens.
3. Makes the corners transparent, with anti-aliasing.
4. Saves it as a PNG in a per-user cache directory, reached through `cache` in the extension folder.

`cache` is a symbolic link (a junction on Windows) to `~/Library/Caches/krad-spotify` on macOS, `$XDG_CACHE_HOME/krad-spotify` (or `~/.cache/krad-spotify`) on Linux, and `%LOCALAPPDATA%\krad-spotify` on Windows. Krad ADE fingerprints every file in the extension folder to decide whether you trust it, and records only a link's target, so new covers do not ask for trust again. If no link can be made, the covers fall back to a plain `cache` directory. Only the current track's artwork is kept, and `cache` is git-ignored.

## Limitations

- **Local control only.** The extension controls the Spotify desktop app and the other media apps on this computer. It does not use Spotify Connect, the Web API, or OAuth.
- **Polling, not events.** State is read every 1–30 seconds, so changes made in Spotify itself (a new track, pausing) appear after a short delay. The timeline still moves smoothly in between.
- **Icon set.** The SDK has no pause, shuffle, or music icon. Play/pause shows a *stop* icon while playing, shuffle uses the *exchange* icon, and the popover button uses a *chevron*.
- **No "Like".** None of the platform interfaces lets the extension like a track (on macOS, `starred` is read-only).
- **Pins are not saved.** A pinned source is forgotten when Krad ADE restarts.
- **MediaRemote is private.** On macOS, apps other than Spotify and Music depend on a private framework that Apple can change without notice, and only the app Control Center shows is visible.
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

### Releasing

Push a version tag from `main`:

```sh
git tag v0.2.0
git push origin v0.2.0
```

The **Release** workflow (`.github/workflows/release.yml`) then:

1. Runs the tests and builds the binary for all six platforms on GitHub-hosted macOS, Linux, and Windows runners.
2. Creates the GitHub Release for the tag, with the binaries as assets.
3. Rewrites the binaries block and `version` in `extension.toml` with `scripts/binaries.py release` and pushes that commit to `main`.

From then on, installing or updating from GitHub uses the new binaries.

## License

MIT

The macOS backend embeds [mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) by Jonas van den Berg and contributors, licensed under the BSD 3-Clause License. The vendored copy and its [`LICENSE`](vendor/mediaremote-adapter/LICENSE) are in [`vendor/mediaremote-adapter/`](vendor/mediaremote-adapter/README.md).
