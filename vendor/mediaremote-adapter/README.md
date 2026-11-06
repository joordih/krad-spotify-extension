# mediaremote-adapter (vendored)

A minimal copy of [ungive/mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)
at commit `0e82bf1c44ec39554fbfc402ec88a48c6ae2a0fd` (2026-10-17),
Copyright (c) 2025, Jonas van den Berg and contributors, licensed under the
BSD 3-Clause License in [`LICENSE`](LICENSE). That copyright notice and license
must ship with every build of this extension that embeds the adapter.

It lets the macOS backend read and control the system *Now Playing* client
through the private MediaRemote framework: `/usr/bin/perl` is entitled to use
MediaRemote, so `bin/mediaremote-adapter.pl` loads the helper dylib built from
`src/` into a `perl` process. See `docs/adr/0001-hybrid-macos-backend.md`.

What was copied, unmodified:

- `LICENSE`
- `bin/mediaremote-adapter.pl`
- `include/MediaRemoteAdapter.h`
- `src/adapter/` except `speed.m` and `test.m` (the playback speed command and
  the self-test, which needs the upstream test client, are not used)
- `src/private/`, `src/utility/`

`build.rs` compiles these sources into a universal, ad-hoc signed dylib in
place of upstream's CMake framework build; the extension writes it to
`MediaRemoteAdapter.framework/MediaRemoteAdapter` in its cache directory, the
layout the perl script expects.
