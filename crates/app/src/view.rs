//! View, streaming-budget, and input-feel tuning for the binary.

use ox_core::generation::CHUNK_AXIS;

pub(crate) struct ViewSettings {
    /// Window size at start and headless render target size, in pixels.
    pub(crate) window_size: (u32, u32),
    /// Chebyshev radius of meshed chunks around the player.
    pub(crate) render_dist: i32,
    /// Vertical field of view in degrees.
    pub(crate) fov: f32,
    /// Far clip distance of the perspective projection, in blocks.
    pub(crate) far_plane: f32,
    /// Fog start offset from the meshed ring edge; negative starts fog inside it.
    pub(crate) fog_near_offset: i32,
    /// Full-fog offset from the meshed ring edge; negative keeps full fog inside it.
    pub(crate) fog_far_offset: i32,
    /// Chunk-data generations allowed per streaming pass.
    pub(crate) data_budget: usize,
    /// Chunk mesh uploads allowed per streaming pass.
    pub(crate) mesh_budget: usize,
    /// Frames between unload sweeps.
    pub(crate) unload_interval: u32,
    /// Look radians per mouse device unit.
    pub(crate) mouse_sensitivity: f32,
    /// How long the selected-item name stays visible before fading, in ms.
    pub(crate) item_name_ms: u64,
    /// Seconds the selected-item name takes to fade out.
    pub(crate) item_name_fade_s: f32,
}

impl ViewSettings {
    /// Fog start distance in blocks.
    pub(crate) const fn fog_near(&self) -> f32 {
        (self.render_dist * CHUNK_AXIS + self.fog_near_offset) as f32
    }

    /// Distance at which fog is fully opaque, in blocks.
    pub(crate) const fn fog_far(&self) -> f32 {
        (self.render_dist * CHUNK_AXIS + self.fog_far_offset) as f32
    }
}

/// The shipped view tuning.
pub(crate) const VIEW: ViewSettings = ViewSettings {
    window_size: (1280, 720),
    render_dist: 6,
    fov: 75.0,
    far_plane: 300.0,
    fog_near_offset: -22,
    fog_far_offset: -4,
    data_budget: 6,
    mesh_budget: 3,
    unload_interval: 120,
    mouse_sensitivity: 0.0022,
    item_name_ms: 1400,
    item_name_fade_s: 0.35,
};
