# breath

Breath: a paced-breathing pacer. Inhale, hold, exhale, hold — up to four
phases in a cycle — and a ring and orb keep the count. All of it — the phase, the pacing, the
validation, the audio cue, the persistence — is Rust. `cargo build` generates
the **static PWA** into `./dist`: a front-end-only site any file host can
serve, with no server and no runtime dependency on a binary.

AGPL-3.0-only. See `LICENSE`.

This is a standalone repository. It is the Rust rewrite of the public
[`wdomitrz/breath`](https://github.com/wdomitrz/breath), whose original
HTML/CSS/JS history is preserved here in the early commits. The rewrite keeps
that app's behaviour, wording, palette and layout; it replaces its
implementation, and the files `app.js`, `style.css`, `sw.js`, `manifest.json`,
`index.html` and `icon.svg` are deleted by it.

## Build and run

Two builds, because there are two targets. Nothing generated is committed.

```
# 1. the site: compile the crate to wasm and run the bindings generator
rustup target add wasm32-unknown-unknown
cargo build --locked --lib --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir dist --out-name app \
  target/wasm32-unknown-unknown/release/breath.wasm

# 2. the rest of the site
touch build.rs
cargo build --release --locked
```

Step 1 writes `dist/app.js` and `dist/app_bg.wasm`; step 2 adds the six files
`build.rs` owns — `index.html`, `service-worker.js` with its version
substituted, `icon.svg`, the two rasterized icons, and the manifest — and
derives the service worker's cache version.

**The order matters, twice over.** `build.rs` derives the cache version from the
bytes of every other file in `dist/`, the wasm included, so running it first
would pin the version to whatever the previous build left behind. And the
`touch build.rs` is not redundant: `build.rs` writes into the source tree rather
than `OUT_DIR`, so cargo cannot see that anything changed and will not re-run it
for a second, otherwise identical invocation — which leaves a half-built site
holding the two wasm artefacts and none of the shell files.

There is **no run step and no server**: the build is the whole story.

`wasm-bindgen` installs to `~/.cargo/bin`, which is on `PATH` in a normal login
shell; in a bare or non-login shell call it by absolute path
(`~/.cargo/bin/wasm-bindgen`). The version is pinned to `=0.2.128` in
`Cargo.toml` and must match the CLI exactly — a mismatched generator produces
bindings the runtime will not load, and the page then fails with "Could not load
the breathing pacer". No npm or JS build tool is needed.

## The static site

The pacer is a browser application; there is no server and no service. `dist/`
is its whole form, and nothing in it is committed.

The eight files arrive from two builds:

- `app.js` and `app_bg.wasm` are written by `wasm-bindgen` (step 1 above) into
  `dist/`. They are the pacer itself and exist nowhere else in the tree.
- The other six are published by `build.rs` during step 2: `src/ui.html`, the
  committed `assets/icon.svg`, the two PNGs rasterized from it, the assembled
  manifest, and `service-worker.js` with its cache name pinned to a version
  derived from the bytes of every other file in the directory **and its own
  source** — so changing the caching logic invalidates the cache too, and
  changing the wasm moves the version.

`build.rs` writes only the files it owns, each under a scratch name and renamed
into place, so a host serving `dist/` never sees a half-written file. It does
not replace the directory, because the wasm step owns two files in there.

Everything the shell references is relative (`./app.js`,
`new URL('./', self.location.href)`, `start_url: "./"`), so one build works from
any subdirectory. Any file host can publish it: nginx, Caddy, GitHub Pages,
`python3 -m http.server`.

`dist/` is gitignored. It is reproducible: the same sources and the same pinned
toolchain produce the same bytes.

## The icon

`assets/icon.svg` is **redrawn**, which is the one liberty this repository took
and the reason it is worth explaining.

The original mark was a filled teardrop over a filled lozenge in `#434343`, and
it was the same glyph the other five apps in this family ship — a Material
Symbols shape in a neutral grey. Here the family brief was to match the apps'
shared colour scheme and simplicity, so the mark was redrawn to two elements in
one colour: a **ring**, and the **orb inside it**.

The reasoning, in the order that decided it:

- **It is the app's own picture.** The pacer's entire visual is a conic-gradient
  ring with a circle scaling inside it. An icon that is a ring and a disc is the
  app at rest, at a size where no animation is needed to read it.
- **It is the theme colour.** `#0f766e` is the manifest's `theme_color`, the
  `:root` `--inhale`, and the page's `<meta name="theme-color">` — so the icon
  and the window chrome around it are one colour. The `#434343` grey on a teal
  app read as an unrelated app.
- **Two shapes, not a glyph.** A stroked ring plus a filled circle survives
  being scaled to 48 px in a launcher, a 32 px list row, or a maskable circle
  where the corners are cropped. No interior detail, no strokes thinner than
  48/512 of the viewBox, nothing that dies at small sizes.
- **One colour, transparent ground.** Because `build.rs` rasterizes a
  transparent PNG, a second colour would be the only thing distinguishing the
  mark from a silhouette of itself. `tests/shell.rs` asserts exactly one hex in
  the file.

It is still the committed, authoritative, hand-editable source: `build.rs`
derives `icon-192.png` and `icon-512.png` from it with `usvg`/`resvg`, and those
PNGs are build output that is never committed. The SVG itself is *also* copied
into `dist/`, because the shell asks for it as the favicon and the worker
precaches it — see the trap below.

## The four phases

A cycle is **inhale → hold → exhale → hold**. Either hold may be zero, and both
being zero reproduces the two-phase app exactly: same phases, same formulae,
same ring, same pace. `src/pacer.rs` owns that guarantee and its tests enforce
it; the default is 4-7-8-1 and used to be 4-in/6-out — see `Settings::DEFAULT`
and the tests' `plain()` helper there for why, and for why only tests about the
default name `DEFAULT`.

Four decisions were open. Each is recorded with its full reasoning at the code
that implements it, because each could reasonably have gone the other way:

1. **The two original rules measure the breath, not the pattern.** See
   `Settings::validate` and `Settings::moving_seconds` in `src/pacer.rs`.
2. **A hold is zero, or 1–20 seconds, at most twice the inhale.** See
   `MIN_HOLD_SECONDS` and `MAX_HOLD_INHALE_RATIO` in `src/pacer.rs`.
3. **A hold gets its own colour and one note of its own.** See `phase_colour`
   in `src/ui.rs` and `Phase::cue_frequency` in `src/pacer.rs`.
4. **A hold is the absence of movement.** See `Settings::phase_at` in
   `src/pacer.rs`.

Verified in a browser, not just in unit tests: all four phases screenshotted in
both themes, and the cues read out of `AudioParam.setValueAtTime` across a full
16-second box cycle — 740/1480, 587/1174, 392/784, 587/1174, 740/1480.

## Tests

Two kinds, both under plain `cargo test`, with no browser, no Node and no
Chromium: unit tests in `src/pacer.rs`, and `tests/shell.rs` — the executable
specification of the committed shell's invariants; read it. It also records why
nothing asserts on the gitignored `dist/` (`.github/workflows/build.yml` runs
the two builds and inspects that instead) and the shipped-seven-file-`dist/`
favicon bug behind its precache test.

## Code map

- `pacer.rs`: the whole application, minus the DOM. `Settings` (the two
  durations, their validation, the pace readout), `Phase` (the name, the ring
  sweep, the orb scale, the cue frequency), `Pacer` (the settings plus the
  cycle's start time), and the `localStorage` shape. Every constant here —
  `STORAGE_KEY`, `TICK_MS`, `MIN_CYCLE_SECONDS`, the cue envelope — is in this
  file, and is what the unit tests assert on.
- `ui.rs`: wasm-only `web-sys` DOM, the 80 ms tick, the two duration inputs,
  the audio cue, and the service worker registration. It decides nothing: it
  moves `pacer`'s answers into the page.
- `ui.html`: the app shell. `build.rs` copies it into `dist/index.html` byte for
  byte, and `tests/shell.rs` asserts against the source, which is therefore the
  same thing.
- `service-worker.js`: caches only a fixed app-shell allowlist, scope-specific
  content-versioned cache, atomic install, no `skipWaiting`. Offline needs one
  successful online visit over HTTPS or localhost. Clearing site data removes
  offline support.
- `build.rs`: writes the six files it owns into `dist/`, rasterizing the icons
  from `assets/icon.svg`, assembling the manifest, and deriving the worker's
  content-derived cache version. It leaves `dist/app.js` and `dist/app_bg.wasm`
  to the `wasm-bindgen` step, so it writes files in place rather than replacing
  the directory.

## Two deliberate departures from the original

Both are noted here because they are decisions, not accidents; both stories are
kept with the code that embodies them:

- **The `parseInt` truthiness quirk is gone** — see `parse_whole_seconds` in
  `src/pacer.rs`; the related emptied-box behaviour that *is* preserved is on
  `Field::read` in `src/ui.rs`.
- **Each duration box reads only itself** — see the `Field` enum in `src/ui.rs`.

## Verification

```
cargo clippy --all-targets -- -D warnings
cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings
cargo test --locked
```

Both clippy passes are required. The crate denies warnings, but only for the
target being compiled, and almost all of the interesting code here is
`wasm32`-only — so a clean host build says very little about the one that
matters.

### `web-sys` notes, for anyone doing the same job

Verified against `web-sys 0.3.105`, and the reason several of them are here at
all is that the compiler error is unhelpfully indirect. Four of the notes live
with the code that needed them: the audio-feature forensics in `Cargo.toml`,
the `Element::style` cast on `styled()` and the `connect_with_audio_node` name
on `play_cue` in `src/ui.rs`, and the does-not-throw `register` promise on
`register_service_worker` in the same file. The rest:

- `Navigator` is needed too, for `Window::navigator` — the property is on every
  window in every browser, but `web-sys` gates even the getter behind the
  feature, and the error says only "no method named `navigator` found for struct
  `Window`", which reads like a missing dependency rather than a feature.
- `Window::set_interval_with_callback_and_timeout_and_arguments_0` takes
  `&Function` (not `Option<&Function>`) and an **`i32`** timeout. Both are
  signature mismatches that read as "method not found".
- `tiny_skia::Pixmap::new` returns `Option`, not `Result` — so an
  `unwrap_or_else(|error| ...)` closure does not compile, and the error is the
  confusing "closure is expected to take 0 arguments".
- `clippy::approx_constant` fires on `3.141_592` in a *test*, so a test that
  wants to check one-decimal rounding of a familiar value must pick another one.
  `60.0 / 19.0` works.

## Known limitations

- The pacer runs entirely in one `Rc<RefCell<App>>` shared by three listeners and
  one 80 ms timer. Every borrow is taken and released inside a single callback
  with no DOM work and no re-entry in between; `try_borrow_mut` is used so a
  re-entrant event is skipped rather than panicking the page.
- The service worker has no `skipWaiting`, so an update waits for old tabs to
  close. That is deliberate — swapping the wasm under a live session would
  change the pacer mid-breath — but it does mean a second tab can pin the old
  version until it is closed.
- Audio cannot be armed without a gesture, by browser policy. The button is
  therefore required, disabled once pressed, and the app is fully usable
  silent.
