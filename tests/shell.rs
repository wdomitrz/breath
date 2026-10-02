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

/// The shell's inline `<style>` block, and nothing else.
///
/// Assertions about what the *stylesheet* says — that a media query exists, that
/// a custom property is defined — have to be scoped to it. A whole-document
/// search is satisfied by a `meta` tag, an HTML comment or an attribute that
/// merely mentions the same words, so it goes green with the rule deleted. This
/// returns the CSS alone, so a deleted rule cannot hide behind a mention of it.
///
/// Panics if there is no style block, which is itself a failure worth naming:
/// every rule below lives in one.
fn style_of(page: &str) -> String {
    let open = page
        .find("<style")
        .unwrap_or_else(|| panic!("the shell must carry an inline <style> block:\n{page}"));
    let body = open + page[open..].find('>').expect("a closed <style> tag");
    let close = page[body..]
        .find("</style>")
        .unwrap_or_else(|| panic!("an unclosed <style> block:\n{page}"));
    page[body + 1..body + close].to_string()
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

/// The pattern hint is gone, and stays gone.
///
/// It used to sit under the four boxes and read "Holds are the pauses between
/// breaths. Leave one at 0 to switch it off — try 4 · 4 · 4 · 4 for box
/// breathing." Two reasons it had to go:
///
/// * It taught a pattern (4·4·4·4) rather than explaining the fields the user
///   was looking at, and the default is no longer that pattern either.
/// * With the default now carrying holds, a line whose whole job was to explain
///   what a hold *is* was doing the explaining that the default demonstrates.
///
/// This asserts on `rendered_text`, not on the raw page, because the text could
/// equally be reintroduced as a comment — where a reader never sees it — and a
/// raw `contains` would pass just as happily. The prose is only gone if it is
/// gone from what a reader sees.
#[test]
fn the_pattern_hint_is_not_rendered() {
    let rendered = rendered_text(&shell());
    for phrase in ["pauses between breaths", "box breathing", "switch it off"] {
        assert!(
            !rendered.contains(phrase),
            "{phrase:?} is back in the interface:\n{rendered}"
        );
    }

    // And the stylesheet rule that only existed to lay it out is gone with it.
    let page = shell();
    assert!(
        !page.contains("pattern-hint"),
        "the removed hint's stylesheet rule is still here"
    );
}

/// The pace readout baked into the shell must be the default's own readout.
///
/// `index.html` carries a hand-written "6 breaths/min" so the page shows
/// something before the bindings load. Nothing kept it in step with
/// `Settings::DEFAULT`, so the day the default changed from a 10-second cycle
/// to a 20-second one, the shell would have flashed *6 breaths/min* at a reader
/// and then corrected itself to 3 on the first tick — a wrong number, shown
/// before anything had a chance to be right. Asserted against the crate rather
/// than against a literal, so the next default change fails here rather than in
/// front of a user.
#[test]
fn the_shell_opens_on_the_default_pace() {
    assert_eq!(
        breath::pacer::Settings::DEFAULT.pace_label(),
        "3 breaths/min"
    );

    let page = shell();
    let expected = format!("<strong id=\"pace-label\">{}</strong>", "3 breaths/min");
    assert!(
        page.contains(&expected),
        "the shell's opening pace readout must be the default's: {expected}"
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
    // Asserted against the stylesheet, not the whole document: `prefers-color-
    // scheme` also appears in the two per-scheme `<meta name="theme-color">`
    // tags, so a whole-document search passes even with the media query deleted
    // — which is the case this is here to prevent. The dark palette is the
    // default block rather than a `prefers-color-scheme: dark` query, so the
    // query that must exist is the light override.
    let style = style_of(&page);
    for query in [
        "prefers-reduced-motion: reduce",
        "prefers-color-scheme: light",
    ] {
        assert!(
            style.contains(query),
            "{query} must be respected in the CSS"
        );
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

/// The worker only ever answers for a URL inside its own app's directory.
///
/// This is the guard that stops one of these apps from taking over the pages it
/// shares an origin with. A service worker registered for a scope is consulted
/// for every URL under that scope, and these apps are all served from the same
/// origin as pages that are not apps at all — so "the scope is small" is a
/// promise, and this test is what keeps it one.
#[test]
fn the_worker_never_answers_outside_its_own_directory() {
    let worker = std::fs::read_to_string(root().join("src/service-worker.js"))
        .expect("the committed worker template");
    assert!(
        worker.contains("new URL('./', self.location.href)"),
        "the worker must resolve its own directory from its location"
    );
    // The guard is a prefix test against that directory, on the request URL,
    // applied before the allowlist decides anything.
    assert!(
        worker.contains("IS_OWN(url)"),
        "the fetch handler must check the request is inside this app's directory; \
         without it a mis-scoped registration serves whatever it cached"
    );
    assert!(
        worker.contains("const IS_OWN = url => url.startsWith(ROOT.href)"),
        "the directory guard must be a prefix test against the worker's own root"
    );
}

/// The page states the worker's scope instead of inheriting it, and cleans up a
/// wider registration left behind by an earlier version.
///
/// A registration outlives the page that created it, and nothing short of an
/// explicit `unregister` takes one away. So the second half is what makes this
/// recoverable without the user clearing their browser: a stale registration
/// is not fixed by a reload, and the newer worker cannot take control of a
/// scope it does not own.
#[test]
fn the_page_states_the_scope_and_releases_a_wider_one() {
    let ui = std::fs::read_to_string(root().join("src/ui.rs")).expect("src/ui.rs");
    assert!(
        ui.contains("register_with_options"),
        "the worker must be registered with an explicit scope; left to default, \
         the scope is whatever directory the registering page sits in"
    );
    assert!(
        ui.contains("RegistrationOptions::new()") && ui.contains("set_scope(SCOPE)"),
        "the scope has to be actually stated, not merely a named constant"
    );
    assert!(
        ui.contains("get_registrations") && ui.contains("unregister"),
        "a stale wider registration survives a reload, a version bump and a \
         reinstall; only an explicit unregister clears it"
    );
}

/// A worker's script is compared by suffix, not by `trim_end_matches`.
///
/// `trim_end_matches` strips a *set of characters*, so a directory whose name
/// ends in those letters is silently treated as ours — and a registration
/// belonging to a sibling app would be torn down. This is a regression test for
/// a real bug in the first version of this code.
#[test]
fn the_script_comparison_strips_a_suffix_rather_than_a_character_set() {
    let ui = std::fs::read_to_string(root().join("src/ui.rs")).expect("src/ui.rs");
    // The prose in this file names the method to explain why it is not used, so
    // the assertion is about code: a call, not the word.
    let calls: Vec<&str> = ui
        .lines()
        .filter(|line| {
            let code = line.split("//").next().unwrap_or(line);
            code.contains("trim_end_matches(")
        })
        .collect();
    assert!(
        calls.is_empty(),
        "`trim_end_matches` strips a character set, not a filename: it would eat \
         any directory ending in those letters and tear down a sibling's worker. \
         Found: {calls:?}"
    );
    assert!(
        ui.contains("strip_suffix(\"service-worker.js\")"),
        "the comparison must strip the one filename it expects"
    );
}

/// The scope is named once, and the page and the worker agree on the directory.
///
/// Two independent resolutions of "where am I" — the page's `./` and the worker's
/// `new URL('./', self.location.href)`. They have to describe the same
/// directory, or the page registers a scope the worker's guard does not match.
#[test]
fn the_scope_is_a_relative_directory_shared_with_the_worker() {
    let ui = std::fs::read_to_string(root().join("src/ui.rs")).expect("src/ui.rs");
    assert!(
        ui.contains("const SCOPE: &str = \"./\";"),
        "the scope must be the app's own directory, relative — so one build works \
         from any subdirectory"
    );
    let worker = std::fs::read_to_string(root().join("src/service-worker.js"))
        .expect("the committed worker template");
    assert!(
        worker.contains("new URL('./', self.location.href)"),
        "the worker must resolve the same directory the page registered"
    );
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

    // One name per file, sorted. `service-worker.js` is named twice — once in
    // the `SHELL` table it is copied from, once by the loop above — and a list
    // that repeats a name is not a description of a directory. The duplicate was
    // invisible while callers only asked `any(...)` and `>= 8`; it matters to a
    // caller that compares this list against the files a build actually
    // produces. The sort is the dedup: adjacent entries only.
    files.sort();
    files.dedup();
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

/// Every path a release leaves behind must be ignored, not just the ones a
/// build produces.
///
/// `.release-recovery/` is the one this repository had wrong. It is written by
/// the release machinery on every successful release, so before it was ignored
/// `git status` reported master as permanently dirty after every release — a
/// directory full of journals, archived rather than deleted, that nobody
/// reviews and that `git add -A` would happily commit. The `dist_is_ignored`
/// test above cannot catch that: it only knows about the one path it names, so
/// a new one is invisible until it has already been committed once.
///
/// This asks git directly rather than reading `.gitignore`, so a negated or
/// scoped rule (`dist/*` but not `dist/keep-me`, say) cannot pass by containing
/// the right word.
#[test]
fn every_path_a_release_leaves_behind_is_ignored() {
    if tracked_files().is_none() {
        return; // not a checkout
    }
    for path in ["dist/", ".release-recovery/"] {
        let ignored = std::process::Command::new("git")
            .args(["check-ignore", "-q", path])
            .current_dir(root())
            .status()
            .expect("git check-ignore")
            .success();
        assert!(ignored, "{path} must be in .gitignore");
    }
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
/// assembled from are the ones the page is actually painted in, and that the two
/// cannot drift apart.
///
/// This used to pin the original app's teal and cream. It no longer does, and
/// the reason is the invariant worth keeping: an installed app's status bar is
/// painted from a single `theme_color`, and a manifest cannot express a colour
/// scheme variant. Naming one colour here would quietly override the two
/// per-scheme `<meta>` tags the shell declares and pin every installer's bar to
/// whichever scheme that colour happened to suit. So the manifest declares no
/// `theme_color` at all, and what is asserted here is the absence — plus that
/// the splash background is the dark one, since a splash cannot follow a scheme
/// either and a light splash flashes against the dark page behind it.
#[test]
fn the_manifest_declares_no_theme_color_and_the_shell_owns_both_schemes() {
    let build = std::fs::read_to_string(root().join("build.rs")).expect("build.rs");
    let code = strip_rust_comments(&build);
    assert!(
        !code.contains("theme_color"),
        "a manifest theme_color cannot follow the scheme and would override the shell's"
    );
    for value in ["\"#0f1117\"", "\"standalone\""] {
        assert!(code.contains(value), "the manifest must publish {value}");
    }
    let page = shell();
    // Both schemes, and both carrying the background the page is actually
    // painted in — one per scheme, or an installed app's bar is a foreign
    // colour until the reader changes their phone.
    for scheme in ["dark", "light"] {
        assert!(
            page.contains(&format!(
                "content=\"#0f1117\" media=\"(prefers-color-scheme: {scheme})\""
            )) || page.contains(&format!(
                "content=\"#f5f6f8\" media=\"(prefers-color-scheme: {scheme})\""
            )),
            "the shell must declare a theme colour for the {scheme} scheme"
        );
    }
    // The two theme colours are the family's, and the page draws the same two
    // backgrounds under the same two names.
    for token in ["#0f1117", "#f5f6f8"] {
        assert!(
            page.contains(token),
            "the shell must paint itself in {token}, the theme colour it declares"
        );
    }
    // `id`, `start_url` and `scope` are `"./"` in build.rs so the site mounts
    // anywhere; a stray absolute path would pin it to one host.
    assert!(
        code.contains("\"start_url\": \"./\""),
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

/// `build.rs` with every comment removed.
///
/// `theme_color` is the reason this exists. That word belongs in a comment
/// explaining *why* the manifest omits the key, and a test asserting the key is
/// absent then fails on the explanation. The same trap the rest of this file
/// documents for the shell: an assertion about code must not be satisfied by a
/// comment about the code.
///
/// A line comment runs to the end of the line and a block comment to its closer.
/// A `//` inside a string literal is not a comment, but no string these tests
/// search for contains one, and a full Rust tokenizer is not worth the
/// complexity to guard against it.
fn strip_rust_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find('/') {
        let after = &rest[start + 1..];
        if let Some(tail) = after.strip_prefix("//") {
            out.push_str(&rest[..start]);
            rest = tail.split_once('\n').map_or("", |(_, line)| line);
        } else if let Some(tail) = after.strip_prefix("/*") {
            out.push_str(&rest[..start]);
            rest = tail.split_once("*/").map_or("", |(_, line)| line);
        } else {
            out.push_str(&rest[..=start]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// The Pages deployment
//
// `dist/` is gitignored and `app.js`/`app_bg.wasm` exist nowhere else in the
// tree, so a Pages build serving committed files cannot publish the pacer at
// all: the page would load and never start. `.github/workflows/pages.yml` is
// therefore not an optimisation, it is the thing that makes the site exist
// off-host. The assertions below are the invariants that survive it being
// edited by hand, which is what happens to a YAML file that no test reads.
// ─────────────────────────────────────────────────────────────────────────────

/// A workflow file, as committed.
///
/// Nonexistent is not a reason to fail: a fresh export of a *release branch*
/// need not have the Pages workflow, and a test that hard-failed on its absence
/// would make a partial export red for a reason that says nothing about the
/// pacer. Callers say which they want; the one test that requires it is
/// [`the_pages_workflow_is_the_thing_that_publishes_the_pacer`].
fn workflow(name: &str) -> Option<String> {
    let path = root().join(".github/workflows").join(name);
    std::fs::read_to_string(&path).ok()
}

/// A workflow with every comment removed.
///
/// The trap this guards: commenting a line out instead of deleting it. A
/// `# RUSTFLAGS:` in `pages.yml` is not a setting, but a plain `contains` finds
/// it just as happily as the live line — which is how a workflow once passed its
/// own test with the flag commented out.
///
/// A line-wise cut at the first `#` is enough here: this workflow quotes nothing
/// in the keys the assertions read and carries no `#` in any value they depend
/// on, so a YAML parser would be a dependency bought for nothing.
fn strip_yaml_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find('#') {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Pages deployment cannot drift from the crate it publishes.
///
/// The deploy installs its own `wasm-bindgen`, pinned to a literal in the YAML,
/// and `Cargo.toml` pins the same version the crate compiles against. Move the
/// dependency and the workflow keeps building happily: it generates bindings for
/// a runtime the page does not have, and the only symptom is a live site that
/// fails at startup with "Could not load the breathing pacer" — for every
/// visitor, and only in a browser. So the two are asserted equal rather than
/// trusted to be edited together.
#[test]
fn the_pages_build_generates_bindings_for_the_pinned_runtime() {
    let Some(pages) = workflow("pages.yml") else {
        return;
    };
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // Read the pin as Cargo writes it: `wasm-bindgen = "=0.2.128"`, an exact
    // requirement. A looser form would resolve to whatever is newest in the
    // lockfile, and the workflow's literal would then be naming one arbitrary
    // version of several.
    let expected = manifest
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("wasm-bindgen")?;
            let rest = rest.trim_start().strip_prefix('=')?;
            Some(rest.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| panic!("Cargo.toml pins no exact wasm-bindgen version"));
    let version = expected.trim_start_matches('=').trim_matches('"');
    assert!(
        !version.is_empty(),
        "Cargo.toml pins no version at all; the workflow cannot name what to install"
    );

    // The version has to reach the job that generates the bindings, as the
    // `version=` it installs — not merely somewhere in the file, where a
    // comment naming it would satisfy this and the job would still install
    // whatever else it found.
    let live = strip_yaml_comments(&pages);
    let literal = format!("version=\"{version}\"");
    let indirect = "version=\"$WASM_BINDGEN_VERSION\"";
    assert!(
        live.contains(&literal) || live.contains(indirect),
        "pages.yml must install wasm-bindgen {version} (`{literal}` or `{indirect}`); it cannot \
         drift from the Cargo.toml pin, or the site fails to start in the browser and nowhere else",
    );

    // A version passed through `env:` is a single source of truth only if the
    // variable is declared at the same literal value. An undeclared variable
    // expands to nothing, so the job would install the empty string — which
    // passes every other check here and publishes bindings for no runtime. The
    // declaration has to be *top-level*: `env:` on one step does not reach the
    // others, and a step-scoped variable that looks top-level reads as global.
    if live.contains(indirect) {
        let declared = format!("WASM_BINDGEN_VERSION: {version}");
        let line = live
            .lines()
            .find(|line| line.trim_start().starts_with("WASM_BINDGEN_VERSION:"))
            .unwrap_or_else(|| {
                panic!(
                    "pages.yml installs $WASM_BINDGEN_VERSION but never declares it as {version}"
                )
            });
        // Compare against the line trimmed of the comment cut, not of its
        // trailing spaces: `strip_yaml_comments` leaves the indent and the
        // padding before a `#` behind, so an exact-prefix match on the raw line
        // fails on a correctly declared variable whose line ends in a space.
        let live_line = line.trim_end();
        assert!(
            live_line.starts_with("  WASM_BINDGEN_VERSION:"),
            "WASM_BINDGEN_VERSION must be declared in the workflow's top-level `env:` (found: \
             {line:?}); `env:` on one step does not reach the others, and a step-scoped \
             declaration reads exactly like a global one",
        );
        assert_eq!(
            declared,
            live_line.trim_start(),
            "pages.yml declares a wasm-bindgen version other than the Cargo.toml pin ({version})",
        );
    }
}

/// This crate does not use the Screen Wake Lock API, so it does not need the
/// flag — and must not carry one.
///
/// `--cfg=web_sys_unstable_apis` is what `chess_clock`'s Pages workflow needs:
/// the whole Screen Wake Lock API is behind that cfg in web-sys 0.3.105, so the
/// wasm build dies with `cannot find WakeLockSentinel in crate web_sys` without
/// it, and `build.rs`'s `cargo:rustc-cfg` does not reach the registry crate.
/// That is a fact about *that* crate. Here there is no Wake Lock use anywhere —
/// verified 2026-10-02 by a clean build with `RUSTFLAGS` unset, which finishes
/// green.
///
/// So the invariant asserted is the absence, and it is asserted against the
/// stripped text because the flag would otherwise be free to hide in a comment
/// that explains why it is not needed. This is not a style preference: a
/// workflow carrying a compiler flag for an API the crate never calls is a lie
/// about what the build requires, and the next person to add a wake lock will
/// read the flag as load-bearing and never learn whether it is.
#[test]
fn the_pages_build_carries_no_wake_lock_flag() {
    let Some(pages) = workflow("pages.yml") else {
        return;
    };
    let live = strip_yaml_comments(&pages);
    for absent in ["RUSTFLAGS", "web_sys_unstable_apis"] {
        assert!(
            !live.contains(absent),
            "pages.yml sets {absent}, which this crate does not need: nothing in src/ uses the \
             Screen Wake Lock API, and a clean wasm build with RUSTFLAGS unset succeeds (verified \
             2026-10-02). Copy the flag into build.yml too and the two workflows disagree about \
             what the crate needs",
        );
    }
}

/// The site cannot be published by serving committed files, so the workflow
/// that publishes it has to run both builds itself, in order.
///
/// `dist/` is gitignored and two of its files — `app.js` and `app_bg.wasm`,
/// written by `wasm-bindgen` — exist nowhere else in the tree. A workflow that
/// deployed a checked-out `dist/` would publish a page that loads and never
/// starts, and the workflow would be green throughout.
///
/// The order is the second half of the contract. `build.rs` derives the service
/// worker's cache version from the bytes of every other file in `dist/`, the wasm
/// included, so running it before the bindings leaves the cache pinned to
/// whatever the previous build wrote; and the `touch build.rs` is not
/// redundancy, because `build.rs` writes into the source tree rather than
/// `OUT_DIR`, so cargo cannot see its own output change and skips the second
/// build entirely — leaving `dist/` holding the two wasm artefacts and none of
/// the six shell files.
#[test]
fn the_pages_workflow_is_the_thing_that_publishes_the_pacer() {
    let Some(pages) = workflow("pages.yml") else {
        return;
    };
    let live = strip_yaml_comments(&pages);

    // Both builds, in the order AGENTS.md gives them. Read as positions in the
    // live text rather than as a fixed sequence of lines, so reordering the
    // steps or wrapping one differently cannot disguise the mistake.
    let wasm_build = live
        .find("cargo build --locked --lib --target wasm32-unknown-unknown")
        .expect(
            "pages.yml must build the wasm target: app.js and app_bg.wasm are gitignored build \
                 output, and nothing committed produces them",
        );
    let bindings = live
        .find("wasm-bindgen --target web")
        .expect("pages.yml must run the bindings generator; it is the only writer of dist/app.js");
    let touch = live.find("touch build.rs").expect(
        "pages.yml must touch build.rs before the host build; build.rs writes into the source tree \
         rather than OUT_DIR, so without the touch the second build is a no-op and dist/ keeps \
         only the two wasm artefacts",
    );
    let host_build = live
        .find("cargo build --release --locked")
        .expect("pages.yml must run the host build: build.rs writes the six shell files from it");
    assert!(
        wasm_build < bindings && bindings < touch && touch < host_build,
        "the two builds must run in order — wasm, bindings, touch build.rs, host. The worker cache \
         version is hashed from the wasm, so a host build first pins it to whatever the previous \
         build left behind (wasm@{wasm_build}, bindings@{bindings}, touch@{touch}, host@{host_build})",
    );

    // It must check what it built rather than trusting a green job. `dist/` is
    // gitignored, so no test in this suite can see it and this step is the only
    // thing that ever does — a build that produced seven of the eight files is a
    // publishable-looking site with no pacer in it.
    let expected = live
        .split("expected=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("pages.yml must declare the file list it expects in dist/");
    let declared: Vec<&str> = expected.split_whitespace().collect();
    assert_eq!(
        declared.len(),
        published_files().len(),
        "pages.yml checks for {} files ({}) but the build publishes {}; the check must name \
         every published file, and nothing else",
        declared.len(),
        declared.join(", "),
        published_files().len(),
    );
    for file in published_files() {
        assert!(
            declared.contains(&file.as_str()),
            "pages.yml does not check dist/{file}, which the build publishes",
        );
    }
    // Each of the four silent shapes a green build can still be. `__VERSION__`
    // is the cache version, derived at build time: a placeholder surviving it
    // ships a worker that never invalidates. `export` and `__wbindgen_start`
    // are the two things the page's dynamic import and its `m.default()` call
    // need, and a bindings file missing either fails only in a browser, long
    // after the build looked fine. The PNG magic number is the icons: they are
    // rasterized at build time, so a truncated write is not a picture.
    for (needle, why) in [
        (
            "__VERSION__",
            "the worker's cache version would never be substituted",
        ),
        ("'export'", "the page's dynamic import would fail"),
        ("__wbindgen_start", "the pacer would load and never run"),
        ("m.default()", "the page would never start the pacer"),
        ("89504e470d0a1a0a", "the rasterized icons are not PNGs"),
    ] {
        assert!(
            live.contains(needle),
            "pages.yml must check for {needle} in the built site: {why}",
        );
    }
}

/// A deploy that can run from any branch is a deploy a stranger can run.
///
/// `pages: write` and `id-token: write` are the two permissions that let a job
/// overwrite the live site, and the token behind them is minted for the
/// repository however the workflow was reached. Publishing on every merge to
/// master is the point; publishing from anywhere else is not, so the invariant
/// is the narrow one that survives the convenience — master is the only ref that
/// reaches the live site, and the publishing permissions live in the one job
/// gated on it.
#[test]
fn only_master_can_reach_the_live_site() {
    let Some(pages) = workflow("pages.yml") else {
        return;
    };

    // The trigger must be the named branch, not a bare `push:`, which deploys
    // from every branch that exists — including a contributor's.
    assert!(
        pages.contains("branches: [master]"),
        "pages.yml must trigger on `branches: [master]`, not a bare `push:`; a bare push deploys \
         from every branch, including other people's",
    );
    // No tag trigger: a tagged commit that never reached master would be
    // published to the live site.
    assert!(
        !pages.contains("tags:"),
        "pages.yml must not also deploy on tags; a tagged commit that never reached master would \
         be published",
    );

    // The deploy job's own gate, named so this cannot be satisfied by a gate on
    // some other job. This is the check that still holds if the trigger is later
    // widened by accident.
    let deploy_job = pages
        .split("\n  deploy:")
        .nth(1)
        .expect("pages.yml must have a `deploy:` job");
    assert!(
        deploy_job.contains("if:") && deploy_job.contains("github.ref == 'refs/heads/master'"),
        "the `deploy` job must be gated on the build being for master",
    );
    // And the environment, which is what makes the repository's own approval
    // rules able to hold the live site back.
    assert!(
        deploy_job.contains("github-pages"),
        "the deploy must run in the `github-pages` environment, or a protected branch or a \
         reviewer-gated environment cannot stop it",
    );

    // The permissions that can publish belong to that job alone, never
    // workflow-wide: granted globally, a build step or a third-party action added
    // later can spend them, and the build job is the one that runs other people's
    // scripts.
    assert!(
        !pages.contains("pages: write") || deploy_job.contains("pages: write"),
        "the workflow declares `pages: write` somewhere other than the deploy job",
    );
    let build_job = pages
        .split("\n  build:")
        .nth(1)
        .and_then(|after| after.split("\n  deploy:").next())
        .expect("pages.yml must have a `build:` job");
    assert!(
        !build_job.contains("pages: write") && !build_job.contains("id-token: write"),
        "the `build` job must not hold pages: write or id-token: write; those belong to `deploy`, \
         where the master gate is",
    );
}

/// The deploy has to be handed something the upload actually produced.
///
/// `deploy-pages` v5 takes `artifact_name`. There is no `artifact_id` input:
/// passing one is reported as `Unexpected input(s) 'artifact_id'`, warned about,
/// and ignored — the action then falls back to its own default, which is only
/// the right answer while the upload side defaults to the same string. Change
/// one side and the deploy finds no artifact and fails with a bare
/// `HttpError: Not Found` that names nothing useful.
#[test]
fn the_deploy_is_handed_the_artifact_the_build_uploaded() {
    let Some(pages) = workflow("pages.yml") else {
        return;
    };
    let live = strip_yaml_comments(&pages);

    // The input v5 does not have. Its presence is a warning at run time and
    // never an error, so nothing else would ever report it.
    assert!(
        !live.contains("artifact_id:"),
        "pages.yml passes `artifact_id` to deploy-pages v5, which has no such input; it is warned \
         about and ignored, leaving the deploy to guess the artifact name",
    );
    // And the upload must be the Pages action, not `upload-artifact`: the Pages
    // artifact is a single tarball the deploy finds *by name*, so a plain
    // upload gives a green build and then a deploy that cannot find what it was
    // given.
    assert!(
        live.contains("upload-pages-artifact") && !live.contains("upload-artifact@"),
        "pages.yml must upload with `actions/upload-pages-artifact`, not `actions/upload-artifact`: \
         the former produces the tarball the deploy looks up by name, the latter does not",
    );

    // Both sides name the artifact the same way. Read the two keys out of the
    // live text rather than asserting a fixed string, so the invariant is the
    // *agreement* and not the particular name.
    //
    // Only the two keys are read, and only where they are the artifact's own: the
    // file also carries the workflow's `name:` and every step's `name:`, and a
    // prefix match picks up whichever comes first. `artifact_name:` is unique, and
    // the upload's `name:` is the one indented ten spaces — a step's own `name:`
    // is eight, and the `deploy` job's `name:` is four.
    let name_of = |key: &str, indent: usize| {
        live.lines().find_map(|line| {
            let prefix = format!("{}{key}: ", " ".repeat(indent));
            let rest = line.strip_prefix(prefix.as_str())?;
            Some(rest.trim().trim_matches('"').to_owned())
        })
    };
    let uploaded = name_of("name", 10).unwrap_or_else(|| {
        panic!("pages.yml must state the upload step's artifact `name:` so the deploy can match it")
    });
    let deployed = name_of("artifact_name", 10).unwrap_or_else(|| {
        panic!(
            "pages.yml must pass `artifact_name:` to deploy-pages, or it uses a default that can \
             drift from the upload"
        )
    });
    assert_eq!(
        uploaded, deployed,
        "the artifact the build uploads ({uploaded:?}) and the one the deploy asks for ({deployed:?}) \
         must be the same name",
    );
    // A hardcoded `github-pages` on one side only would pass the agreement above
    // while renaming the other away from the action's default, which is the case
    // the bare `HttpError: Not Found` comes from. Naming it on both sides is the
    // cheap way to keep the two visibly one decision.
    assert_eq!(
        uploaded, "github-pages",
        "the artifact name should stay `github-pages`, the default both actions agree on; \
         renaming it is fine, but rename it on both sides in the same commit",
    );
}

/// The Pages deployment is a separate workflow, not extra steps in `build.yml`.
///
/// `build.yml` runs on every pull request, including from forks, where
/// `pages: write`, `id-token: write` and the `github-pages` environment do not
/// exist. A deploy folded into it is a workflow that can fail to run for every
/// ordinary pull request, and one that is reachable from every branch that can
/// open one. This is the check that the split stays a split: `build.yml` must
/// still be able to build the site on its own, and must not have grown a deploy.
#[test]
fn the_deploy_is_not_folded_into_the_read_only_build() {
    let Some(build) = workflow("build.yml") else {
        return;
    };
    let live = strip_yaml_comments(&build);

    for forbidden in [
        "deploy-pages",
        "upload-pages-artifact",
        "pages: write",
        "id-token: write",
    ] {
        assert!(
            !live.contains(forbidden),
            "build.yml references {forbidden}; it runs on pull requests from forks, where the Pages \
             permissions and environment do not exist, so the deploy must live in pages.yml",
        );
    }
    // And the build it does still has to be able to produce the site, or the
    // split has cost the read-only check rather than adding a second one.
    assert!(
        live.contains("cargo build --locked --lib --target wasm32-unknown-unknown")
            && live.contains("wasm-bindgen --target web")
            && live.contains("cargo build --release --locked"),
        "build.yml must still run both builds itself; promoting its artifact would lose the \
         executable bit and the Pages tarball layout, and the check it runs is the only one that \
         sees dist/",
    );
}
