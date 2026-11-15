//! AutoSort core: a cross-platform, rule-based file automation engine.
//!
//! Everything here is GUI-free so it can be unit-tested and reused by the desktop
//! app, the headless daemon and the CLI.

pub mod actions;
pub mod engine;
pub mod fileinfo;
pub mod history;
pub mod kind;
pub mod matching;
pub mod model;
pub mod presets;
pub mod service;
pub mod store;
pub mod template;
pub mod undo;

pub use engine::Engine;
pub use fileinfo::FileInfo;
pub use history::History;
pub use kind::FileKind;
pub use model::*;
pub use service::Service;
