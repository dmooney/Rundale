//! State-frame rendering: turn EngineState + narrative into a readable SVG and
//! a deterministic PNG, with a non-blank content guard (rule #14).

pub mod renderer;

pub use renderer::{StateFrame, render};
