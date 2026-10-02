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
being zero reproduces the two-phase app exactly: same phases, same formulae, same
ring, same pace. `both_holds_off_reproduces_the_two_phase_app_exactly` asserts
that at every tenth of a second of a 4/6 cycle rather than assuming it, because
this is the one guarantee that must not break silently.

**The default is 4-7-8-1, and used to be 4-in/6-out.** The two-phase behaviour is
still a guarantee about *patterns with both holds off*, not about the default: it
used to be worded "the default equals the two-phase app", which made the two
statements indistinguishable and made the compat test fail for the wrong reason
when the default changed. Tests about the default name `Settings::DEFAULT`; every
other test builds its pattern through the `plain()` helper. Do not reach for
`DEFAULT` as "some valid pattern" — that is what coupled eleven tests to a
constant none of them were about.

Four decisions were open. Each is recorded here with its reasoning, because each
could reasonably have gone the other way.

**1. The two original rules measure the breath, not the pattern.** "At least 8
seconds per breath" and "exhale no more than twice the inhale" were both written
when the cycle *was* the breath. Reading them against the total cycle instead
would turn the 8-second floor into a floor on the whole pattern, and would let a
20-second hold satisfy a rule about how long a breath is — so a 3+3 pattern with
a hold would "pass" for the wrong reason. Both now apply to inhale-plus-exhale
alone, via `moving_seconds()`. Every two-phase pattern therefore validates
exactly as it did, and the only rules a hold can newly fail are the three new
ones. The pace readout does the opposite, and deliberately: it uses the **full**
cycle, because a held breath is not a breath. Box breathing reads "3.8
breaths/min", which is correct and is the point of the feature.

**2. Hold range: zero, or 1–20 seconds; and at most twice the inhale.** Zero is
never an error — it is the two-phase pattern, and it has to stay reachable
without clearing a box. Twenty seconds is the working ceiling of breath-hold
practice. The ratio needed the most thought, because two readings disagree:
`hold <= inhale` is what box breathing actually prescribes, but it makes 4-4-4-4
— the most widely prescribed retention pattern there is — *unreachable*, since a
4-second hold would be the cap and the inhale is also 4. Two-to-one keeps box
breathing available and still bounds the hold to a third of a 15-second cycle.
Both holds are measured against the **inhale**, not the exhale: a hold is not a
kind of exhale, and with nothing else to compare against the exhale would be the
only candidate.

**3. A hold gets its own colour and one note of its own.** Reusing the inhale
tone was the cheap option and it is wrong — a hold after the exhale would sound
exactly like the inhale about to follow, which is the one moment where telling
them apart matters most. The hold tone is 587 Hz, a third above the exhale and a
fourth below the inhale, so the three are heard as one scale; both holds share
it, because from the inside they are the same instruction: wait. The hold
*colour* is a desaturated slate that is neither of the moving colours, so "I am
not breathing right now" is visible without reading the label.

**4. A hold is the absence of movement.** The orb holds the size the movement
before it left — full after an inhale, at rest after an exhale — and does not
drift. The ring follows the same rule, which means a hold is motionless in its
*entirety*, not just in the orb: the top hold parks the arc at full, the bottom
hold leaves it at empty, and only the colour changes. This was not the first
design. The bottom hold used to close the ring's arc, sweeping from full back
to empty across its whole length, on the reasoning that four phases should read
as one continuous sweep. That made the bottom hold the only phase in which the
user watches a bar drain while being told to hold still, and a hold that empties
the ring is a hold the eye reads as a slow exhale. The discontinuity it avoided
is now paid instead at the top of the next inhale, where the empty ring meets
its first sliver of fill — a smaller event, once per cycle, at the boundary the
user is watching anyway. `data-holding` tells the stylesheet to drop the orb's
size easing, because under `prefers-reduced-motion` an orb easing between two
sizes it is not visiting reads as a drift rather than as stillness. The rule is
asserted over every tenth of every hold, not at its endpoints, by
`no_hold_moves_anything_on_the_dial`: a sweep is a thing that is *nearly* static
at its ends, and sampling the ends is exactly how the old one got through.

Verified in a browser, not just in unit tests: all four phases screenshotted in
both themes, and the cues read out of `AudioParam.setValueAtTime` across a full
16-second box cycle — 740/1480, 587/1174, 392/784, 587/1174, 740/1480.

## Tests

Two kinds, both under plain `cargo test`. No browser, no Node, no Chromium.

- **Unit tests in `src/pacer.rs`** cover the app's actual decisions: the phase
  at any instant in a cycle, the ring sweep and orb scale through both halves,
  the boundary between them, the cycle's repetition, the whole validation rule
  set and the order the rules fire in, the pace readout's one-decimal rounding,
  and the persisted shape's round trip and its fallbacks. `src/pacer.rs` never
  mentions `web-sys`, which is why they run anywhere.
- **`tests/shell.rs`** asserts the invariants of the committed shell and of what
  is and is not committed — see below.

There is no test on `dist/`, deliberately. It is gitignored, so the release
gate's exported tree never has it and cannot build it (that needs the wasm target
and the pinned generator); a test asserting on it would run only in a
developer's checkout, which is exactly where it is least likely to catch
anything.

What covers the built output is `.github/workflows/build.yml`, which runs the
two build steps from a clean checkout and then inspects the result: all eight
files present and non-empty, nothing unexpected, no unsubstituted
`__VERSION__`, bindings that still export, a manifest of the right shape — and
that **every file the service worker precaches is one the build publishes**.

That last check is not decoration. This repository shipped a seven-file `dist/`
once: `ui.html` asked for `./icon.svg` and the worker precached `icon.svg`, but
`build.rs` only ever rasterized the PNGs. The favicon 404ed for everyone, and
because `caches.addAll` rejects an entire install if any one URL 404s, the site
silently got **no service worker and no offline support at all**. The symptom
looks like a caching bug. The test derives its expectation from
`src/service-worker.js` and `build.rs` rather than from a hardcoded list of
eight names, because a second copy of the truth is how the omission arose.

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

Both are noted here because they are decisions, not accidents.

**The `parseInt` truthiness quirk is gone.** The original read stored settings
with `storedSettings.inhaleSeconds || DEFAULT.inhaleSeconds`, which means a
stored `0` falls back to the default — but if a zero were *typed*, the value was
adopted and `validateSettings` was then called on it, and `0 < 3` fails, so the
pattern was rejected and never saved. Kept faithfully, that quirk would mean
carrying an unusable zero through `Settings`; instead the fields are `u32` and a
stored zero is a parse-level fallback. The user-visible behaviour is identical.
A related case *is* preserved, because it is user-visible: a box the user has
emptied reads as `None` and the pattern is left alone, so a half-typed value
never freezes the orb.

**Each duration box reads only itself.** The original bound one
`handleInputChange` to both boxes, and it read both. Called once per box, that
adopts the half-typed value in the *other* box at the same moment and persists
it — so editing `exhale` alone would commit whatever was mid-edit in `inhale`.
The handler here is told which box changed. The render pass fills the other in
immediately, so the two are equivalent a moment later and only differ in the
instant the change is committed.

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
all is that the compiler error is unhelpfully indirect:

- The feature list in the shared spec is **not sufficient**. Beyond
  `AudioContext`, `AudioContextState`, `OscillatorNode`, `OscillatorType`,
  `GainNode` and `AudioScheduledSourceNode`, the calls this app makes need
  `AudioParam` (for `oscillator.frequency` and `gain.gain`), `AudioNode` (for
  `connect`) and `AudioDestinationNode` (for `AudioContext::destination`).
  Each is separately `#[cfg]`-gated and none is implied by the six above.
- `Navigator` is needed too, for `Window::navigator` — the property is on every
  window in every browser, but `web-sys` gates even the getter behind the
  feature, and the error says only "no method named `navigator` found for struct
  `Window`", which reads like a missing dependency rather than a feature.
- `Element::style` **does not exist**. `style()` is on `HtmlElement` and there is
  no `Deref` from `Element`, so custom properties need an explicit
  `dyn_into::<HtmlElement>()`.
- `AudioNode::connect` is generated as `connect_with_audio_node`, not `connect`,
  because `AudioNode` has five overloads of it.
- `Window::set_interval_with_callback_and_timeout_and_arguments_0` takes
  `&Function` (not `Option<&Function>`) and an **`i32`** timeout. Both are
  signature mismatches that read as "method not found".
- `Navigator::service_worker()` returns a `ServiceWorkerContainer`, not an
  `Option`, and `ServiceWorkerContainer::register` returns a `js_sys::Promise`
  that does **not** throw on failure — a non-secure context rejects it, so the
  rejection has to be caught or the app logs an unhandled rejection. Note
  `Promise::catch` is typed to take `&ScopedClosure` directly, the one place in
  this crate where a closure is passed instead of a function reference.
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
