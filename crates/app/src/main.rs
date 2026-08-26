//! oxcraft entry point: the interactive window by default, or a headless
//! mode (screenshot self-tests, socket test server) that needs no display.

mod audio;
mod headless;
mod menu_input;
mod screens;
mod session;
mod state;
mod stream;
mod view;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |index: usize, default: &str| {
        args.get(index)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    };
    match args.get(1).map(String::as_str) {
        Some("--screenshot") => headless::screenshot(&arg(2, "/tmp/oxcraft-menu.png"), false),
        Some("--screenshot-playing") => {
            headless::screenshot(&arg(2, "/tmp/oxcraft-playing.png"), true)
        }
        Some("--test-server") => headless::serve(&arg(2, "/tmp/oxcraft-e2e.sock")),
        _ => state::run(),
    }
}
