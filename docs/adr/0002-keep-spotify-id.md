# 2. Keep the extension's original identity: id "spotify" and name "Spotify"

Status: Accepted

Date: 2026-11-06

## Context

The extension is the original author's Spotify player, installed with the id `spotify` and shown as *Spotify*, with `Spotify:` command titles. Support for other media Sources (Apple Music, browsers and other media players) is contributed to it as an addition, not as a new extension. Krad ADE keys an extension's install folder, its stored settings, and its trust record by the id; command ids and setting keys are likewise what keybindings and saved values refer to.

## Decision

The extension keeps its original identity: the id `spotify`, the name *Spotify*, the view and titlebar titles, and the `Spotify:` command titles, including *Spotify: Open Current Track in Spotify*. Every existing command id and setting key stays as it was. The multi-source work only adds to it: a sentence in the description, the `next-source` command (*Spotify: Next Source*) with its shortcut, and the `ignored-apps` setting.

Rejected alternative: renaming the display name, view and titlebar titles, and command titles to *Now Playing* / `Media:` wording while keeping the id. It would describe the multi-source extension better, but it rebrands someone else's extension in a contribution, and that choice belongs to the original author.

## Consequences

- Existing installs update in place and keep their settings, shortcuts, and trust, and users see the extension they installed.
- The name and the `Spotify:` prefix undersell the extension, since its commands act on whichever Source is active. The README and the description say that other Sources are supported.
- A rename, if the author wants one later, can change the display strings without touching the id.
