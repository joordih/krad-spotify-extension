# Glossary

## Source

One app, or one browser session, that plays media on this computer, such as Spotify, Apple Music, VLC, or a browser tab playing a video. A source has a stable identity, a display name taken from its app, and its own Capabilities.

_Avoid_: player, session, app (when the playing thing is meant)

## Active Source

The one Source that the popover, the titlebar buttons, and the commands show and control at any moment. Unless a Pinned Source is set, it is chosen automatically: the source that most recently started playing, or the last active source when nothing is playing.

_Avoid_: current player, focused source

## Pinned Source

A Source the user has chosen to keep as the Active Source, whether or not it is playing. A pin lasts until the user changes it, the source goes away, or the extension restarts; then the Active Source is chosen automatically again.

_Avoid_: selected source, locked source

## Capabilities

The set of controls a particular Source supports: seeking, shuffle, repeat, its own volume, and opening its app. Controls a source lacks are not offered for it.

_Avoid_: features, platform support

## Ignored Apps

A user setting listing app names whose Sources are never shown, chosen, or controlled. An entry excludes every source whose app name contains it, or whose identity has it as a whole part, ignoring case.

_Avoid_: blocklist, excluded players

## Backend

The part of the extension that discovers Sources and controls them through one operating system's media interfaces. There is exactly one backend per operating system.

_Avoid_: driver, adapter, platform layer
