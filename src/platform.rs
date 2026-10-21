//! Everything that differs between macOS, Windows and Linux.

use std::path::Path;
use std::process::Command;

/// Short name of the system file manager, for the toolbar.
pub const FILE_MANAGER: &str = if cfg!(target_os = "macos") {
    "Finder"
} else if cfg!(windows) {
    "Explorateur"
} else {
    "Fichiers"
};

/// Menu entry that reveals a file in the file manager.
pub const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Afficher dans le Finder"
} else if cfg!(windows) {
    "Afficher dans l'Explorateur"
} else {
    "Afficher dans le gestionnaire de fichiers"
};

/// `⌘R` on macOS, `Ctrl+R` elsewhere (egui maps COMMAND to Ctrl there).
pub fn shortcut(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{}", if key == "↩" { "Entrée" } else { key })
    }
}

/// Window chrome: on macOS the tabs are drawn in a transparent title bar.
pub const TABS_IN_TITLEBAR: bool = cfg!(target_os = "macos");

/// Opens a file or folder with its default application.
pub fn open(path: &Path) {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        // explorer opens folders, and files with their default application.
        let mut c = Command::new("explorer");
        c.arg(windows_path(path));
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    no_console(&mut cmd);
    let _ = cmd.spawn();
}

/// Shows `path` selected in the file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(windows)]
    {
        // explorer wants `/select,<path>` as a single argument.
        let _ = Command::new("explorer")
            .arg(format!("/select,{}", windows_path(path)))
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        open(path.parent().unwrap_or(path));
    }
}

/// Opens a terminal in `dir`.
pub fn open_terminal(dir: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").args(["-a", "Terminal"]).arg(dir).spawn();
    }
    #[cfg(windows)]
    {
        // Windows Terminal if installed, else a classic console.
        let wt = Command::new("wt").arg("-d").arg(dir).spawn();
        if wt.is_err() {
            let _ = Command::new("cmd")
                .args(["/C", "start", "", "cmd"])
                .current_dir(dir)
                .spawn();
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = Command::new("x-terminal-emulator").current_dir(dir).spawn();
    }
}

/// Git reports paths with `/`; Windows tools such as explorer need `\\`.
#[cfg(windows)]
fn windows_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

/// Keeps child processes from flashing a console window on Windows.
pub fn no_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Where to look for the `git` executable, in order of preference.
pub fn git_candidates() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/git", "/usr/local/bin/git"]
    } else if cfg!(windows) {
        &[
            r"C:\Program Files\Git\cmd\git.exe",
            r"C:\Program Files (x86)\Git\cmd\git.exe",
        ]
    } else {
        &[]
    }
}

/// System UI and monospace fonts, tried in order.
pub fn font_candidates() -> (&'static [&'static str], &'static [&'static str]) {
    if cfg!(target_os = "macos") {
        (
            &["/System/Library/Fonts/SFNS.ttf"],
            &["/System/Library/Fonts/SFNSMono.ttf"],
        )
    } else if cfg!(windows) {
        (
            &[r"C:\Windows\Fonts\segoeui.ttf"],
            &[
                r"C:\Windows\Fonts\CascadiaMono.ttf",
                r"C:\Windows\Fonts\consola.ttf",
            ],
        )
    } else {
        (
            &[
                "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            ],
            &[
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_uses_the_platform_modifier() {
        if cfg!(target_os = "macos") {
            assert_eq!(shortcut("R"), "⌘R");
            assert_eq!(shortcut("↩"), "⌘↩");
        } else {
            assert_eq!(shortcut("R"), "Ctrl+R");
            assert_eq!(shortcut("↩"), "Ctrl+Entrée");
        }
    }
}
