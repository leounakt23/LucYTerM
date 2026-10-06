//! Terminal emulation: vte parser front-end + grid model.
//!
//! The custom wgpu renderer (glyph atlas + instanced cell quads) lands in a
//! later prompt per the staged plan in `doc/tech_stack.md` risk R3; the grid
//! model and parser integration land first so logic is testable headlessly.

pub mod atlas;
pub mod emulator;
pub mod grid;
pub mod input;
mod parser;
pub mod renderer;

pub use atlas::{AtlasError, GlyphAtlas, GlyphEntry, GlyphKey, ATLAS_SIZE};
pub use emulator::{MouseMode, Terminal, TerminalMode};
pub use grid::{Attributes, Cell, Color, Cursor, CursorShape, Grid, Pos, Selection, SelectionMode};
pub use renderer::{
    cell_size_for_font, color_to_rgba, grid_for_viewport, xterm256_to_rgb, CellInstance,
    RenderStats, RendererError, TerminalRenderer, Uniforms, DEFAULT_BG, DEFAULT_FG,
};
pub use vte;
