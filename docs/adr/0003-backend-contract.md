# 3. One Backend per operating system behind a small, typed contract

Status: Accepted

Date: 2026-11-06

## Context

The extension shows and controls media Sources on macOS, Linux and Windows. Each operating system has its own media interfaces (AppleScript and MediaRemote, MPRIS over D-Bus, `Windows.Media.Control`), and they differ in what they report, how fast they answer and how they fail. The popover, the titlebar buttons, the commands, the choice of the Active Source and the Ignored Apps setting should not depend on any of that, and should be testable without a media app running.

## Decision

The shared `media` module owns the model (Source, Track, Capabilities, Artwork), the selection of the Active Source, the Ignored Apps filter, and a `Backend` trait with two methods: one lists every Source, the other runs one `Command` on one Source. Exactly one Backend is compiled, chosen by the target operating system:

| OS | Module | Mechanism |
| --- | --- | --- |
| macOS | `media/macos.rs` | AppleScript for Spotify and Music, MediaRemote for every other app ([ADR 0001](0001-hybrid-macos-backend.md)) |
| Linux | `media/linux.rs` | every MPRIS player on the D-Bus session bus |
| Windows | `media/windows.rs` | every `Windows.Media.Control` session |
| any other | `media/unsupported.rs` | nothing; every call fails with "not available" |

Each module exports one `Platform` type that implements `Default` and `Backend`. The poll thread and the command handlers share one instance, so it must be `Send + Sync`. Everything else in the module stays private to it.

Every Backend follows these rules:

- **Ids.** A Source's id is stable for as long as the Source exists: it does not change between listings or with the track. It is unique within one listing and only means something to the Backend that produced it (a bundle id, an MPRIS bus name, an AppUserModelId). Commands receive it back unchanged.
- **Listing.** The listing returns every Source that currently has a media session, playing or not, each with its track, playback state and Capabilities. Backends do not filter: the shared layer drops Sources that are stopped with nothing loaded, removes Ignored Apps and duplicate ids, sorts the rest by name, and picks the Active Source. Nothing running is an empty listing, not an error. A listing fails only when the Backend could not look at all; when one mechanism fails and another works, it returns what works.
- **Commands.** A command acts on the given Source only. The shared layer sends only commands the Source's Capabilities allow, with values already validated (a volume within 0–100, a finite position of at least zero), and turns relative changes such as a volume step into absolute ones from the latest listing. A Backend must still fail cleanly rather than panic. An id that no longer exists fails with "source gone"; a command the Source cannot run fails with "unsupported".
- **Speed.** Listings and commands run on the poll thread or a command handler and block it, so each should finish well within a second.
- **Artwork.** A Backend reports the track's artwork as an HTTPS URL, a local file, or image bytes already in hand. Bytes are keyed by their content, so an unchanged cover is not processed again.
- **Errors** are typed. Besides "source gone" and "unsupported", a Backend reports "nothing playing", a denied macOS Automation permission (naming the app), a timeout, an invalid value, "not available on this platform", or a platform failure with its detail. The popover and the toasts turn the ones the user can act on into advice, and show the others as they are.

## Consequences

- The shared layer, including selection, filtering and the capability checks, is tested with a fake Backend and no media app.
- Adding a platform means one new module that implements two methods; nothing else changes.
- A Backend that cannot support a control simply leaves its Capability off, and the popover hides it.
- Validated commands and typed errors mean the view never inspects error text, and Backends never repeat clamping or capability checks the shared layer already did.
- Because Backends report every session, a source that briefly disappears from one listing is tolerated by the shared selection instead of each Backend inventing its own debouncing.
