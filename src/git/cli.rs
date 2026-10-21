//! Runs the `git` executable for every operation that writes, so user hooks,
//! commit signing, credential helpers and SSH config behave as in a terminal.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

static GIT: OnceLock<String> = OnceLock::new();

fn git() -> &'static str {
    GIT.get_or_init(|| {
        ["/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .into_iter()
            .find(|p| Path::new(p).exists())
            .unwrap_or("git")
            .to_owned()
    })
}

/// Apps started from the Finder get a minimal `PATH`. Borrow the user's
/// shell `PATH` so hooks (husky, lint-staged, …) find node, python, etc.
/// Must run before any thread is spawned.
pub fn init_env() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let Ok(mut child) = Command::new(shell)
        .args(["-i", "-l", "-c", "printf '\\n__FORKED_PATH__%s' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    // An interactive shell may hang on a misbehaving rc file: give it 3s.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                return;
            }
        }
    }
    let Ok(out) = child.wait_with_output() else {
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some((_, path)) = text.rsplit_once("__FORKED_PATH__") {
        let path = path.trim();
        if !path.is_empty() {
            // SAFETY: called from `main` before any other thread exists.
            unsafe { std::env::set_var("PATH", path) };
        }
    }
}

/// Runs `git <args>` in `cwd`. Returns stdout, or an error carrying git's message.
pub fn run(cwd: &Path, args: &[String], stdin: Option<&str>) -> Result<String> {
    let mut child = Command::new(git())
        .current_dir(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .env("LC_ALL", "C")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "impossible de lancer git")?;
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().expect("stdin is piped");
        pipe.write_all(input.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).trim_end().to_owned();
    if out.status.success() {
        Ok(stdout)
    } else if !stderr.is_empty() {
        bail!(stderr)
    } else if !stdout.is_empty() {
        bail!(stdout)
    } else {
        bail!("git {} a échoué ({})", args.join(" "), out.status)
    }
}
