pub mod debug;
pub mod headless;
pub mod testing;
pub mod tui;
// Re-export all parish-core modules for backward compatibility
pub use parish_core::{config, error, inference, input, loading, npc, persistence, world};
