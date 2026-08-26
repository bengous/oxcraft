//! Block registry: every placeable block, its tiles and physical flags.
//!
//! The registry is a compile-time [`REGISTRY`] table indexed by [`BlockId`];
//! lookup helpers never fail because every id below
//! `Block::Water as u8 + 1` has a definition.

use crate::atlas;

/// Numeric block identifier, indexes into [`REGISTRY`].
pub type BlockId = u8;
/// Numeric texture-tile identifier, indexes into the generated atlas.
pub type TileId = u8;

/// How a block is drawn by the renderer and meshed by the mesher.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderKind {
    /// Not drawn at all (air).
    Air,
    /// Fully solid geometry with back-face culling.
    Opaque,
    /// Drawn with alpha cutout (leaves, glass).
    Cutout,
    /// Translucent, non-colliding volume (water).
    Liquid,
}

/// What a block is made of: decides its break time, dig sound and debris.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    /// Stone, cobblestone.
    Stone,
    /// Dirt, grass.
    Dirt,
    /// Sand.
    Sand,
    /// Logs, planks.
    Wood,
    /// Leaf canopy.
    Leaves,
    /// Glass.
    Glass,
    /// Snow cover.
    Snow,
    /// Never breaks: air, bedrock, water.
    Unbreakable,
}

/// Seconds of held attack needed to break a block of `material`; `None`
/// when it never breaks.
#[must_use]
pub const fn break_seconds(material: Material) -> Option<f32> {
    match material {
        Material::Snow | Material::Leaves => Some(0.25),
        Material::Glass => Some(0.3),
        Material::Dirt | Material::Sand => Some(0.4),
        Material::Wood => Some(0.8),
        Material::Stone => Some(1.2),
        Material::Unbreakable => None,
    }
}

/// Static definition of one block kind: appearance plus gameplay flags.
#[derive(Clone, Copy)]
pub struct BlockDef {
    /// Display name shown in HUD/debug UIs.
    pub name: &'static str,
    /// Rendering category.
    pub render: RenderKind,
    /// Atlas tile per cube face, order: +X, -X, +Y, -Y, +Z, -Z.
    pub tiles: [TileId; 6],
    /// Whether entities collide with this block.
    pub solid: bool,
    /// Whether this block hides neighboring faces when meshing.
    pub opaque: bool,
    /// Whether this block darkens ambient occlusion around it.
    pub ao_cast: bool,
    /// What the block is made of.
    pub material: Material,
}

const fn air_def() -> BlockDef {
    BlockDef {
        name: "Air",
        render: RenderKind::Air,
        tiles: [0; 6],
        solid: false,
        opaque: false,
        ao_cast: false,
        material: Material::Unbreakable,
    }
}

const fn cube(name: &'static str, tile: TileId, material: Material) -> BlockDef {
    textured(name, [tile; 6], material)
}

const fn textured(name: &'static str, tiles: [TileId; 6], material: Material) -> BlockDef {
    BlockDef {
        name,
        render: RenderKind::Opaque,
        tiles,
        solid: true,
        opaque: true,
        ao_cast: true,
        material,
    }
}

const fn cutout(name: &'static str, tile: TileId, material: Material) -> BlockDef {
    BlockDef {
        render: RenderKind::Cutout,
        opaque: false,
        ao_cast: false,
        ..cube(name, tile, material)
    }
}

const fn liquid(name: &'static str, tile: TileId) -> BlockDef {
    BlockDef {
        render: RenderKind::Liquid,
        solid: false,
        opaque: false,
        ao_cast: false,
        ..cube(name, tile, Material::Unbreakable)
    }
}

/// Every block kind; discriminants double as [`BlockId`] values.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Block {
    /// Empty space.
    Air,
    /// Grass-topped dirt.
    Grass,
    /// Plain dirt.
    Dirt,
    /// Stone.
    Stone,
    /// Beach sand.
    Sand,
    /// Oak log.
    Log,
    /// Leaf canopy.
    Leaves,
    /// Wooden planks.
    Plank,
    /// Cobblestone.
    Cobble,
    /// Glass pane block.
    Glass,
    /// Snow-topped dirt (peaks).
    Snow,
    /// Unbreakable floor layer.
    Bedrock,
    /// Translucent water.
    Water,
}

/// [`Block::Air`] as a [`BlockId`].
pub const AIR: BlockId = Block::Air as u8;
/// [`Block::Grass`] as a [`BlockId`].
pub const GRASS: BlockId = Block::Grass as u8;
/// [`Block::Dirt`] as a [`BlockId`].
pub const DIRT: BlockId = Block::Dirt as u8;
/// [`Block::Stone`] as a [`BlockId`].
pub const STONE: BlockId = Block::Stone as u8;
/// [`Block::Sand`] as a [`BlockId`].
pub const SAND: BlockId = Block::Sand as u8;
/// [`Block::Log`] as a [`BlockId`].
pub const LOG: BlockId = Block::Log as u8;
/// [`Block::Leaves`] as a [`BlockId`].
pub const LEAVES: BlockId = Block::Leaves as u8;
/// [`Block::Plank`] as a [`BlockId`].
pub const PLANK: BlockId = Block::Plank as u8;
/// [`Block::Cobble`] as a [`BlockId`].
pub const COBBLE: BlockId = Block::Cobble as u8;
/// [`Block::Glass`] as a [`BlockId`].
pub const GLASS: BlockId = Block::Glass as u8;
/// [`Block::Snow`] as a [`BlockId`].
pub const SNOW: BlockId = Block::Snow as u8;
/// [`Block::Bedrock`] as a [`BlockId`].
pub const BEDROCK: BlockId = Block::Bedrock as u8;
/// [`Block::Water`] as a [`BlockId`].
pub const WATER: BlockId = Block::Water as u8;

/// All block definitions, ordered by [`BlockId`]; see [`def`] for lookup.
pub static REGISTRY: &[BlockDef] = &[
    air_def(),
    textured(
        "Grass",
        [
            atlas::T_GRASS_SIDE,
            atlas::T_GRASS_SIDE,
            atlas::T_GRASS_TOP,
            atlas::T_DIRT,
            atlas::T_GRASS_SIDE,
            atlas::T_GRASS_SIDE,
        ],
        Material::Dirt,
    ),
    cube("Dirt", atlas::T_DIRT, Material::Dirt),
    cube("Stone", atlas::T_STONE, Material::Stone),
    cube("Sand", atlas::T_SAND, Material::Sand),
    textured(
        "Oak Log",
        [
            atlas::T_LOG_SIDE,
            atlas::T_LOG_SIDE,
            atlas::T_LOG_TOP,
            atlas::T_LOG_TOP,
            atlas::T_LOG_SIDE,
            atlas::T_LOG_SIDE,
        ],
        Material::Wood,
    ),
    cutout("Leaves", atlas::T_LEAVES, Material::Leaves),
    cube("Planks", atlas::T_PLANK, Material::Wood),
    cube("Cobblestone", atlas::T_COBBLE, Material::Stone),
    cutout("Glass", atlas::T_GLASS, Material::Glass),
    textured(
        "Snowy Grass",
        [
            atlas::T_SNOW_SIDE,
            atlas::T_SNOW_SIDE,
            atlas::T_SNOW_TOP,
            atlas::T_DIRT,
            atlas::T_SNOW_SIDE,
            atlas::T_SNOW_SIDE,
        ],
        Material::Snow,
    ),
    cube("Bedrock", atlas::T_BEDROCK, Material::Unbreakable),
    liquid("Water", atlas::T_WATER),
];

/// Blocks offered in the player's build hotbar, in slot order.
pub const HOTBAR: [BlockId; 9] = [GRASS, DIRT, STONE, SAND, LOG, LEAVES, PLANK, COBBLE, GLASS];

/// Looks up a block's definition; `id` must be `< REGISTRY.len()`.
///
/// # Panics
///
/// Panics if `id` does not index [`REGISTRY`], which can only happen with a
/// corrupted id above `WATER`.
#[must_use]
pub fn def(id: BlockId) -> &'static BlockDef {
    &REGISTRY[id as usize]
}

/// Whether entities collide with the block.
#[must_use]
pub fn is_solid(id: BlockId) -> bool {
    def(id).solid
}

/// Whether the block hides neighboring faces when meshing.
#[must_use]
pub fn is_opaque(id: BlockId) -> bool {
    def(id).opaque
}

/// What the block is made of.
#[must_use]
pub fn material(id: BlockId) -> Material {
    def(id).material
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_every_id() {
        for id in 0..=WATER {
            let d = def(id);
            assert!(!d.name.is_empty());
            assert!(d.tiles.iter().all(|t| *t <= atlas::T_WATER));
        }
    }

    #[test]
    fn air_has_no_physics_and_no_render() {
        assert!(!is_solid(AIR));
        assert!(!is_opaque(AIR));
        assert_eq!(def(AIR).render, RenderKind::Air);
    }

    #[test]
    fn water_is_not_solid_but_renders_liquid() {
        assert!(!is_solid(WATER));
        assert_eq!(def(WATER).render, RenderKind::Liquid);
        assert!(break_seconds(material(WATER)).is_none());
    }

    #[test]
    fn bedrock_is_unbreakable() {
        assert!(break_seconds(material(BEDROCK)).is_none());
        assert!(break_seconds(material(STONE)).is_some());
    }

    #[test]
    fn materials_group_registry_entries() {
        assert_eq!(material(GRASS), Material::Dirt);
        assert_eq!(material(DIRT), Material::Dirt);
        assert_eq!(material(LOG), Material::Wood);
        assert_eq!(material(PLANK), Material::Wood);
        assert_eq!(material(COBBLE), Material::Stone);
        assert_eq!(material(SAND), Material::Sand);
        assert_eq!(material(LEAVES), Material::Leaves);
        assert_eq!(material(GLASS), Material::Glass);
        assert_eq!(material(SNOW), Material::Snow);
        assert_eq!(material(AIR), Material::Unbreakable);
        assert_eq!(material(WATER), Material::Unbreakable);
    }

    #[test]
    fn break_times_order_by_hardness() {
        let secs = |m| break_seconds(m).unwrap_or(f32::INFINITY);
        assert!(secs(Material::Snow) < secs(Material::Glass));
        assert!(secs(Material::Glass) < secs(Material::Dirt));
        assert!(secs(Material::Dirt) < secs(Material::Wood));
        assert!(secs(Material::Wood) < secs(Material::Stone));
        assert!(secs(Material::Stone) < secs(Material::Unbreakable));
    }
}
