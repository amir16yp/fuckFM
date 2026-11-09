//! Nested Xephyr helper with Glamor (GPU) for safe WM development.
//!
//! ```text
//! cargo run --bin fuckwm -- --xephyr
//! ```

use anyhow::{bail, Context, Result};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

/// If `--xephyr` / `FUCKWM_XEPHYR=1`, spawn Xephyr with `-glamor` and set `DISPLAY`.
/// Returns the child handle (caller should kill on exit).
pub fn maybe_spawn() -> Result<Option<Child>> {
    let want = std::env::args().any(|a| a == "--xephyr")
        || std::env::var("FUCKWM_XEPHYR")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
    if !want {
        return Ok(None);
    }
    if which("Xephyr").is_none() {
        bail!("Xephyr not found (Arch: extra/xorg-server-xephyr)");
    }

    let display_num = free_display_num().unwrap_or(21);
    let display = format!(":{display_num}");
    let w = std::env::var("FUCKWM_XEPHYR_W").unwrap_or_else(|_| "1280".into());
    let h = std::env::var("FUCKWM_XEPHYR_H").unwrap_or_else(|_| "800".into());

    // Prefer Glamor for HW accel inside the nested server; fall back without it.
    let mut child = spawn_xephyr(&display, &w, &h, true).or_else(|err| {
        eprintln!("fuckWM: Xephyr -glamor failed ({err}); retrying without Glamor");
        spawn_xephyr(&display, &w, &h, false)
    })?;

    // Wait until the display accepts connections.
    for _ in 0..50 {
        if x_ready(&display) {
            std::env::set_var("DISPLAY", &display);
            eprintln!("fuckWM: nested Xephyr on {display} ({w}x{h}, Glamor when available)");
            return Ok(Some(child));
        }
        if let Ok(Some(status)) = child.try_wait() {
            bail!("Xephyr exited early: {status}");
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    bail!("Xephyr did not become ready on {display}")
}

fn spawn_xephyr(display: &str, w: &str, h: &str, glamor: bool) -> Result<Child> {
    let mut cmd = Command::new("Xephyr");
    cmd.arg(display)
        .arg("-screen")
        .arg(format!("{w}x{h}"))
        .arg("-ac")
        .arg("-br");
    // Do NOT pass -terminate: our readiness probe would kill the nested server.
    if glamor {
        cmd.arg("-glamor");
    }
    cmd.arg("-resizeable");
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn Xephyr {display}"))?;
    Ok(child)
}

fn x_ready(display: &str) -> bool {
    // Prefer socket presence (no xdpyinfo dependency), then a real xcb connect.
    if let Some(num) = display.strip_prefix(':') {
        let sock = format!("/tmp/.X11-unix/X{num}");
        if !std::path::Path::new(&sock).exists() {
            return false;
        }
    }
    let Ok(c_display) = std::ffi::CString::new(display) else {
        return false;
    };
    match x11rb::xcb_ffi::XCBConnection::connect(Some(c_display.as_c_str())) {
        Ok((_conn, _)) => true,
        Err(_) => false,
    }
}

fn free_display_num() -> Option<u32> {
    for n in 21..64 {
        let lock = format!("/tmp/.X{n}-lock");
        if !std::path::Path::new(&lock).exists() {
            return Some(n);
        }
    }
    None
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| p.is_file())
    })
}
