mod app;
mod debug_shot;
mod git;
mod state;
mod tab;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

fn main() -> eframe::Result {
    git::cli::init_env();

    // `forked /path/to/repo` or `open -a Forked --args /path/to/repo`.
    let open_arg = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Forked")
            .with_app_id("forked")
            .with_inner_size([1380.0, 880.0])
            .with_min_inner_size([960.0, 600.0])
            // Tabs live in the title bar, as in Fork.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            // An empty icon keeps the .app bundle icon instead of egui's default.
            .with_icon(Arc::new(egui::IconData::default())),
        ..Default::default()
    };
    eframe::run_native(
        "Forked",
        options,
        Box::new(move |cc| Ok(Box::new(app::ForkedApp::new(cc, open_arg)))),
    )
}
