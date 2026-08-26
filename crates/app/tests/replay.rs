//! Replay and determinism tests driven through the public `ox_app` API only.

use ox_app::game::{Effect, Game, LOAD_RADIUS};
use ox_app::harness::{Applied, FIXED_DT, GameHarness, TEST_SEED};
use ox_app::input::InputState;
use ox_app::test_server::Command;
use ox_core::blocks::{AIR, GRASS, HOTBAR, STONE, break_seconds, material};
use ox_core::generation::SEA_LEVEL;
use ox_core::generation::settings::WorldGenSettings;
use ox_core::generation::terrain::column_info;
use ox_core::world::Chunk;

struct OwnedHarness {
    input: InputState,
    game: Game,
}

fn owned_harness() -> OwnedHarness {
    OwnedHarness {
        input: InputState::new(),
        game: Game::with_seed(TEST_SEED),
    }
}

/// Same seed and startup state as the socket server (`serve` grabs input).
fn owned_server_harness() -> OwnedHarness {
    OwnedHarness {
        input: InputState::new(),
        game: Game::new(),
    }
}

/// Replays a fixture file: a bare JSON array of wire-format commands.
fn replay_fixture(h: &mut GameHarness<'_>, fixture: &str) {
    let parsed = serde_json::from_str::<Vec<Command>>(fixture);
    assert!(
        parsed.is_ok(),
        "fixture parse error: {:?}",
        parsed.as_ref().err()
    );
    h.grab();
    let commands: Vec<Command> = parsed.into_iter().flatten().collect();
    for command in &commands {
        let applied = h.apply(command);
        assert!(!matches!(applied, Applied::Shell), "shell verb in fixture");
    }
}

#[test]
fn spawn_rests_on_known_ground() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.run(2.0);
        let p = h.player();
        let expected = column_info(8, 8, TEST_SEED, &WorldGenSettings::DEFAULTS).h as f32 + 1.0;
        assert!(p.on_ground, "must settle on ground");
        assert!(
            p.pos[1] >= expected && p.pos[1] - expected < 0.01,
            "y={} expected ~{expected}",
            p.pos[1]
        );
    }
}

#[test]
fn known_seed_pins_terrain() {
    let info = column_info(8, 8, TEST_SEED, &WorldGenSettings::DEFAULTS);
    assert!(
        info.h > 4 && info.h < 54,
        "spawn column height sane: {}",
        info.h
    );
    let surface = if info.sandy {
        ox_core::blocks::SAND
    } else if info.snowy {
        ox_core::blocks::SNOW
    } else {
        GRASS
    };
    let owned = owned_harness();
    assert_eq!(owned.game.world.get_block(8, info.h, 8), surface);
    assert_eq!(
        owned.game.world.get_block(8, 0, 8),
        ox_core::blocks::BEDROCK
    );
}

#[test]
fn w_key_moves_player_forward() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.run(1.0);
        let before = h.player().pos;
        h.look(0.0, 0.0);
        h.hold("W");
        h.tick(60);
        h.release("W");
        let after = h.player().pos;
        let dz = before[2] - after[2];
        assert!(dz > 0.5 && dz < 4.0, "walked dz={dz}");
        assert!(
            (after[0] - before[0]).abs() < 1e-4,
            "no strafe drift dx={}",
            after[0] - before[0]
        );
    }
}

#[test]
fn keys_ignored_while_menu_open() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.run(1.0);
        let before = h.player().pos;
        h.hold("W");
        h.tick(60);
        h.release("W");
        let after = h.player().pos;
        for k in 0..3 {
            assert!(
                (before[k] - after[k]).abs() < 1e-6,
                "menu open must block movement input: axis {k}"
            );
        }
    }
}

#[test]
fn clicks_while_ungrabbed_queue_no_actions() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.mouse_press("left");
    }
    assert!(
        owned.input.actions.is_empty(),
        "an ungrabbed click must not queue an action"
    );
}

#[test]
fn jump_rises_and_lands() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.run(1.0);
        let y0 = h.player().pos[1];
        h.hold("Space");
        h.tick(2);
        h.release("Space");
        let mut max_y = y0;
        for _ in 0..360 {
            h.tick(1);
            max_y = max_y.max(h.player().pos[1]);
            if h.player().on_ground {
                break;
            }
        }
        let gain = max_y - y0;
        assert!(gain > 1.0 && gain < 1.6, "jump gain={gain}");
        assert!(h.player().on_ground);
        assert!((h.player().pos[1] - y0).abs() < 0.05);
    }
}

#[test]
fn fly_toggles_and_moves_up() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.run(1.0);
        h.press("F");
        h.tick(1);
        assert!(h.player().flying);
        let y0 = h.player().pos[1];
        h.hold("Space");
        h.tick(30);
        h.release("Space");
        assert!(h.player().pos[1] > y0 + 1.0, "fly ascent");
        h.press("F");
        h.tick(1);
        assert!(!h.player().flying);
    }
}

#[test]
fn digit_selects_hotbar_slot() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.press("3");
        h.tick(1);
        assert_eq!(h.player().selected, 2);
        assert_eq!(HOTBAR[2], STONE);
    }
}

#[test]
fn held_attack_breaks_targeted_block_after_its_break_time() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.run(1.0);
        h.look(0.0, -1.5);
        let (x, y, z, block, _) = h.target_cell().expect("target below feet");
        let secs = break_seconds(material(block)).expect("ground is breakable");
        let ticks = (secs / FIXED_DT).round() as usize;
        h.mouse_press("left");
        h.tick(1);
        assert_eq!(h.block_at(x, y, z), block, "one tick must not break it");
        h.tick(ticks - 1);
        assert_eq!(h.block_at(x, y, z), AIR);
        h.mouse_release("left");
    }
}

#[test]
fn mouse_release_command_stops_breaking() {
    let mut owned = owned_harness();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    h.grab();
    h.run(1.0);
    h.look(0.0, -1.5);
    let (x, y, z, block, _) = h.target_cell().expect("target below feet");
    h.apply(&Command::Mouse {
        button: "left".into(),
    });
    h.apply(&Command::Tick { n: 5 });
    assert_eq!(h.game.breaking.map(|b| b.cell), Some([x, y, z]));
    let applied = h.apply(&Command::MouseRelease {
        button: "left".into(),
    });
    assert!(matches!(applied, Applied::Reply(_)));
    h.apply(&Command::Tick { n: 400 });
    assert_eq!(h.block_at(x, y, z), block);
    assert!(h.game.breaking.is_none());
}

#[test]
fn place_puts_selected_block_in_world() {
    let mut owned = owned_harness();
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.grab();
        h.run(1.0);
        h.look(0.0, -0.2);
        h.hold("W");
        h.tick(120);
        h.release("W");
        h.look(0.0, -0.4);
        let cell = h.target_cell();
        assert!(cell.is_some(), "need a target to place on");
        let Some((x, y, z, _, face)) = cell else {
            return;
        };
        h.press("3");
        h.tick(1);
        h.mouse_press("right");
        h.tick(1);
        let placed = h.block_at(x + face[0], y + face[1], z + face[2]);
        assert_eq!(placed, STONE, "stone placed at hit+face");
        assert_eq!(h.block_at(x, y, z), GRASS, "hit block untouched");
        let expected = Effect::Placed {
            cell: [x + face[0], y + face[1], z + face[2]],
            block: STONE,
        };
        assert!(h.game.pending_effects.contains(&expected));
    }
}

fn script(h: &mut GameHarness<'_>) {
    h.run(1.0);
    h.look(0.0, -0.1);
    h.hold("W");
    h.tick(45);
    h.release("W");
    h.press("F");
    h.tick(20);
}

const GOLDEN_REPLAY_HASH_424242: u64 = 9_930_984_824_774_689_304;

/// Cells the golden replay hash absorbs after the player state.
const DIGEST_CELLS: [[i32; 3]; 3] = [[8, 30, 8], [9, 33, 9], [12, 28, 4]];

/// Cells every fixture digest absorbs: [`DIGEST_CELLS`] plus the two blocks
/// walk-break-place edits, so place and break stay visible to the pins.
const FIXTURE_DIGEST_CELLS: [[i32; 3]; 5] =
    [[8, 30, 8], [9, 33, 9], [12, 28, 4], [8, 31, 5], [8, 30, 6]];

#[test]
fn golden_replay_hash_is_pinned() {
    let mut owned = owned_harness();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    script(&mut h);
    assert_eq!(
        h.digest(&DIGEST_CELLS),
        GOLDEN_REPLAY_HASH_424242,
        "the shared digest routine must keep the pinned replay hash"
    );
}

const WALK_BREAK_PLACE_DIGEST: u64 = 14_220_616_296_191_036_127;

#[test]
fn walk_break_place_fixture_digest_is_pinned() {
    let mut owned = owned_server_harness();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    replay_fixture(&mut h, include_str!("replays/walk-break-place.json"));
    assert_eq!(h.block_at(8, 30, 6), AIR, "held attack broke the grass");
    assert_ne!(h.block_at(8, 29, 6), AIR, "cooldown spared the block below");
    assert_eq!(h.digest(&FIXTURE_DIGEST_CELLS), WALK_BREAK_PLACE_DIGEST);
}

const FLY_FAR_DIGEST: u64 = 13_964_549_411_563_690_023;

#[test]
fn fly_far_fixture_digest_is_pinned() {
    let mut owned = owned_server_harness();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    replay_fixture(&mut h, include_str!("replays/fly-far.json"));
    assert_eq!(h.digest(&FIXTURE_DIGEST_CELLS), FLY_FAR_DIGEST);
}

#[test]
fn same_seed_and_script_give_bit_identical_outcome() {
    let mut a = owned_harness();
    let mut b = owned_harness();
    {
        let mut ha = GameHarness::wrap(&mut a.input, &mut a.game);
        let mut hb = GameHarness::wrap(&mut b.input, &mut b.game);
        script(&mut ha);
        script(&mut hb);
        assert_eq!(ha.player(), hb.player(), "deterministic replay must match");
    }
    for (x, y, z) in [(8, 30, 8), (9, 33, 9), (12, 28, 4)] {
        assert_eq!(
            a.game.world.get_block(x, y, z),
            b.game.world.get_block(x, y, z)
        );
    }
}

fn open_ground_column(seed: i32, from: (i32, i32)) -> Option<(i32, i32, i32)> {
    let settings = &WorldGenSettings::DEFAULTS;
    (0..64)
        .flat_map(|dz| (0..64).map(move |dx| (from.0 + dx, from.1 + dz)))
        .find_map(|(x, z)| {
            let info = column_info(x, z, seed, settings);
            let treeless = (-3..=3)
                .all(|ox| (-3..=3).all(|oz| !column_info(x + ox, z + oz, seed, settings).tree));
            (treeless && info.h > SEA_LEVEL + 1).then_some((x, z, info.h))
        })
}

#[test]
fn teleport_far_from_spawn_lands_on_generated_ground() {
    let (x, z, h) = open_ground_column(TEST_SEED, (1000, -2000)).expect("open column");
    let mut owned = owned_harness();
    let mut harness = GameHarness::wrap(&mut owned.input, &mut owned.game);
    harness.teleport([x as f32 + 0.5, 60.0, z as f32 + 0.5]);
    harness.run(5.0);
    let p = harness.player();
    let expected = h as f32 + 1.0;
    assert!(
        p.on_ground,
        "must land on generated terrain, y={}",
        p.pos[1]
    );
    assert!(
        p.pos[1] >= expected && p.pos[1] - expected < 0.01,
        "y={} expected ~{expected}",
        p.pos[1]
    );
}

#[test]
fn update_loads_every_chunk_within_load_radius() {
    let mut owned = owned_harness();
    {
        let mut harness = GameHarness::wrap(&mut owned.input, &mut owned.game);
        harness.teleport([-500.0, 60.0, 700.0]);
        harness.tick(1);
    }
    let (cx, cz) = owned.game.player_chunk();
    for dz in -LOAD_RADIUS..=LOAD_RADIUS {
        for dx in -LOAD_RADIUS..=LOAD_RADIUS {
            let loaded = owned
                .game
                .world
                .get_chunk(cx + dx, cz + dz)
                .is_some_and(Chunk::has_data);
            assert!(loaded, "chunk ({}, {}) must be loaded", cx + dx, cz + dz);
        }
    }
}

#[test]
fn day_time_advances_with_ticks() {
    let mut owned = owned_harness();
    let t0 = owned.game.time;
    {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        h.tick(120);
    }
    assert!(
        (owned.game.time - t0 - 1.0).abs() < 1e-3,
        "120 ticks = 1s of day time: t0={t0} t1={}",
        owned.game.time
    );
}

#[test]
fn digest_command_reply_carries_the_pinned_walk_digest() {
    let mut owned = owned_server_harness();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    replay_fixture(&mut h, include_str!("replays/walk-break-place.json"));
    let applied = h.apply(&Command::Digest {
        cells: FIXTURE_DIGEST_CELLS.to_vec(),
    });
    let digest = match &applied {
        Applied::Reply(result) => result["digest"].as_u64(),
        Applied::Ticked { .. } | Applied::Shell => None,
    };
    assert_eq!(
        digest,
        Some(WALK_BREAK_PLACE_DIGEST),
        "apply(Digest) must agree with the pin"
    );
}
