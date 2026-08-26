use std::collections::HashSet;

use ox_core::mesher::build_mesh;
use ox_core::world::World;
use ox_render::Renderer;

use crate::view::VIEW;
use ox_app::game::Game;

pub(crate) struct Streamer {
    offsets: Vec<(i32, i32, i32)>,
}

fn cheb(dx: i32, dz: i32) -> i32 {
    dx.abs().max(dz.abs())
}

impl Streamer {
    pub(crate) fn new() -> Self {
        let mut offsets: Vec<(i32, i32, i32)> = Vec::new();
        for dx in -(VIEW.render_dist + 1)..=(VIEW.render_dist + 1) {
            for dz in -(VIEW.render_dist + 1)..=(VIEW.render_dist + 1) {
                offsets.push((dx, dz, cheb(dx, dz)));
            }
        }
        offsets.sort_by_key(|&(dx, dz, _)| dx * dx + dz * dz);
        Self { offsets }
    }

    pub(crate) fn initial(
        &self,
        game: &mut Game,
        renderer: &mut Renderer,
        meshed: &mut HashSet<(i32, i32)>,
    ) {
        self.stream(game, renderer, meshed, usize::MAX, usize::MAX);
    }

    pub(crate) fn stream(
        &self,
        game: &mut Game,
        renderer: &mut Renderer,
        meshed: &mut HashSet<(i32, i32)>,
        data_budget: usize,
        mesh_budget: usize,
    ) {
        let (pcx, pcz) = game.player_chunk();
        let mut data_left = data_budget;
        let mut mesh_left = mesh_budget;
        for &(dx, dz, ring) in &self.offsets {
            if data_left == 0 && mesh_left == 0 {
                break;
            }
            let (cx, cz) = (pcx + dx, pcz + dz);
            let has_data = game
                .world
                .get_chunk(cx, cz)
                .is_some_and(ox_core::world::Chunk::has_data);
            if !has_data {
                if data_left > 0 {
                    game.world.ensure_data(cx, cz);
                    data_left -= 1;
                }
                continue;
            }
            if ring <= VIEW.render_dist
                && mesh_left > 0
                && !meshed.contains(&(cx, cz))
                && neighbors_ready(&game.world, cx, cz)
            {
                let mesh = build_mesh(&game.world, cx, cz);
                renderer.upload_chunk((cx, cz), &mesh);
                meshed.insert((cx, cz));
                mesh_left -= 1;
            }
        }
    }

    pub(crate) fn unload(
        game: &mut Game,
        renderer: &mut Renderer,
        meshed: &mut HashSet<(i32, i32)>,
    ) {
        let (pcx, pcz) = game.player_chunk();
        meshed.retain(|&(cx, cz)| {
            let keep = cheb(cx - pcx, cz - pcz) <= VIEW.render_dist + 2;
            if !keep {
                renderer.remove_chunk((cx, cz));
            }
            keep
        });
        let stale: Vec<(i32, i32)> = game
            .world
            .loaded_chunks()
            .filter(|&(cx, cz)| cheb(cx - pcx, cz - pcz) > VIEW.render_dist + 2)
            .collect();
        for (cx, cz) in stale {
            game.world.remove_chunk(cx, cz);
        }
    }
}

fn neighbors_ready(world: &World, cx: i32, cz: i32) -> bool {
    for dx in -1..=1 {
        for dz in -1..=1 {
            let ready = world
                .get_chunk(cx + dx, cz + dz)
                .is_some_and(ox_core::world::Chunk::has_data);
            if !ready {
                return false;
            }
        }
    }
    true
}
