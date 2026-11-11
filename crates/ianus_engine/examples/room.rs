//! Opens a room file in the editor window.
//!
//! ```text
//! cargo run -p ianus_objects --example write_cellar     # once, makes rooms/cellar.room
//! cargo run -p ianus_engine --example room              # opens it
//! cargo run -p ianus_engine --example room -- my.room   # or your own
//! ```
//!
//! Press F5 to play, F5 again to stop and put everything back. Edit and save
//! the room file in any text editor and it reloads by itself (while editing;
//! while playing it waits for Stop).

use bevy::prelude::AppExit;
use ianus_engine::prelude::*;

fn main() -> AppExit {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rooms/cellar.room".into());
    let mut engine = Engine::new("Ianus").room_file(path);

    // Hooks for automated checks.
    if let Some(n) = std::env::var("IANUS_EXIT_AFTER_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        engine = engine.exit_after_frames(n);
    }
    if let Ok(shot) = std::env::var("IANUS_SCREENSHOT") {
        let at = std::env::var("IANUS_SCREENSHOT_FRAME")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120);
        engine = engine.screenshot_at_frame(at, shot);
    }
    if std::env::var("IANUS_PLAY").is_ok() {
        engine = engine.play_on_start(true);
    }
    engine.run()
}
