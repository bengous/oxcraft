//! Unix-socket JSON server letting external tools drive the running binary.

use std::io::Write;

use serde::Deserialize;
use serde_json::json;

/// One client command, parsed from newline-delimited JSON over the socket.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Tap a named key down and up.
    Press {
        /// Logical key name, e.g. `"W"`.
        key: String,
    },
    /// Press a named key and keep it held.
    Hold {
        /// Logical key name, e.g. `"Space"`.
        key: String,
    },
    /// Release a held named key.
    Release {
        /// Logical key name, e.g. `"W"`.
        key: String,
    },
    /// Press a named mouse button; `"left"` holds the attack until
    /// [`Command::MouseRelease`].
    Mouse {
        /// Button name: `"left"`, `"right"`, or `"middle"`.
        button: String,
    },
    /// Release a named mouse button.
    MouseRelease {
        /// Button name: `"left"`, `"right"`, or `"middle"`.
        button: String,
    },
    /// Set the look angles directly.
    Look {
        /// Yaw in radians.
        yaw: f32,
        /// Pitch in radians.
        pitch: f32,
    },
    /// Move the player to a position at rest.
    Teleport {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
        /// Z coordinate.
        z: f32,
    },
    /// Advance the simulation by a number of fixed timesteps.
    Tick {
        /// Tick count.
        n: usize,
    },
    /// Advance the simulation by roughly a duration of simulated time.
    Run {
        /// Duration in seconds.
        seconds: f32,
    },
    /// Report the currently targeted cell.
    Target,
    /// Report full player state.
    State,
    /// Report one world cell.
    Block {
        /// X coordinate.
        x: i32,
        /// Y coordinate.
        y: i32,
        /// Z coordinate.
        z: i32,
    },
    /// Report the FNV-1a digest of player state plus the given world cells,
    /// computed server-side so JSON float printing never touches the bits.
    Digest {
        /// World cells absorbed in order, as `[x, y, z]`.
        cells: Vec<[i32; 3]>,
    },
    /// Save a screenshot to a PNG path.
    Screenshot {
        /// Destination path.
        path: String,
    },
    /// Reply once, then exit the process.
    Quit,
}

/// Channel used to send one reply string back to the connection thread.
pub type Responder = std::sync::mpsc::Sender<String>;

/// One parsed command paired with its reply channel.
pub struct Request {
    /// Command to execute.
    pub command: Command,
    /// Channel receiving the JSON reply line.
    pub respond: Responder,
}

/// Accepts socket connections and forwards parsed requests to the run loop.
pub struct TestServer {
    /// Receiver yielding one request per client command.
    pub requests: std::sync::mpsc::Receiver<Request>,
}

impl TestServer {
    /// Binds a Unix socket at `path` and serves connections on a background
    /// thread.
    ///
    /// # Errors
    ///
    /// Returns the bind error, for example when `path` exceeds the Unix
    /// socket path limit (107 bytes on Linux).
    pub fn spawn(path: &str) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(path);
        let listener = std::os::unix::net::UnixListener::bind(path)?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if handle_connection(stream, &tx).is_err() {
                    return;
                }
            }
        });
        Ok(Self { requests: rx })
    }
}

/// Serves one client connection; newline-delimited JSON commands in, JSON replies out.
fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    tx: &std::sync::mpsc::Sender<Request>,
) -> Result<(), ()> {
    let Ok(reader_clone) = stream.try_clone() else {
        return Err(());
    };
    let reader = std::io::BufReader::new(reader_clone);
    let mut writer = std::io::LineWriter::new(stream);
    for line in std::io::BufRead::lines(reader) {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let command = match serde_json::from_str::<Command>(&line) {
            Ok(cmd) => cmd,
            Err(e) => {
                let reply = json!({"error": format!("bad command: {e}")});
                let _ = writeln!(writer, "{reply}");
                continue;
            }
        };
        let (rtx, rrx) = std::sync::mpsc::channel();
        if tx
            .send(Request {
                command,
                respond: rtx,
            })
            .is_err()
        {
            return Err(());
        }
        match rrx.recv() {
            Ok(reply) => {
                let _ = writeln!(writer, "{reply}");
            }
            Err(_) => return Err(()),
        }
    }
    Ok(())
}

/// Builds a successful JSON reply wrapping `extra` under `"result"`.
pub fn reply_ok<T: serde::Serialize>(extra: T) -> String {
    json!({"ok": true, "result": extra}).to_string()
}

/// Builds a failure JSON reply carrying `message`.
pub fn reply_error(message: &str) -> String {
    json!({"ok": false, "error": message}).to_string()
}
