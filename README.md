# Breath

A paced-breathing pacer. Inhale for 3–10 seconds, exhale for 3–12, and a ring
and orb keep the count — with an optional tone at each phase change.

The whole app is Rust: the phase, the pacing, the validation, the audio cue and
the saved settings. `cargo build` writes a publishable static site into `dist/`.
There is no server and no account, and nothing leaves your machine.

A rewrite of [`wdomitrz/breath`](https://github.com/wdomitrz/breath), whose
original JavaScript history is kept in this repository's early commits.

## Build

Two steps, because there are two targets. Nothing generated is committed.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128 --locked

# 1. the site: compile to wasm and generate the JS bindings
cargo build --locked --lib --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir dist --out-name app \
  target/wasm32-unknown-unknown/release/breath.wasm

# 2. the rest of the site
touch build.rs
cargo build --release --locked
```

That gives you a publishable `dist/`. **The order matters**: step 2 derives the
service worker's cache name from the wasm, so running it first would pin that to
the previous build. The `touch` matters too — `build.rs` writes into the source
tree, not `OUT_DIR`, so cargo will not re-run it for a second identical
invocation.

`cargo test --locked` needs none of the above — it runs without the wasm target
or the bindings generator.

## Use it

`dist/` is self-contained and everything in it is relative, so it mounts under
any path. Point any file host at it — nginx, Caddy, GitHub Pages,
`python3 -m http.server` — and open the result.

Set an inhale and an exhale in whole seconds. A cycle must be at least 8 seconds
and the exhale at most twice the inhale; anything else is refused with a reason
and the last good pattern keeps running. The pace readout is breaths per
minute, to one decimal when it is not whole. Your pattern is remembered; the
cycle always starts fresh on load, so you never resume mid-breath.

Press **Enable sound** once to arm the audio — browsers require a gesture —
and each phase change will sound a note: a rising 740 Hz for the inhale, 392 Hz
for the exhale, each doubled by a triangle an octave up.

## In CI

`.github/workflows/build.yml` runs the two steps in that order on every push to
`master` and every pull request, then checks the site they produced, runs
`cargo test` and both clippy passes, and uploads `dist/` as an artifact. It
publishes nothing.

It fetches the pinned generator as the official prebuilt binary rather than
`cargo install`ing it, and verifies the download twice: against the checksum
published with the release, and against a digest recorded in the workflow
itself, which is what catches a re-uploaded release.

The check on `dist/` is the part worth knowing about. `dist/` is gitignored, so
no test can assert on it — the exported tree the release gate uses does not
have it, and cannot build it. The workflow therefore inspects the result
directly: all eight files present and non-empty, nothing unexpected, the service
worker's version placeholder substituted, the bindings carrying their exports,
the manifest well formed — and that every file the service worker precaches is
one the build actually publishes.

That last one is earned. This repository shipped a seven-file `dist/` once: the
page asked for `./icon.svg` and the worker precached it, but the build only ever
rasterized the PNGs. The favicon 404ed, and because `caches.addAll` rejects an
entire install if any one URL 404s, the site silently ended up with no service
worker and no offline support at all — which reads as a caching bug rather than
a missing file.

### Gotchas

- `wasm-bindgen` installs to `~/.cargo/bin`, which is **not** on `PATH` in a
  non-login shell. Add it, or call it by absolute path.
- The generator version is pinned to `=0.2.128` and must match `Cargo.toml`. A
  mismatch produces bindings the page won't load, and it fails with "Could not
  load the breathing pacer".
- After changing anything in `src/pacer.rs` or `src/ui.rs`, run all four commands
  again. The wasm is not committed, so nothing reminds you — and a `dist/` built
  from a stale one ships a pacer that predates your change, silently.

## Layout

    src/pacer.rs         the app's decisions: cycle, validation, pace, storage
    src/ui.rs            the browser: DOM, the 80 ms tick, the audio cue
    src/ui.html          the static shell
    assets/icon.svg      the icon; build.rs rasterizes the install PNGs from it
    build.rs             writes the site into dist/
    dist/                build output — the site itself, gitignored
    tests/shell.rs       invariants of the shell and of what is committed

`AGENTS.md` documents the design decisions, the icon, the two deliberate
departures from the original app, and the `web-sys` and build traps found on the
way.

## Licence

AGPL-3.0-only. See `LICENSE`.

Inspired by [breathe-cli](https://github.com/wdomitrz/breathe-cli/).
