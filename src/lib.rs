// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! Breath — a paced-breathing pacer, in Rust.
//!
//! The app is a browser application and nothing else: there is no server, no
//! binary, no unit. `cargo build` writes the whole static site into `dist/`.
//!
//! * [`pacer`] holds the app's decisions — the phase, the orb, the pace, the
//!   validation — as plain data with plain tests, and never mentions the DOM.
//! * `ui` is the only module that knows a browser exists. It is compiled for
//!   `wasm32` only, so the host build and `cargo test` never pull in `web-sys`.
//!   (`ui` is written as code rather than as a doc link deliberately: the module
//!   does not exist in a host build, so `[`ui`]` is an unresolvable link there
//!   and `-D warnings` turns that into a documentation failure.)
//!
//! All of the JavaScript in the built site is plumbing that cannot be Rust: the
//! generated `wasm-bindgen` bindings, a six-line dynamic import in the shell,
//! and the service worker's cache lifecycle.

pub mod pacer;

#[cfg(target_arch = "wasm32")]
pub mod ui;
