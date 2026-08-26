//! Headless modes on an offscreen renderer: screenshot self-tests and the
//! socket test server. Neither opens a window or an event loop, so they run
//! wherever a Vulkan adapter exists (`WGPU_ADAPTER_NAME=llvmpipe` selects
//! Mesa's software driver).

use ox_app::game::{Action, Breaking};
use ox_app::harness::{Applied, FIXED_DT};
use ox_app::test_server::{Command, TestServer, reply_error, reply_ok};
use ox_render::Renderer;
use serde_json::json;

use crate::session::Session;
use crate::view::VIEW;

fn session() -> Result<Session, String> {
    let (width, height) = VIEW.window_size;
    let renderer = pollster::block_on(Renderer::headless(width, height))?;
    Ok(Session::new(renderer))
}

/// Renders the start menu, or a staged playing scene, to `path` as PNG.
pub(crate) fn screenshot(path: &str, playing: bool) -> Result<(), String> {
    let mut session = session()?;
    session.game.player.yaw = 2.55;
    session.game.player.pitch = -0.14;
    if playing {
        session.enter_world();
        session.game.player.flying = true;
        session.game.player.pos[1] += 2.0;
        session.game.player.pitch = -0.85;
        session.game.handle(&Action::Select(2));
        session.game.handle(&Action::Place);
    }
    for _ in 0..30 {
        session.game.update(FIXED_DT, session.input.input);
    }
    if playing && let Some(hit) = session.game.target() {
        session.game.breaking = Some(Breaking {
            cell: [hit.x, hit.y, hit.z],
            progress: 0.55,
        });
        session
            .particles
            .spawn_break([hit.x, hit.y + 1, hit.z], hit.block);
        for _ in 0..12 {
            session.particles.step(FIXED_DT, &session.game.world);
        }
    }
    session.sync_gpu(usize::MAX, usize::MAX);
    if playing {
        session.show_item_name();
    }
    session.capture(path)
}

enum Outcome {
    Reply(String),
    Quit(String),
}

fn exec(session: &mut Session, command: &Command) -> Outcome {
    match session.harness().apply(command) {
        Applied::Ticked { ticks, result } => {
            session.present(ticks);
            session.sync_gpu(VIEW.data_budget, VIEW.mesh_budget);
            Outcome::Reply(reply_ok(result))
        }
        Applied::Reply(mut result) => {
            if matches!(command, Command::State) && !enrich_state(session, &mut result) {
                Outcome::Reply(reply_error("state result is not an object"))
            } else {
                Outcome::Reply(reply_ok(result))
            }
        }
        Applied::Shell => shell_outcome(session, command),
    }
}

/// Replies for the verbs the harness leaves to the shell.
fn shell_outcome(session: &mut Session, command: &Command) -> Outcome {
    match command {
        Command::Screenshot { path } => match session.capture(path) {
            Ok(()) => Outcome::Reply(reply_ok(json!({ "saved": path }))),
            Err(e) => Outcome::Reply(reply_error(&e)),
        },
        Command::Quit => Outcome::Quit(reply_ok(json!({ "bye": true }))),
        _ => Outcome::Reply(reply_error("verb reached neither dispatch")),
    }
}

/// Adds the session-owned `seed` and `meshed_chunks` fields to a `state`
/// result; false when the shape is unexpected.
fn enrich_state(session: &Session, result: &mut serde_json::Value) -> bool {
    let Some(fields) = result.as_object_mut() else {
        return false;
    };
    fields.insert("seed".into(), json!(session.game.seed()));
    fields.insert("meshed_chunks".into(), json!(session.meshed_count()));
    true
}

/// Serves socket commands until `quit`. Each command runs to completion
/// before the next one is read, so a script replays deterministically.
pub(crate) fn serve(socket_path: &str) -> Result<(), String> {
    let mut session = session()?;
    session.enter_world();
    let server = TestServer::spawn(socket_path).map_err(|e| format!("bind {socket_path}: {e}"))?;
    while let Ok(request) = server.requests.recv() {
        let (reply, quit) = match exec(&mut session, &request.command) {
            Outcome::Reply(reply) => (reply, false),
            Outcome::Quit(reply) => (reply, true),
        };
        let _ = request.respond.send(reply);
        if quit {
            break;
        }
    }
    Ok(())
}
