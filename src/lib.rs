// Library target for benchmarks, integration tests, and the gpui-petruterm
// binary. `main.rs` (the existing wgpu/winit binary) keeps its own private
// module declarations and is unaffected by this — it's a separate crate
// compiling the same source files.

// These modules are `pub` so `gpui-petruterm` (a second binary) can reach
// them, not because they're a deliberately-designed public API surface —
// `clippy::new_without_default` fires on newly publicly-reachable `new()`
// methods that were never meant to be part of an external API contract, so
// it doesn't apply here. Adding a dozen speculative `impl Default` blocks
// would be more code and more risk than this lint is worth in that context.
#![allow(clippy::new_without_default)]

pub mod app;
pub mod config;
pub mod font;
pub mod gpui_shell;
pub mod i18n;
pub mod llm;
pub mod platform;
pub mod renderer;
pub mod term;
pub mod ui;

rust_i18n::i18n!("locales");
