//! GPU renderer module — wgpu-based rendering pipeline.

pub mod glyph_atlas;
pub mod gpu;
pub mod overlay;

pub use gpu::GpuRenderer;
pub use overlay::OverlayManager;
