//! Developer aid: `FORKED_SCREENSHOT=/tmp/shot.bmp forked <repo>` renders a
//! few frames, saves the window as a BMP and quits. Optional
//! `FORKED_VIEW=history|changes|checkout:<branch>` picks the view and selects
//! its first item, or opens the checkout dialog.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{ColorImage, Context, ScreenshotCallback, ViewportCommand};

pub struct DebugShot {
    path: PathBuf,
    pub view: Option<String>,
    frames: u32,
    requested: bool,
}

impl DebugShot {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            path: std::env::var_os("FORKED_SCREENSHOT")?.into(),
            view: std::env::var("FORKED_VIEW").ok(),
            frames: 0,
            requested: false,
        })
    }

    /// Returns true on the first frame, so the caller can set up the demo state.
    pub fn tick(&mut self, ctx: &Context) -> bool {
        self.frames += 1;
        ctx.request_repaint();
        if self.frames == 15 && !self.requested {
            self.requested = true;
            let path = self.path.clone();
            let ctx2 = ctx.clone();
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(ScreenshotCallback::new(
                move |image: Arc<ColorImage>| {
                    let _ = std::fs::write(&path, bmp(&image));
                    ctx2.send_viewport_cmd(ViewportCommand::Close);
                    ctx2.request_repaint();
                },
            )));
        }
        self.frames == 1
    }
}

fn bmp(image: &ColorImage) -> Vec<u8> {
    let [w, h] = image.size;
    let data_len = w * h * 4;
    let mut out = Vec::with_capacity(54 + data_len);
    out.extend(b"BM");
    out.extend(((54 + data_len) as u32).to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(54u32.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend((w as i32).to_le_bytes());
    out.extend((-(h as i32)).to_le_bytes()); // top-down rows
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend([0u8; 24]);
    for p in &image.pixels {
        out.extend([p.b(), p.g(), p.r(), 255]);
    }
    out
}
