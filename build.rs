// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! Write the static site to `dist/` while the crate compiles.
//!
//! `breath` is a browser application, so publishing it is a file copy, not a
//! program run. Six of the eight shell files are committed as they are; the
//! other two are derived here:
//!
//! * `icon-192.png` and `icon-512.png` are rasterized from the committed
//!   `assets/icon.svg`. The SVG is the icon's authoritative source and is never
//!   replaced by a PNG; the PNGs are build output and live only in `dist/`. The
//!   SVG itself is published too, because the page and the service worker both
//!   reference it and it has to resolve at runtime.
//! * `service-worker.js` carries a `__VERSION__` placeholder standing for a
//!   cache name derived from the bytes of every *other* file in `dist/`
//!   **including the two wasm artefacts**, which the `wasm-bindgen` step writes
//!   into `dist/` just before this one runs. That is the whole reason the two
//!   build steps have an order.
//!
//! The manifest is assembled from `serde_json` rather than committed as a file
//! of its own, so its icon list cannot drift from what was actually rasterized.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// The icon sizes a manifest must declare, rasterized from `assets/icon.svg`.
const ICON_SIZES: [u32; 2] = [192, 512];

/// The files this script owns, and the committed file each is copied from.
///
/// The two wasm artefacts are deliberately absent: they are written into
/// `dist/` by the `wasm-bindgen` step, which runs *after* a wasm build and
/// *before* this one. They are build output and are never committed, so they
/// have no committed source to copy from — and this script must not delete
/// them, because they are the app itself.
const SHELL: &[(&str, &str)] = &[
    ("index.html", "src/ui.html"),
    ("service-worker.js", "src/service-worker.js"),
    ("icon.svg", "assets/icon.svg"),
    // The SVG goes into `dist/` as well as being the source the PNGs are
    // rasterized from. It has to: the shell asks for it as the favicon and the
    // worker precaches it, and `caches.addAll` rejects the *whole* install if
    // any URL in its list 404s -- so a site whose `dist/` holds seven files
    // installs no worker at all, and loses offline support while looking like a
    // caching bug. Publishing the source next to its own derivatives costs one
    // file and is the only way both references can resolve.
];

/// The icon `build.rs` rasterizes, and its committed source.
const ICON_SVG: &str = "assets/icon.svg";

/// Preserved verbatim from the app's original `manifest.json`.
const APP_NAME: &str = "Breath";

/// `background_color` only ever colours the splash screen, which is shown for a
/// moment and cannot be scheme-aware, so it is the one surface where a fixed
/// value costs nothing and a light one flashes against the dark page the app
/// actually opens in.
const BACKGROUND_COLOR: &str = "#0f1117";

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());

    // The site is built only for the host target. This script also runs during
    // `cargo build --lib --target wasm32-unknown-unknown`, and at that moment
    // `dist/app.js` and `dist/app_bg.wasm` are the *output* of that build: they
    // do not exist yet, so writing the site there would fail on the very step
    // that produces them. The host build that follows the wasm-bindgen pass is
    // the one that publishes.
    //
    // `starts_with("wasm")`, not an equality test against one triple: any wasm
    // target has this problem, and the one that matters here is
    // `wasm32-unknown-unknown`.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("wasm") {
        return;
    }

    // Watch the SOURCE paths, not the output names: cargo compares these
    // against real files, so `service-worker.js` and `ui.html` have to be
    // named as the files they are. Watching the destination names watches
    // files that never change, and the script then never re-runs.
    for (_, source) in SHELL {
        println!("cargo:rerun-if-changed={source}");
    }
    println!("cargo:rerun-if-changed={ICON_SVG}");
    println!("cargo:rerun-if-changed=build.rs");

    let dist = root.join("dist");
    // Names are owned, not borrowed from a `&'static str` table: two of them
    // are built here (`icon-{size}.png`), and a `Vec<(&str, Vec<u8>)>` cannot
    // borrow a `String` that a loop has already dropped.
    let mut built: Vec<(String, Vec<u8>)> = SHELL
        .iter()
        .map(|(name, source)| {
            let bytes = std::fs::read(root.join(source))
                .unwrap_or_else(|error| panic!("reading {source}: {error}"));
            ((*name).to_string(), bytes)
        })
        .collect();

    // The worker's own template is read separately and hashed in template
    // form, so a change to the caching logic invalidates the cache: clients
    // holding the old worker would otherwise keep running stale logic against
    // new assets.
    let template = std::fs::read_to_string(root.join("src/service-worker.js"))
        .unwrap_or_else(|error| panic!("reading src/service-worker.js: {error}"));

    for size in ICON_SIZES {
        let bytes = rasterize_icon(&root.join(ICON_SVG), size);
        built.push((format!("icon-{size}.png"), bytes));
    }

    built.push(("manifest.webmanifest".to_string(), manifest().into_bytes()));

    // The wasm artefacts are hashed from `dist/` itself, not copied: they are
    // the other half of this app and a cache name that ignored them would ship
    // a worker that never notices a rebuild of the Rust. They are optional in
    // the sense that a host-only `cargo build` may run before any wasm has been
    // built at all — in that case there is nothing to hash and the shell's own
    // bytes still move the version.
    //
    // They are *hashed*, never written: `write_tree` below owns only the files
    // listed above, and copying the wasm onto itself would be a pointless
    // window in which `dist/app_bg.wasm` does not exist.
    let mut hashed = built.clone();
    for wasm in ["app_bg.wasm", "app.js"] {
        if let Ok(bytes) = std::fs::read(dist.join(wasm)) {
            hashed.push((wasm.to_string(), bytes));
        }
    }

    let version = cache_version(&hashed, &template);
    let worker = template.replace("__VERSION__", &version);
    assert!(
        !worker.contains("__VERSION__"),
        "the service worker still contains the version placeholder"
    );
    built.push(("service-worker.js".to_string(), worker.into_bytes()));

    write_tree(&dist, &built);
}

/// A cache name derived from the bytes of every file in the site except the
/// worker's own output.
///
/// It deliberately covers the worker's own source: a change to the caching
/// logic must invalidate the cache too, or clients keep running the old logic
/// against new assets.
fn cache_version(built: &[(String, Vec<u8>)], worker_template: &str) -> String {
    let mut hasher = DefaultHasher::new();
    for (name, bytes) in built {
        name.as_str().hash(&mut hasher);
        bytes.hash(&mut hasher);
    }
    worker_template.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Render `svg` into a square PNG of `size` pixels on a transparent background.
///
/// The scale is `f32`: `resvg` takes a `tiny_skia::Transform`, whose fields are
/// `f32`, so computing the ratio as `f64` does not compile. `tiny-skia` must
/// stay on `0.12` to match `resvg 0.48`.
///
/// The one cast clippy objects to is allowed, and narrowly: `f32` has no
/// `From<u32>`, so an `as f32` is the only spelling available, and widening
/// `size` through `f64` first to satisfy the lint would buy nothing — the
/// conversion is lossy in the general case and *not* lossy here, because
/// `size` is one of two constants far below the 2^24 point where `u32` stops
/// being exactly representable, and the divisor is an SVG user-unit length the
/// icon sets to a small integer. Both operands are therefore exact, and the
/// scale the renderer receives is bit-for-bit what it always was.
#[allow(clippy::cast_precision_loss)]
fn rasterize_icon(svg: &Path, size: u32) -> Vec<u8> {
    let data =
        std::fs::read(svg).unwrap_or_else(|error| panic!("reading {}: {error}", svg.display()));
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &options)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", svg.display()));

    // Both of these return `Option`, not `Result`: a 0-sized or
    // over-allocating pixmap is the only failure, and a bad `size` is a
    // constant in this file rather than something to surface to a browser.
    let mut pixmap = tiny_skia::Pixmap::new(size, size)
        .unwrap_or_else(|| panic!("could not allocate a {size}x{size} pixmap"));
    let scale = size as f32 / tree.size().width() as f32;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .unwrap_or_else(|error| panic!("encoding icon-{size}.png: {error}"))
}

/// The web app manifest, assembled so its icon list matches [`ICON_SIZES`].
///
/// `id`, `start_url` and `scope` are all `"./"` so the site mounts anywhere.
///
/// `theme_color` is deliberately **absent**, and that is not an oversight.
/// Chrome for Android prefers a manifest `theme_color` over the per-scheme
/// `<meta name="theme-color" media=...>` tags the shell declares, and a manifest
/// cannot express a scheme variant — so naming one here would pin the installed
/// app's status bar to a single scheme and quietly override both of them.
fn manifest() -> String {
    let icons: Vec<serde_json::Value> = ICON_SIZES
        .iter()
        .map(|size| {
            serde_json::json!({
                "src": format!("icon-{size}.png"),
                "sizes": format!("{size}x{size}"),
                "type": "image/png",
                "purpose": "any maskable",
            })
        })
        .collect();
    serde_json::json!({
        "id": "./",
        "name": APP_NAME,
        "short_name": APP_NAME,
        "start_url": "./",
        "scope": "./",
        "display": "standalone",
        "background_color": BACKGROUND_COLOR,
        "icons": icons,
    })
    .to_string()
}

/// Write every file this script owns into `dir`, in place.
///
/// Not a wholesale directory swap: the wasm artefacts live in `dist/` and are
/// written by the `wasm-bindgen` step, so replacing the tree would delete them
/// and leave a publishable-looking site with no app in it, and no error. Each
/// file is written under a scratch name and renamed over its target, so a host
/// serving the directory never observes a half-written file.
fn write_tree(dir: &Path, built: &[(String, Vec<u8>)]) {
    std::fs::create_dir_all(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));

    for (name, bytes) in built {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create shell directory");
        }
        let scratch = dir.join(format!(".{name}.new"));
        std::fs::write(&scratch, bytes)
            .unwrap_or_else(|error| panic!("writing {}: {error}", scratch.display()));
        std::fs::rename(&scratch, &path)
            .unwrap_or_else(|error| panic!("publishing {}: {error}", path.display()));
    }
}
