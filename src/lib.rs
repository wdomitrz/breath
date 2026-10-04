// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! Breath — a paced-breathing pacer, in Rust.
//!
//! A browser application and nothing else: no server, no binary. `cargo build`
//! writes the whole static site into `dist/`.
//!
//! * [`pacer`] holds the app's decisions — the phase, the orb, the pace, the
//!   validation — as plain data with plain tests, and never mentions the DOM.
//! * `ui` is the only module that knows a browser exists, and is compiled for
//!   `wasm32` only, so the host build and `cargo test` never pull in
//!   `web-sys`. Written as code rather than as a doc link: the module is absent
//!   from a host build, so `[`ui`]` is an unresolvable link there, and
//!   `-D warnings` turns that into a documentation failure.
//!
//! The site's JavaScript is only plumbing that cannot be Rust: the generated
//! `wasm-bindgen` bindings, a six-line dynamic import in the shell, and the
//! service worker's cache lifecycle.

pub mod pacer;

#[cfg(target_arch = "wasm32")]
pub mod ui;
