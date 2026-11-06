# 1. Hybrid macOS backend: AppleScript for Spotify and Music, MediaRemote for everything else

Status: Accepted

Date: 2026-11-06

## Context

The extension now shows and controls several media Sources, not only Spotify. On macOS there is no single public API that lists every app playing media and controls each one:

- **AppleScript** is public and stable, and Spotify and Music.app both have rich scripting dictionaries (track details, artwork, position, shuffle, repeat, their own volume, and opening the current track). It only covers apps that ship such a dictionary; browsers do not expose their media sessions to it.
- **MediaRemote** is the private framework behind Control Center's *Now Playing* module. It sees every app that publishes media information, browsers included. Since macOS 15.4 Apple only answers MediaRemote requests from processes holding a private entitlement. `/usr/bin/perl` holds it, so the *mediaremote-adapter* technique (BSD-3-Clause) loads a small helper dylib into a `perl` process and streams the state from there. Being private, the framework and that loophole can change or break with any macOS update.

## Decision

The macOS Backend combines both:

1. **AppleScript** (through `/usr/bin/osascript`) for Spotify and Music.app.
2. **MediaRemote**, loaded through `/usr/bin/perl` with the mediaremote-adapter technique, for every other Source. The perl script and the helper dylib are embedded in the extension binary and extracted to the extension's per-user cache directory on first use.

When both mechanisms report the same app, the Spotify or Music entry from AppleScript wins and the MediaRemote duplicate is dropped. A MediaRemote failure only removes the other Sources; Spotify and Music keep working.

### Alternatives rejected

- **MediaRemote only.** Simplest, but a single point of failure: a macOS update that closes the perl loophole would take away every Source, including the two that have a stable public API.
- **AppleScript only, with JavaScript injected into browsers.** Narrow (only scriptable browsers, only pages whose players the script understands), fragile against site changes, and it needs the user to allow JavaScript from Apple Events in each browser.
- **Depend on an external tool installed with Homebrew** (such as `media-control`). Adds install friction and a dependency the extension cannot pin or verify, for the same private API underneath.

## Consequences

- Spotify and Music.app keep their full feature set and stay reliable regardless of what happens to MediaRemote.
- Browsers and other apps appear as Sources, with whatever Capabilities MediaRemote offers for them (typically no own volume).
- The binary grows by the embedded script and dylib, and the extension writes them to its cache directory, which it already owns for artwork.
- MediaRemote may break on future macOS releases; when it does, the extension degrades to Spotify and Music only instead of failing.
- The embedded adapter is BSD-3-Clause, so its copyright notice and license must ship with the extension.
