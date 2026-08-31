// Library target for benchmarks, integration tests, and the gpui-petruterm
// binary. `main.rs` (the existing wgpu/winit binary) keeps its own private
// module declarations and is unaffected by this — it's a separate crate
// compiling the same source files.
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
