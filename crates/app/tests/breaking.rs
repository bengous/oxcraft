//! Hold-to-break mechanics driven through the harness.

use ox_app::game::{BREAK_COOLDOWN, Breaking, DIG_PERIOD, Effect, Game};
use ox_app::harness::{FIXED_DT, GameHarness, TEST_SEED};
use ox_app::input::InputState;
use ox_core::blocks::{AIR, BEDROCK, BlockId, STONE, break_seconds, material};

struct Owned {
    input: InputState,
    game: Game,
}

fn owned() -> Owned {
    Owned {
        input: InputState::new(),
        game: Game::with_seed(TEST_SEED),
    }
}

/// Ticks of held attack that break `block`; `None` for an unbreakable one.
fn break_ticks(block: BlockId) -> Option<usize> {
    break_seconds(material(block)).map(|secs| (secs / FIXED_DT).round() as usize)
}

/// Settles the player, then aims straight down at the block under the feet.
fn aim_below(h: &mut GameHarness<'_>) -> Option<([i32; 3], BlockId)> {
    h.grab();
    h.run(1.0);
    h.look(0.0, -1.5);
    h.target_cell()
        .map(|(x, y, z, block, _)| ([x, y, z], block))
}

#[test]
fn hold_breaks_on_the_rounded_tick() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    let (cell, block) = aim_below(&mut h).expect("target below feet");
    let ticks = break_ticks(block).expect("breakable target");
    h.mouse_press("left");
    h.tick(ticks - 1);
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), block);
    let progress = h.game.breaking.expect("still breaking").progress;
    assert!(progress > 0.9 && progress < 1.0, "progress={progress}");
    h.tick(1);
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), AIR);
    assert!(h.game.breaking.is_none());
}

#[test]
fn retargeting_resets_progress() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    let (cell, block) = aim_below(&mut h).expect("target below feet");
    let ticks = break_ticks(block).expect("breakable target");
    h.mouse_press("left");
    h.tick(ticks / 2);
    h.look(0.0, -0.9);
    h.tick(1);
    assert_ne!(h.game.breaking.map(|b| b.cell), Some(cell));
    h.look(0.0, -1.5);
    h.tick(ticks - 1);
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), block);
    h.tick(1);
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), AIR);
}

#[test]
fn unbreakable_target_never_breaks() {
    let mut owned = owned();
    let cell = {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        aim_below(&mut h).expect("target below feet").0
    };
    let _ = owned
        .game
        .world
        .set_block(cell[0], cell[1], cell[2], BEDROCK);
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    h.mouse_press("left");
    h.tick(360);
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), BEDROCK);
    assert!(h.game.breaking.is_none());
    assert!(h.game.pending_effects.is_empty());
}

#[test]
fn break_emits_dig_hits_then_broke() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    let (cell, block) = aim_below(&mut h).expect("target below feet");
    let ticks = break_ticks(block).expect("breakable target");
    h.mouse_press("left");
    h.tick(ticks);
    let effects = &h.game.pending_effects;
    assert_eq!(effects.first(), Some(&Effect::Dig { cell, block }));
    assert_eq!(effects.last(), Some(&Effect::Broke { cell, block }));
    let secs = break_seconds(material(block)).expect("breakable target");
    let digs = effects
        .iter()
        .filter(|e| matches!(e, Effect::Dig { .. }))
        .count();
    assert_eq!(digs, (secs / DIG_PERIOD).ceil() as usize);
}

#[test]
fn cooldown_delays_the_next_block() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    let (cell, block) = aim_below(&mut h).expect("target below feet");
    h.mouse_press("left");
    h.tick(break_ticks(block).expect("breakable target"));
    assert_eq!(h.block_at(cell[0], cell[1], cell[2]), AIR);
    let cooldown_ticks = (BREAK_COOLDOWN / FIXED_DT) as usize;
    h.tick(cooldown_ticks - 1);
    assert!(h.game.breaking.is_none());
    h.tick(2);
    let next = h.game.breaking.expect("digging the block below");
    assert_eq!(next.cell, [cell[0], cell[1] - 1, cell[2]]);
}

/// A staged break carries progress but no hit count, so the tick that
/// resumes it must not replay every hit that progress implies.
#[test]
fn resuming_a_staged_break_replays_no_dig_hits() {
    let mut owned = owned();
    let cell = {
        let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
        aim_below(&mut h).expect("target below feet").0
    };
    let _ = owned.game.world.set_block(cell[0], cell[1], cell[2], STONE);
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    h.game.breaking = Some(Breaking {
        cell,
        progress: 0.55,
    });
    h.game.pending_effects.clear();
    h.mouse_press("left");
    h.tick(1);
    let digs = h
        .game
        .pending_effects
        .iter()
        .filter(|e| matches!(e, Effect::Dig { .. }))
        .count();
    assert!(digs <= 1, "digs={digs}");
    assert_eq!(h.game.breaking.map(|b| b.cell), Some(cell));
}

/// The window can lose the pointer with the button down; the release then
/// arrives ungrabbed and must still drop the attack.
#[test]
fn releasing_the_button_off_view_drops_a_held_attack() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    h.grab();
    h.mouse_press("left");
    assert!(h.input.input.attack);
    h.input.grabbed = false;
    h.mouse_release("left");
    assert!(!h.input.input.attack);
}

#[test]
fn left_button_holds_attack_only_while_grabbed() {
    let mut owned = owned();
    let mut h = GameHarness::wrap(&mut owned.input, &mut owned.game);
    h.mouse_press("left");
    assert!(!h.input.input.attack);
    h.grab();
    h.mouse_press("left");
    assert!(h.input.input.attack);
    h.mouse_release("left");
    assert!(!h.input.input.attack);
}
