// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! Invariants of the app shell source, and of what is and is not committed.
//!
//! Everything here reads committed files. `dist/` is build output and is
//! gitignored, so the release gate — which exports the candidate tree — never
//! has it, and cannot build it either: that needs the wasm target and a pinned
//! `wasm-bindgen` CLI. A test asserting on `dist/` would therefore run only in a
//! developer's checkout, which is exactly where it is least likely to catch
//! anything, so those assertions are gone rather than skipped.
//!
//! What covers the built output is running the two build steps, in the order
//! AGENTS.md gives them, and inspecting the result.
//!
//! One thing these tests do keep: the property that a committed build never
//! rots. That was the reason the wasm artefacts were committed anywhere at all,
//! and it is the failure mode most worth a test here — a stale committed
//! `app_bg.wasm` ships a pacer that predates the code that would produce it,
//! silently.

use std::path::Path;

/// The repository root, for reading committed files.
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The app shell, as committed.
///
/// `build.rs` copies this into `dist/index.html` byte for byte, so asserting on
/// it asserts on exactly what gets published.
fn shell() -> String {
    let path = root().join("src/ui.html");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// Tracked file names, or `None` outside a checkout.
///
/// The release gate exports the candidate as a bare directory with no `.git`,
/// so there is no index to ask. Callers decide what that means.
fn tracked_files() -> Option<String> {
    let inside = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(root())
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !inside {
        return None;
    }
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(root())
        .output()
        .expect("git ls-files");
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The shell is the app, and the app is wasm. A hand-written ABI would mean the
/// pacer no longer shares the module the tests cover, and would look exactly
/// like a bug in the app when the page failed to load.
#[test]
fn the_page_loads_generated_bindings_not_a_manual_wasm_abi() {
    let page = shell();
    assert!(
        page.contains("<!doctype html>"),
        "the shell must be a document"
    );
    assert!(page.contains("<script type=\"module\">"), "a module script");
    assert!(
        page.contains("import('./app.js')"),
        "the page must load the generated bindings"
    );
    assert_eq!(
        page.matches("<script").count(),
        1,
        "exactly one script tag:\n{page}"
    );
    for obsolete in [
        "instantiateStreaming",
        "alloc_buf",
        "wasm.exports",
        "WebAssembly.instantiate",
        "fetch(",
    ] {
        assert!(!page.contains(obsolete), "{obsolete} in the static shell");
    }
}

/// The page is the app's whole interface: the ring, the orb, both duration
/// boxes, the pace readout, the validation sentence and the button that arms
/// the sound. `ui.rs` panics on a missing id, so a rename here is a crash at
/// load, and these ids are the contract between the two files.
#[test]
fn the_shell_carries_every_element_the_rust_expects() {
    let page = shell();
    for id in [
        "breath-ring",
        "breath-orb",
        "phase-label",
        "pace-label",
        "inhale-input",
        "exhale-input",
        "validation-message",
        "sound-button",
        // The two holds, and the labels whose "off" state Rust marks.
        "hold-in-input",
        "hold-out-input",
        "hold-in-field",
        "hold-out-field",
    ] {
        assert!(page.contains(&format!("id=\"{id}\"")), "missing #{id}");
    }
}

/// The holds are a first-class part of the pattern, so they are as visible and
/// as reachable as the inhale and the exhale — not tucked behind a disclosure
/// that would hide the very thing box breathing is.
#[test]
fn the_two_holds_are_visible_and_accept_zero() {
    let page = shell();

    for id in ["hold-in-input", "hold-out-input"] {
        let tag = tag_containing(&page, &format!("id=\"{id}\""))
            .unwrap_or_else(|| panic!("no tag for #{id}"));
        assert!(
            tag.contains("type=\"number\""),
            "#{id} must be a number box: {tag}"
        );
        // Zero is a real setting, so the floor is 0 and not 1. Anything else
        // makes "no hold" unreachable without clearing the field, which is the
        // mistake this test exists to prevent.
        assert!(
            tag.contains("min=\"0\""),
            "#{id} must accept 0, since a hold of zero means off: {tag}"
        );
        assert!(tag.contains("max=\"20\""), "#{id} must cap at 20: {tag}");
    }

    // Both are marked so Rust can dim them when they are off.
    for id in ["hold-in-field", "hold-out-field"] {
        let tag = tag_containing(&page, &format!("id=\"{id}\""))
            .unwrap_or_else(|| panic!("no tag for #{id}"));
        assert!(
            tag.contains("data-active="),
            "#{id} must carry the state Rust marks: {tag}"
        );
        assert!(tag.contains("class=\"hold\""), "#{id} must be a hold box");
    }

    // The ring must be able to say "holding" by itself.
    assert!(
        page.contains("data-holding"),
        "the ring must carry the hold state the stylesheet keys off"
    );
    assert!(
        page.contains("--phase-colour"),
        "the ring must be painted per phase, not in one fixed colour"
    );
}

/// The first `<…>` run containing `needle`, for asserting on a single tag.
fn tag_containing<'a>(page: &'a str, needle: &str) -> Option<&'a str> {
    let mut depth = 0usize;
    let mut open = 0usize;
    for (index, character) in page.char_indices() {
        match character {
            '<' => {
                if depth == 0 {
                    open = index;
                }
                depth += 1;
            }
            '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && page[open..index + 1].contains(needle) {
                    return Some(&page[open..index + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The pacer is for people who are, quite literally, trying to relax. The
/// changing text is announced, the ring and orb are hidden from assistive
/// technology rather than described badly, and both media queries the original
/// honoured are still honoured here rather than in Rust.
#[test]
fn the_shell_is_accessible_and_honours_the_original_media_queries() {
    let page = shell();
    assert!(
        page.contains("aria-live=\"polite\""),
        "changing text must be announced politely"
    );
    assert!(
        page.contains("role=\"status\""),
        "the phase and the validation sentence are status regions"
    );
    assert!(
        page.contains("aria-hidden=\"true\""),
        "the decorative ring and orb are hidden from assistive technology"
    );
    assert!(page.contains("<label"), "the duration inputs are labelled");
    assert!(page.contains("focus-visible"), "focus must be visible");
    for query in [
        "prefers-reduced-motion: reduce",
        "prefers-color-scheme: dark",
    ] {
        assert!(page.contains(query), "{query} must be respected");
    }
    // The orb's reduced-motion rule is the original's: no transition, so the
    // value Rust writes is the value shown.
    assert!(
        page.contains("transition: none"),
        "the orb must not ease under prefers-reduced-motion"
    );
}

/// The site is mounted under an arbitrary prefix, so every URL in it is
/// relative. One build, any subdirectory.
#[test]
fn the_shell_is_mountable_anywhere() {
    let page = shell();
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "an absolute URL would break the site outside its own origin"
    );
    assert!(
        page.contains("./app.js"),
        "bindings must be referenced relatively"
    );
    assert!(
        page.contains("./manifest.webmanifest"),
        "the page must register a manifest"
    );
    assert!(
        page.contains("./icon.svg"),
        "the committed SVG is the icon the browser tab shows"
    );
}

/// The service worker is a committed template with exactly one placeholder, and
/// `build.rs` substitutes it. A template with no placeholder would mean the
/// cache never invalidates; a second one would mean the substitution is not
/// the only edit.
#[test]
fn the_service_worker_template_has_exactly_one_placeholder() {
    let worker = std::fs::read_to_string(root().join("src/service-worker.js"))
        .expect("the committed worker template");
    assert_eq!(
        worker.matches("__VERSION__").count(),
        1,
        "the template must carry exactly one version placeholder"
    );
    assert!(
        worker.contains("new URL('./', self.location.href)"),
        "the worker must resolve its cache from its own location"
    );
    // A substring search would match the worker's own comment, which explains
    // why the call is absent. So look for the call: it is always a member
    // access on `self`, never a bare mention.
    for line in worker
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
    {
        assert!(
            !line.contains("skipWaiting"),
            "an update must not swap the wasm under a live tab: {line}"
        );
    }
}

/// The worker caches the eight files the site is made of, and the build
/// produces exactly those eight. The two lists are written separately — one is
/// committed JavaScript, one is the build — so they can drift, and a drifted
/// list is a worker that fails its install and an app that is not offline.
#[test]
fn the_worker_caches_exactly_the_eight_files_the_build_writes() {
    let worker = std::fs::read_to_string(root().join("src/service-worker.js"))
        .expect("the committed worker template");
    for asset in [
        "app.js",
        "app_bg.wasm",
        "manifest.webmanifest",
        "icon-192.png",
        "icon-512.png",
        "icon.svg",
        "index.html",
    ] {
        assert!(
            worker.contains(&format!("'{asset}'")),
            "the worker does not cache {asset}"
        );
    }
    assert!(
        worker.contains("'./'"),
        "the worker must cache the directory itself"
    );
}

/// Every file the service worker precaches must be one the build publishes.
///
/// This is the test for the bug this repository actually had: `ui.html` asked for
/// `./icon.svg` as the favicon and the worker precached `icon.svg`, but
/// `build.rs` only ever rasterized the PNGs and never published the SVG. The
/// favicon 404ed for everyone, and — because `caches.addAll` rejects an entire
/// install if any one URL 404s — the site silently got no service worker and no
/// offline support at all. The symptom looked like a caching bug.
///
/// The expectation is *derived* from `src/service-worker.js` and from
/// `build.rs`, not written out here. A hardcoded list of eight names in the test
/// would be a second copy of the truth, and drift between that copy and the two
/// sources is exactly how the omission arose in the first place.
#[test]
fn every_precached_file_is_published_by_the_build() {
    let worker = std::fs::read_to_string(root().join("src/service-worker.js"))
        .expect("the committed worker template");
    let assets = worker_assets(&worker);
    assert!(
        !assets.is_empty(),
        "the ASSETS list could not be read out of the worker"
    );

    let published = published_files();
    assert!(
        published.len() >= 8,
        "the build publishes only {published:?}"
    );

    for asset in &assets {
        // `'./'` is the scope root, which is `index.html` on disk.
        let name = if asset == "./" { "index.html" } else { asset };
        assert!(
            published.iter().any(|file| file == name),
            "the service worker precaches {asset:?}, which the build does not \
             publish; caches.addAll rejects the whole install on one 404, so this \
             silently costs the app its offline support (published: {published:?})"
        );
    }

    // And the other direction: a published file the worker does not cache is not
    // an error — an uncached file is simply fetched from the network — but the
    // app itself should all be cached, so a name dropped from the list is caught
    // here rather than being a silent no-op in the cache.
    for file in ["app.js", "app_bg.wasm", "manifest.webmanifest", "icon.svg"] {
        assert!(
            assets.iter().any(|asset| asset == file),
            "{file} is published but not precached"
        );
    }

    // The page's own references have to resolve too. The favicon is the one
    // that broke: a stylesheet or a script named by `ui.html` and missing from
    // `dist/` is the same class of fault, from the other end of the chain.
    let page = shell();
    for reference in [
        "./app.js",
        "./manifest.webmanifest",
        "./icon.svg",
        "./icon-192.png",
    ] {
        assert!(
            page.contains(reference),
            "the page references {reference}, so it must be published"
        );
    }
}

/// The `ASSETS` entries from a service worker template, in order.
///
/// Reads the array as written rather than evaluating JavaScript: the entries
/// are string literals in a fixed list, and the test needs to know the list even
/// in a template that would not parse.
fn worker_assets(worker: &str) -> Vec<String> {
    let start = worker
        .find("const ASSETS = [")
        .expect("the worker must declare ASSETS");
    let body_start = start + "const ASSETS = [".len();
    let end = worker[body_start..]
        .find("]")
        .expect("the ASSETS list must be terminated");
    worker[body_start..body_start + end]
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            let inner = entry.strip_prefix('\'')?.strip_suffix('\'')?;
            Some(inner.to_string())
        })
        .collect()
}

/// The names of the files a complete `dist/` holds, gathered from the build
/// script rather than written out here.
///
/// Two sources, because there are two builds: `build.rs` names the files it
/// copies and derives, and the bindings and the wasm are the two files the
/// `wasm-bindgen` step writes with `--out-name app`. Both are read from the
/// committed sources, so this needs no `dist/` — which is gitignored, and so
/// absent from the tree the release gate exports.
fn published_files() -> Vec<String> {
    let build = std::fs::read_to_string(root().join("build.rs")).expect("build.rs");
    let mut files = Vec::new();

    // The SHELL table: `("index.html", "src/ui.html")` and its siblings.
    for line in build.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("(\"") else {
            continue;
        };
        let Some((name, _)) = rest.split_once("\",") else {
            continue;
        };
        files.push(name.to_string());
    }

    // The derived names: the two icons at the sizes ICON_SIZES lists, the
    // manifest, and the worker itself.
    if let Some(sizes) = build
        .lines()
        .find(|line| line.trim_start().starts_with("const ICON_SIZES"))
    {
        for size in sizes
            .trim_start_matches("const ICON_SIZES: [u32; 2] = [")
            .trim_end_matches("];")
            .split(',')
        {
            let size = size.trim();
            if !size.is_empty() {
                files.push(format!("icon-{size}.png"));
            }
        }
    }
    for name in ["manifest.webmanifest", "service-worker.js"] {
        if build.contains(&format!("\"{name}\""))
            || build.contains(&format!("\"{name}\".to_string()"))
        {
            files.push(name.to_string());
        }
    }

    // The two files the `wasm-bindgen` step writes, from `--out-name app`.
    files.push("app.js".to_string());
    files.push("app_bg.wasm".to_string());
    files
}

/// Nothing generated may be tracked — not the wasm, not the bindings, not the
/// site, and not the rasterized icons. This is the test that would have caught
/// any of them being committed.
#[test]
fn no_build_artifact_is_committed() {
    let Some(tracked) = tracked_files() else {
        return; // not a checkout: the gate's exported tree
    };
    for artefact in [
        "assets/icon-192.png",
        "assets/icon-512.png",
        "dist/index.html",
        "dist/app.js",
        "dist/app_bg.wasm",
    ] {
        assert!(
            !tracked.lines().any(|line| line == artefact),
            "{artefact} is tracked; generated artefacts must never be committed"
        );
    }
}

/// The PNGs exist only in `dist/`. `assets/` holds the SVG and nothing else, so
/// there is no second icon to drift from the one the build rasterizes.
#[test]
fn the_png_icons_exist_only_in_dist() {
    let assets = root().join("assets");
    let entries: Vec<String> = std::fs::read_dir(&assets)
        .unwrap_or_else(|error| panic!("reading {}: {error}", assets.display()))
        .map(|entry| {
            entry
                .expect("an assets entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        entries,
        ["icon.svg"],
        "assets/ holds the SVG icon and nothing else"
    );
}

/// `dist/` has to be ignored, or a build would leave the next commit dirty.
#[test]
fn dist_is_ignored() {
    if tracked_files().is_none() {
        return; // not a checkout
    }
    let ignored = std::process::Command::new("git")
        .args(["check-ignore", "-q", "dist/"])
        .current_dir(root())
        .status()
        .expect("git check-ignore")
        .success();
    assert!(ignored, "dist/ must be in .gitignore");
}

/// The repository is a Rust crate and a static shell, not a Rust crate with a
/// dead copy of the previous JavaScript app still in it.
///
/// The rewrite moved every behaviour into `src/pacer.rs` and `src/ui.rs`. If the
/// original `app.js`, `style.css`, `sw.js` or `manifest.json` were still tracked,
/// the repository would hold two PWAs — one served and one dead — and nothing
/// would say which is the app. Removing them is half of the rewrite; this is the
/// half that stops them coming back.
#[test]
fn the_original_javascript_app_is_gone_from_the_tree() {
    for removed in [
        "app.js",
        "style.css",
        "sw.js",
        "manifest.json",
        "eslint.config.mjs",
        "Makefile",
        "icon.svg",
        "LICENSE.md",
    ] {
        let path = root().join(removed);
        assert!(
            !path.exists(),
            "{removed} is still in the tree; the rewrite replaces it"
        );
        if let Some(tracked) = tracked_files() {
            assert!(
                !tracked.lines().any(|line| line == removed),
                "{removed} is still tracked; the rewrite replaces it"
            );
        }
    }
}

/// And the shell does not quietly reintroduce one.
#[test]
fn the_shell_has_no_application_javascript() {
    let page = shell();
    // The single module script's entire body is the loader. Everything the app
    // does is in Rust, so nothing here may define a function, keep state in a
    // variable, or touch the pacer's own properties.
    // `=>` is not on this list, and cannot be: the six-line loader the spec
    // prescribes is `import(...).then(m => m.default()).catch(error => {...})`,
    // which is nothing but arrows. What is forbidden is the *state* and the
    // *DOM writes* that would mean logic had crept back into the page.
    for forbidden in [
        "function",
        "setInterval",
        "requestAnimationFrame",
        "localStorage",
        "AudioContext",
        "performance.now",
        "cycleStartedAt",
        "addEventListener",
    ] {
        assert!(
            !page.contains(forbidden),
            "{forbidden} in the shell: the application is Rust"
        );
    }

    // The three custom properties belong in the *stylesheet*, as the defaults
    // Rust overrides: the orb's `var(--orb-scale, 0.74)` is what makes the page
    // look right before the first tick, and is the reason a paint that never
    // arrives degrades to a still orb rather than a broken one. They must
    // appear in the CSS and nowhere else -- in particular not in a style
    // attribute or a string in the loader.
    for property in ["--orb-scale", "--phase-color", "--phase-progress"] {
        assert!(
            page.contains(&format!("var({property},")),
            "{property} must have a CSS default in the shell"
        );
    }
    assert!(
        !page.contains("setProperty"),
        "the shell must not write a custom property; Rust does"
    );

    // And the script tag really is only the loader: one dynamic import of the
    // generated bindings, one catch, no other statement.
    // `split_once` yields (before, after), so the loader body is the `before`
    // side of the closing tag — the trap being that the wrong arm still yields
    // a non-empty string, and the failure then reads like a shell problem.
    let script = page
        .split_once("<script type=\"module\">")
        .and_then(|(_, rest)| rest.split_once("</script>"))
        .map(|(body, _)| body.trim())
        .expect("a module script with a body");
    let trimmed = script.trim_start();
    assert!(
        trimmed.starts_with("import('./app.js')"),
        "the loader must be a dynamic import of the bindings:\n{script}"
    );
    assert_eq!(
        script.matches("import(").count(),
        1,
        "exactly one import, so exactly one way the page can start:\n{script}"
    );
}

/// `Cargo.toml` is the build's contract: a pinned generator, and no binary to
/// publish or run. A `[[bin]]` would make the crate a CLI, which this app is
/// not — there is no server and no unit.
#[test]
fn the_crate_is_a_library_with_a_pinned_generator() {
    let manifest =
        std::fs::read_to_string(root().join("Cargo.toml")).expect("the committed Cargo.toml");

    assert!(
        manifest.contains("wasm-bindgen = \"=0.2.128\""),
        "wasm-bindgen must be pinned exactly, or the bindings will not load"
    );
    assert!(
        !manifest.contains("[[bin]]"),
        "breath is a browser app: there is no binary to build or run"
    );
    assert!(
        manifest.contains("crate-type = [\"cdylib\", \"rlib\"]"),
        "the crate must be both a wasm library and a testable library"
    );
    // The audio features the spec lists are not enough on their own: see
    // `AudioParam`, `AudioNode` and `AudioDestinationNode` in Cargo.toml.
    for feature in [
        "AudioContext",
        "AudioContextState",
        "OscillatorNode",
        "OscillatorType",
        "GainNode",
        "AudioScheduledSourceNode",
        "AudioParam",
        "AudioNode",
        "AudioDestinationNode",
    ] {
        assert!(
            manifest.contains(&format!("\"{feature}\"")),
            "the audio cue needs the web-sys feature {feature}"
        );
    }
}

/// The icon is the committed SVG, in one colour, on a transparent ground. It is
/// the one file in this repository that is *not* a byte-for-byte carry-over from
/// the original app — `breath` is the app the family allowed to redraw its mark
/// — so its shape is asserted rather than its bytes.
#[test]
fn the_committed_icon_is_one_svg_and_the_source_of_the_pngs() {
    let icon = std::fs::read_to_string(root().join("assets/icon.svg")).expect("the committed icon");
    assert!(
        icon.contains("viewBox=\"0 0 512 512\""),
        "a square 512 viewBox"
    );
    assert!(
        icon.contains("<svg") && icon.matches("<svg").count() == 1,
        "one SVG, not a sprite sheet"
    );

    // One colour. `build.rs` rasterizes this to a transparent PNG, so a second
    // colour would be the only thing distinguishing the mark from a silhouette
    // of itself.
    let mut colours = icon
        .match_indices(['#', 'r', 'g', 'b'])
        .filter_map(|(index, _)| {
            let rest = &icon[index..];
            let hex: String = rest
                .chars()
                .skip(1)
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            (hex.len() == 6).then(|| format!("#{hex}"))
        })
        .collect::<Vec<_>>();
    colours.sort();
    colours.dedup();
    assert_eq!(
        colours,
        ["#0f766e"],
        "the icon is a single-colour mark in the app's own inhale teal"
    );

    // `build.rs` derives both install PNGs from this file, and never from a
    // committed PNG.
    let build = std::fs::read_to_string(root().join("build.rs")).expect("build.rs");
    assert!(
        build.contains("assets/icon.svg"),
        "build.rs must rasterize the committed SVG"
    );
    assert!(build.contains("ICON_SIZES"), "build.rs owns the icon sizes");
    assert!(
        !build.contains("icon-192.png\""),
        "build.rs must not read a committed PNG icon"
    );
}

/// The manifest is assembled by `build.rs`, not committed, so there is no
/// committed copy to assert. What can be asserted is that the constants it is
/// assembled from are the original app's, and that nothing in the shell pins a
/// different theme colour than the manifest publishes.
#[test]
fn the_manifest_keeps_the_original_apps_colours_and_name() {
    let build = std::fs::read_to_string(root().join("build.rs")).expect("build.rs");
    for value in ["\"#0f766e\"", "\"#f6f2ea\"", "\"standalone\""] {
        assert!(
            build.contains(value),
            "the manifest must publish {value} from the original manifest.json"
        );
    }
    let page = shell();
    assert!(
        page.contains("content=\"#0f766e\""),
        "the page's theme-color must match the manifest's"
    );
    // `id`, `start_url` and `scope` are `"./"` in build.rs so the site mounts
    // anywhere; a stray absolute path would pin it to one host.
    assert!(
        build.contains("\"start_url\": \"./\""),
        "start_url must be relative"
    );
}

/// The pacer's settings live under one key, and that key is the original's: a
/// rewrite that changed it would silently discard the pattern every existing
/// user had already chosen.
#[test]
fn the_storage_key_is_still_the_original_apps() {
    let pacer = std::fs::read_to_string(root().join("src/pacer.rs")).expect("src/pacer.rs");
    assert!(
        pacer.contains("\"breath-pwa-settings-v3\""),
        "the storage key must not change: it is every existing user's pattern"
    );
}

/// No user-visible text may name the implementation.
///
/// A person using this app is trying to relax. Text that says how the app is
/// built — "Rust", "WebAssembly", "wasm", "bindings", "compile" — is addressed to
/// a maintainer, not to them, and the one person guaranteed to read the
/// `<noscript>` is someone whose browser is already failing them.
///
/// The scope is the hard part. This must reach only what a reader can *see*, and
/// the shell is mostly a stylesheet whose comments explain exactly which
/// properties Rust writes and which media queries the stylesheet owns. That
/// documentation is correct, valuable, and must survive — so `<style>` and
/// `<script>` are removed wholesale rather than parsed, and HTML comments are
/// stripped. What remains is element text plus the title and description.
#[test]
fn no_user_visible_text_names_the_implementation() {
    let rendered = rendered_text(&shell());

    for word in [
        "rust",
        "webassembly",
        "wasm",
        "bindings",
        "compile",
        "compiled",
    ] {
        assert!(
            !rendered.to_lowercase().contains(word),
            "{word:?} reaches the reader of the interface:\n{rendered}"
        );
    }

    // The loader's own strings are prose a reader sees, even though the code
    // around them is not — so they are checked here, extracted, rather than
    // exempted along with the rest of the script. `JavaScript` is deliberately
    // *not* on the list: in a `<noscript>` fallback it is the one term that
    // names the actual blocker, and no reader can act on "the application
    // layer" or "this page's scripts" as clearly as they can act on the word
    // their browser settings are labelled with.
    for string in string_literals(&shell()) {
        let lowered = string.to_lowercase();
        for word in ["rust", "webassembly", "wasm", "bindings", "compile"] {
            assert!(
                !lowered.contains(word),
                "{word:?} appears in page text: {string:?}"
            );
        }
    }
}

/// The shell with everything a reader cannot see as prose removed: the
/// stylesheet, the script body, and every comment.
fn rendered_text(page: &str) -> String {
    let mut trimmed = remove_block(page, "<style", "</style>");
    trimmed = remove_block(&trimmed, "<script", "</script>");
    let without_comments = strip_comments(&trimmed);

    let mut kept: Vec<String> = vec![without_comments.replace(['<', '>'], " ")];
    for line in kept.iter_mut() {
        let mut single = String::with_capacity(line.len());
        let mut spaces = 0;
        for character in line.chars() {
            if character.is_whitespace() {
                spaces += 1;
                continue;
            }
            if spaces > 0 && !single.is_empty() {
                single.push(' ');
            }
            spaces = 0;
            single.push(character);
        }
        *line = single;
    }
    for chunk in page.split("<meta").skip(1) {
        let Some(end) = chunk.find('>') else { continue };
        let tag = &chunk[..end];
        if tag.contains("name=\"description\"") {
            if let (Some(start), Some(stop)) = (tag.find("content=\""), tag.rfind("\"")) {
                kept.push(tag[start + "content=\"".len()..stop].to_string());
            }
        }
    }
    kept.retain(|piece| !piece.trim().is_empty());
    kept.join("\n")
}

/// The document with one element's contents removed, tags included.
fn remove_block(page: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(page.len());
    let mut rest = page;
    while let Some(start) = rest.find(open) {
        let (before, tail) = rest.split_at(start);
        out.push_str(before);
        match tail.find(close) {
            Some(end) => rest = &tail[end + close.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The document with every comment removed, in either syntax.
fn strip_comments(page: &str) -> String {
    let mut out = page.to_string();
    for (open, close) in [("<!--", "-->"), ("/*", "*/")] {
        while let Some(start) = out.find(open) {
            let end = match out[start..].find(close) {
                Some(end) => start + end + close.len(),
                None => out.len(),
            };
            out.replace_range(start..end, " ");
        }
    }
    out
}

/// Every single-quoted string literal inside the page's script, which is where
/// this shell's user-facing prose lives.
///
/// Scoped to the script deliberately: run over the whole document the scan pairs
/// an apostrophe in a *comment* with one far away in real markup, and returns a
/// "literal" that is mostly a stylesheet. Comments are documentation and are
/// checked by neither this nor [`rendered_text`].
fn string_literals(page: &str) -> Vec<String> {
    let script = match page.split_once("<script") {
        Some((_, rest)) => match rest.split_once("</script>") {
            Some((body, _)) => body,
            None => return Vec::new(),
        },
        None => return Vec::new(),
    };
    let mut found = Vec::new();
    let mut rest = script;
    while let Some(open) = rest.find('\'') {
        let tail = &rest[open + 1..];
        match tail.find('\'') {
            Some(close) => {
                found.push(tail[..close].to_string());
                rest = &tail[close + 1..];
            }
            None => break,
        }
    }
    found
}
