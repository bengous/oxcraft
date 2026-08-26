//! World generation tuning values.

/// Tuning values consumed by every generation stage.
///
/// Stages read these instead of inline literals so related numbers share one
/// home; [`WorldGenSettings::DEFAULTS`] reproduces the historical world
/// bit-for-bit.
pub struct WorldGenSettings {
    /// Horizontal frequency of the low-frequency continent field.
    pub continent_scale: f32,
    /// Horizontal frequency of the high-frequency detail field.
    pub detail_scale: f32,
    /// Mean surface height before the amplitude terms apply.
    pub base_height: f32,
    /// Height gained from the squared continent field.
    pub continent_amplitude: f32,
    /// Height gained from the squared detail field.
    pub hill_amplitude: f32,
    /// Continent value where mountain growth begins.
    pub mountain_start: f32,
    /// Divisor shaping the ramp past [`Self::mountain_start`].
    pub mountain_sharpness: f32,
    /// Peak height added by the squared mountain term.
    pub mountain_amplitude: f32,
    /// Lowest surface height after clamping.
    pub height_min: i32,
    /// Highest surface height after clamping.
    pub height_max: i32,
    /// Probability that an eligible column carries a tree.
    pub tree_chance: f32,
    /// Shortest oak trunk in blocks.
    pub tree_min_h: i32,
    /// Tallest oak trunk in blocks.
    pub tree_max_h: i32,
    /// Surface height at and above which snow replaces grass.
    pub snow_line: i32,
    /// Columns this far above sea level count as beach sand.
    pub beach_band: i32,
    /// Horizontal cave-noise frequency.
    pub cave_scale_xz: f32,
    /// Vertical cave-noise frequency.
    pub cave_scale_y: f32,
    /// Half-width of the cave-noise band around its center.
    pub cave_band: f32,
    /// Lowest y cave carving may touch.
    pub cave_min_y: i32,
    /// Solid skin depth below the surface that carving may not breach.
    pub cave_skin: i32,
    /// Highest y cave carving may reach under any column.
    pub cave_top_cap: i32,
    /// Columns scanned beyond each chunk edge for cross-border canopies.
    pub tree_scan_margin: i32,
    /// Salt decorrelating the detail field from the continent field.
    pub detail_salt: i32,
    /// Salt decorrelating tree placement from other features.
    pub tree_salt: i32,
    /// Salt decorrelating cave noise from other features.
    pub cave_salt: i32,
    /// Salt decorrelating trunk height from tree placement.
    pub tree_height_salt: i32,
    /// Salt decorrelating canopy corner skips from other features.
    pub canopy_salt: i32,
}

impl WorldGenSettings {
    /// The shipped tuning; equals the historical inline literals exactly.
    pub const DEFAULTS: Self = Self {
        continent_scale: 0.006,
        detail_scale: 0.028,
        base_height: 18.0,
        continent_amplitude: 30.0,
        hill_amplitude: 22.0,
        mountain_start: 0.55,
        mountain_sharpness: 0.2,
        mountain_amplitude: 30.0,
        height_min: 4,
        height_max: 54,
        tree_chance: 0.02,
        tree_min_h: 4,
        tree_max_h: 6,
        snow_line: 44,
        beach_band: 1,
        cave_scale_xz: 0.09,
        cave_scale_y: 0.11,
        cave_band: 0.05,
        cave_min_y: 2,
        cave_skin: 4,
        cave_top_cap: 60,
        tree_scan_margin: 3,
        detail_salt: 0x3E7,
        tree_salt: 0x309,
        cave_salt: 0x14D,
        tree_height_salt: 0x7B,
        canopy_salt: 0x37,
    };
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::float_cmp,
        reason = "DEFAULTS must equal the historical literals bit-for-bit"
    )]
    use super::super::{CHUNK_AXIS, GenCtx, Stage, generate};
    use super::*;
    use crate::blocks::GRASS;
    use crate::generation::terrain;

    #[test]
    fn defaults_match_historical_literals() {
        let s = WorldGenSettings::DEFAULTS;
        assert_eq!(s.continent_scale, 0.006);
        assert_eq!(s.detail_scale, 0.028);
        assert_eq!(s.base_height, 18.0);
        assert_eq!(s.continent_amplitude, 30.0);
        assert_eq!(s.hill_amplitude, 22.0);
        assert_eq!(s.mountain_start, 0.55);
        assert_eq!(s.mountain_sharpness, 0.2);
        assert_eq!(s.mountain_amplitude, 30.0);
        assert_eq!(s.height_min, 4);
        assert_eq!(s.height_max, 54);
        assert_eq!(s.tree_chance, 0.02);
        assert_eq!(s.tree_min_h, 4);
        assert_eq!(s.tree_max_h, 6);
        assert_eq!(s.snow_line, 44);
        assert_eq!(s.beach_band, 1);
        assert_eq!(s.cave_scale_xz, 0.09);
        assert_eq!(s.cave_scale_y, 0.11);
        assert_eq!(s.cave_band, 0.05);
        assert_eq!(s.cave_min_y, 2);
        assert_eq!(s.cave_skin, 4);
        assert_eq!(s.cave_top_cap, 60);
        assert_eq!(s.tree_scan_margin, 3);
        assert_eq!(s.detail_salt, 0x3E7);
        assert_eq!(s.tree_salt, 0x309);
        assert_eq!(s.cave_salt, 0x14D);
        assert_eq!(s.tree_height_salt, 0x7B);
        assert_eq!(s.canopy_salt, 0x37);
    }

    #[test]
    fn terrain_stage_honors_non_default_settings() {
        let ctx = GenCtx {
            seed: 7,
            settings: WorldGenSettings {
                base_height: 30.0,
                continent_amplitude: 0.0,
                hill_amplitude: 0.0,
                mountain_amplitude: 0.0,
                ..WorldGenSettings::DEFAULTS
            },
        };
        let stages = [Stage {
            name: "terrain",
            run: terrain::run,
        }];
        let data = generate(&ctx, 0, 0, &stages);
        for lz in 0..CHUNK_AXIS {
            for lx in 0..CHUNK_AXIS {
                assert_eq!(data.get(lx, 30, lz), GRASS);
            }
        }
    }
}
