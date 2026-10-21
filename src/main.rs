// No console window behind the app on Windows (release builds only, so
// `cargo run` still shows panics and logs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod debug_shot;
mod git;
mod platform;
mod state;
mod tab;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

fn main() -> eframe::Result {
    git::cli::init_env();

    // `forked /path/to/repo`, `open -a Forked --args /path/to/repo` or
    // `Forked.exe C:\path\to\repo`.
    let open_arg = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Forked")
        .with_app_id("forked")
        .with_inner_size([1380.0, 880.0])
        .with_min_inner_size([960.0, 600.0]);
    if platform::TABS_IN_TITLEBAR {
        // Tabs live in the title bar, as in Fork. An empty icon keeps the
        // .app bundle icon instead of egui's default.
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_icon(Arc::new(egui::IconData::default()));
    } else if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/forked.png"))
    {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Forked",
        options,
        Box::new(move |cc| Ok(Box::new(app::ForkedApp::new(cc, open_arg)))),
    )
}
