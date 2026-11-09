use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// An application that can open a given file (desktop id / ProgID / exe key).
#[derive(Clone, Debug)]
pub struct OpenWithApp {
    pub id: String,
    pub name: String,
}

/// Open a file or folder with the OS default handler for its type.
///
/// Linux: `xdg-open` (MIME / `.desktop` associations), with `gio open` fallback.
/// Windows: association lookup via PowerShell `Start-Process` (shell verb),
/// with `cmd /C start` fallback — honors ProgID / Store app handlers for the extension.
pub fn open_default(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    #[cfg(unix)]
    {
        open_unix(path)
    }
    #[cfg(windows)]
    {
        open_windows(path)
    }
}

/// Apps suitable for “Open with” on `path` (files only; empty for dirs / missing).
pub fn list_open_with(path: &Path) -> Vec<OpenWithApp> {
    if !path.is_file() {
        return Vec::new();
    }
    #[cfg(unix)]
    {
        list_open_with_unix(path)
    }
    #[cfg(windows)]
    {
        list_open_with_windows(path)
    }
}

/// Open `path` with a previously listed handler id.
pub fn open_with(path: &Path, app_id: &str) -> Result<()> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    #[cfg(unix)]
    {
        open_with_unix(path, app_id)
    }
    #[cfg(windows)]
    {
        open_with_windows(path, app_id)
    }
}

/// System “Open with” chooser dialog (Windows OpenAs; Linux best-effort).
pub fn open_with_dialog(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    #[cfg(unix)]
    {
        open_with_dialog_unix(path)
    }
    #[cfg(windows)]
    {
        open_with_dialog_windows(path)
    }
}

/// Set `app_id` as the default handler for `path`'s type.
///
/// Linux: `xdg-mime default <desktop> <mime>`.
/// Windows: HKCU ProgID for the extension; elevates via UAC on access denied.
pub fn set_default_app(path: &Path, app_id: &str) -> Result<()> {
    if !path.is_file() {
        bail!("set default requires a file");
    }
    #[cfg(unix)]
    {
        set_default_unix(path, app_id)
    }
    #[cfg(windows)]
    {
        set_default_windows(path, app_id)
    }
}

#[cfg(unix)]
fn open_unix(path: &Path) -> Result<()> {
    // Absolute paths avoid cwd-relative confusion in handlers / sandboxes.
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", path.display()))?;

    if which("xdg-open") && spawn_detached("xdg-open", &[path.as_os_str()]).is_ok() {
        return Ok(());
    }
    if which("gio") && spawn_detached("gio", &[OsStr::new("open"), path.as_os_str()]).is_ok() {
        return Ok(());
    }
    for bin in ["kde-open5", "kde-open", "exo-open"] {
        if which(bin) && spawn_detached(bin, &[path.as_os_str()]).is_ok() {
            return Ok(());
        }
    }
    bail!("no file opener found (install xdg-utils / xdg-open)")
}

#[cfg(windows)]
fn open_windows(path: &Path) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let path_str = path.to_string_lossy();
    // `\\?\` verbatim paths confuse some handlers; strip for Start-Process.
    let display = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);

    let ps = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!("Start-Process -LiteralPath '{}'", display.replace('\'', "''")),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Start-Process")?;
    if ps.success() {
        return Ok(());
    }

    // `start "" "path"` — empty title is required when the path is quoted.
    let status = Command::new("cmd.exe")
        .args(["/C", "start", "", display])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("cmd start")?;
    if status.success() {
        Ok(())
    } else {
        bail!("failed to open {display} with default app")
    }
}

fn spawn_detached(bin: &str, args: &[&OsStr]) -> Result<()> {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {bin}"))?;
    Ok(())
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(bin).is_file()))
        .unwrap_or(false)
}

#[cfg(unix)]
fn list_open_with_unix(path: &Path) -> Vec<OpenWithApp> {
    let mime = mime_type_unix(path).unwrap_or_else(|| "application/octet-stream".into());
    let mut ids = Vec::new();
    if which("gio") {
        if let Ok(out) = Command::new("gio")
            .args(["mime", &mime])
            .stdin(Stdio::null())
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let t = line.trim();
                if t.ends_with(".desktop") && !t.contains(' ') {
                    ids.push(t.to_string());
                } else if let Some(rest) = t.strip_prefix('\t') {
                    let id = rest.trim();
                    if id.ends_with(".desktop") {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    // Dedup, keep order; cap so the context menu stays usable.
    let mut seen = std::collections::HashSet::new();
    let mut apps = Vec::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let name = desktop_display_name(&id).unwrap_or_else(|| nice_desktop_id(&id));
        apps.push(OpenWithApp { id, name });
        if apps.len() >= 8 {
            break;
        }
    }
    apps
}

#[cfg(unix)]
fn mime_type_unix(path: &Path) -> Option<String> {
    if which("xdg-mime") {
        if let Ok(out) = Command::new("xdg-mime")
            .args(["query", "filetype"])
            .arg(path)
            .stdin(Stdio::null())
            .output()
        {
            if out.status.success() {
                let mime = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !mime.is_empty() {
                    return Some(mime);
                }
            }
        }
    }
    if which("gio") {
        if let Ok(out) = Command::new("gio")
            .args(["info", "-a", "standard::content-type"])
            .arg(path)
            .stdin(Stdio::null())
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(v) = line.split("standard::content-type:").nth(1) {
                    let mime = v.trim().to_string();
                    if !mime.is_empty() {
                        return Some(mime);
                    }
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn desktop_file_path(id: &str) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg).join("applications"));
    }
    if let Some(data) = std::env::var_os("XDG_DATA_DIRS") {
        for p in std::env::split_paths(&data) {
            dirs.push(p.join("applications"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/applications"));
        dirs.push(PathBuf::from("/usr/share/applications"));
    }
    for dir in dirs {
        let direct = dir.join(id);
        if direct.is_file() {
            return Some(direct);
        }
        // Flat + one-level vendor dirs (e.g. kde/).
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if p.is_dir() {
                    let nested = p.join(id);
                    if nested.is_file() {
                        return Some(nested);
                    }
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn desktop_display_name(id: &str) -> Option<String> {
    let path = desktop_file_path(id)?;
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut name_en = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Name=") {
            name = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Name[en]=")
            .or_else(|| line.strip_prefix("Name[en_US]="))
        {
            name_en = Some(v.trim().to_string());
        }
    }
    name_en.or(name)
}

#[cfg(unix)]
fn nice_desktop_id(id: &str) -> String {
    id.trim_end_matches(".desktop")
        .rsplit('.')
        .next()
        .unwrap_or(id)
        .replace('-', " ")
}

#[cfg(unix)]
fn open_with_unix(path: &Path, app_id: &str) -> Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", path.display()))?;
    if let Some(desktop) = desktop_file_path(app_id) {
        if which("gio") {
            spawn_detached(
                "gio",
                &[
                    OsStr::new("launch"),
                    desktop.as_os_str(),
                    path.as_os_str(),
                ],
            )?;
            return Ok(());
        }
        if which("gtk-launch") {
            // gtk-launch wants the desktop id, then files.
            spawn_detached(
                "gtk-launch",
                &[OsStr::new(app_id), path.as_os_str()],
            )?;
            return Ok(());
        }
        // Last resort: parse Exec= and substitute %f.
        if let Ok(exec) = desktop_exec_command(&desktop, &path) {
            Command::new("sh")
                .args(["-c", &exec])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("spawn desktop Exec")?;
            return Ok(());
        }
    }
    bail!("could not launch {app_id}")
}

#[cfg(unix)]
fn desktop_exec_command(desktop: &Path, file: &Path) -> Result<String> {
    let text = std::fs::read_to_string(desktop)?;
    let mut exec = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Exec=") {
            exec = Some(v.trim().to_string());
            break;
        }
    }
    let exec = exec.ok_or_else(|| anyhow::anyhow!("no Exec= in desktop file"))?;
    let file_s = file.to_string_lossy();
    let quoted = shell_quote(&file_s);
    let mut cmd = exec
        .replace("%F", &quoted)
        .replace("%f", &quoted)
        .replace("%U", &quoted)
        .replace("%u", &quoted);
    // Drop remaining field codes.
    for code in ["%i", "%c", "%k", "%d", "%D", "%n", "%N", "%v", "%m"] {
        cmd = cmd.replace(code, "");
    }
    Ok(cmd)
}

#[cfg(unix)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[cfg(unix)]
fn open_with_dialog_unix(path: &Path) -> Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", path.display()))?;
    // Portal / DE choosers when available; otherwise fall back to default open.
    if which("gdbus") {
        // xdg-desktop-portal OpenFile? Not standard for Open-With.
        // Try KDE's open-with dialog:
        if which("kde-open5")
            && spawn_detached(
                "kde-open5",
                &[OsStr::new("--choose"), path.as_os_str()],
            )
            .is_ok()
        {
            return Ok(());
        }
    }
    // xfce helper
    if which("exo-open")
        && spawn_detached(
            "exo-open",
            &[OsStr::new("--launch"), OsStr::new("FileManager"), path.as_os_str()],
        )
        .is_err()
    {
        // ignore — exo-open --launch FileManager is wrong for files
    }
    // Last resort: if we have any handlers, open with the first; else xdg-open.
    let apps = list_open_with_unix(&path);
    if let Some(app) = apps.first() {
        return open_with_unix(&path, &app.id);
    }
    open_unix(&path)
}

#[cfg(windows)]
fn list_open_with_windows(path: &Path) -> Vec<OpenWithApp> {
    use winreg::enums::*;
    use winreg::RegKey;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()));
    let Some(ext) = ext else {
        return Vec::new();
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let candidates = [
        (
            &hkcu,
            format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\OpenWithList"
            ),
        ),
        (&hkcu, format!(r"Software\Classes\{ext}\OpenWithList")),
        (&hkcr, format!(r"{ext}\OpenWithList")),
    ];

    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (root, cand) in candidates {
        let Ok(key) = root.open_subkey(&cand) else {
            continue;
        };
        for name in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            let Ok(val) = key.get_value::<String, _>(name) else {
                continue;
            };
            if val.is_empty() || !seen.insert(val.to_ascii_lowercase()) {
                continue;
            }
            let label = val.trim_end_matches(".exe").to_string();
            apps.push(OpenWithApp { id: val, name: label });
            if apps.len() >= 8 {
                return apps;
            }
        }
    }
    apps
}

#[cfg(windows)]
fn open_with_windows(path: &Path, app_id: &str) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let path_str = path.to_string_lossy();
    let display = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    // app_id is typically an exe name from OpenWithList (e.g. notepad.exe).
    let status = Command::new("cmd.exe")
        .args(["/C", "start", "", app_id, display])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("open with app")?;
    if status.success() {
        Ok(())
    } else {
        bail!("failed to open with {app_id}")
    }
}

#[cfg(windows)]
fn open_with_dialog_windows(path: &Path) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let path_str = path.to_string_lossy();
    let display = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    Command::new("rundll32.exe")
        .args(["shell32.dll,OpenAs_RunDLL", display])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("OpenAs_RunDLL")?;
    Ok(())
}

#[cfg(unix)]
fn set_default_unix(path: &Path, app_id: &str) -> Result<()> {
    let mime = mime_type_unix(path).unwrap_or_else(|| "application/octet-stream".into());
    let desktop = if app_id.ends_with(".desktop") {
        app_id.to_string()
    } else {
        format!("{app_id}.desktop")
    };
    if !which("xdg-mime") {
        bail!("xdg-mime not found (install xdg-utils)");
    }
    let status = Command::new("xdg-mime")
        .args(["default", &desktop, &mime])
        .stdin(Stdio::null())
        .status()
        .context("xdg-mime default")?;
    if status.success() {
        Ok(())
    } else {
        bail!("xdg-mime default failed for {mime} → {desktop}")
    }
}

#[cfg(windows)]
fn set_default_windows(path: &Path, app_id: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .ok_or_else(|| anyhow::anyhow!("file has no extension"))?;

    let exe = resolve_windows_app_path(app_id)
        .unwrap_or_else(|| PathBuf::from(app_id));
    let exe_s = exe.to_string_lossy();
    let exe_s = exe_s.strip_prefix(r"\\?\").unwrap_or(&exe_s);
    let prog_id = format!(
        "fuckfm.{}",
        ext.trim_start_matches('.').to_ascii_lowercase()
    );
    let cmd = format!("\"{exe_s}\" \"%1\"");

    match write_user_assoc(&ext, &prog_id, &cmd) {
        Ok(()) => Ok(()),
        Err(err) => {
            let io = err
                .downcast_ref::<std::io::Error>()
                .cloned()
                .or_else(|| {
                    err.chain()
                        .find_map(|e| e.downcast_ref::<std::io::Error>().cloned())
                });
            if io.as_ref().is_some_and(crate::platform::elevate::is_access_denied) {
                let script = format!(
                    "$ext='{ext}'; $progId='{prog_id}'; $cmd='{cmd}'; \
                     New-Item -Path \"HKCU:\\Software\\Classes\\$ext\" -Force | Out-Null; \
                     Set-ItemProperty -Path \"HKCU:\\Software\\Classes\\$ext\" -Name '(default)' -Value $progId; \
                     New-Item -Path \"HKCU:\\Software\\Classes\\$progId\\shell\\open\\command\" -Force | Out-Null; \
                     Set-ItemProperty -Path \"HKCU:\\Software\\Classes\\$progId\\shell\\open\\command\" -Name '(default)' -Value $cmd"
                );
                crate::platform::elevate::run_elevated_powershell(&script)
            } else {
                Err(err)
            }
        }
    }
}

#[cfg(windows)]
fn write_user_assoc(ext: &str, prog_id: &str, cmd: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (ext_key, _) = hkcu
        .create_subkey(format!(r"Software\Classes\{ext}"))
        .with_context(|| format!("create Classes\\{ext}"))?;
    ext_key.set_value("", &prog_id)?;
    let (cmd_key, _) = hkcu
        .create_subkey(format!(
            r"Software\Classes\{prog_id}\shell\open\command"
        ))
        .with_context(|| format!("create Classes\\{prog_id}"))?;
    cmd_key.set_value("", &cmd)?;
    // Soft hint for Explorer OpenWithList.
    let _ = hkcu.create_subkey(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\OpenWithList"
    ));
    Ok(())
}

#[cfg(windows)]
fn resolve_windows_app_path(app_id: &str) -> Option<PathBuf> {
    let p = PathBuf::from(app_id);
    if p.is_file() {
        return Some(p);
    }
    if let Ok(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(app_id);
            if cand.is_file() {
                return Some(cand);
            }
            if !app_id.ends_with(".exe") {
                let cand = dir.join(format!("{app_id}.exe"));
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    let system = PathBuf::from(r"C:\Windows\System32").join(app_id);
    if system.is_file() {
        return Some(system);
    }
    None
}