use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use winreg::enums::*;
use winreg::RegKey;

pub fn open_terminal(dir: &Path) -> Result<()> {
    open_cmd(dir)
}

pub fn open_cmd(dir: &Path) -> Result<()> {
    let dir = dir.to_path_buf();
    Command::new("cmd.exe")
        .arg("/K")
        .arg(format!("cd /d {}", dir.display()))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .spawn()
        .context("spawn cmd.exe")?;
    Ok(())
}

pub fn open_cmd_admin(dir: &Path) -> Result<()> {
    runas(
        "cmd.exe",
        &format!("/K cd /d \"{}\"", dir.display()),
        dir,
    )
}

pub fn open_powershell(dir: &Path) -> Result<()> {
    let dir = dir.to_path_buf();
    Command::new("powershell.exe")
        .args([
            "-NoExit",
            "-Command",
            &format!("Set-Location -LiteralPath '{}'", dir.display()),
        ])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .spawn()
        .context("spawn powershell.exe")?;
    Ok(())
}

pub fn open_powershell_admin(dir: &Path) -> Result<()> {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process powershell -Verb RunAs -ArgumentList '-NoExit','-Command','Set-Location -LiteralPath ''{}'''",
                dir.display().to_string().replace('\'', "''")
            ),
        ])
        .stdin(Stdio::null())
        .spawn()
        .context("elevate powershell")?;
    Ok(())
}

fn runas(exe: &str, args: &str, cwd: &Path) -> Result<()> {
    Command::new("powershell.exe")
        .args([
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -Verb RunAs -ArgumentList '{}' -WorkingDirectory '{}'",
                exe,
                args.replace('\'', "''"),
                cwd.display()
            ),
        ])
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("runas {exe}"))?;
    Ok(())
}

/// Register Open with CMD (+ Admin) under HKCU if missing, so Explorer and fuckFM agree.
pub fn ensure_cmd_context_menu_entries() -> Result<()> {
    ensure_verb(
        r"Software\Classes\Directory\shell\OpenCmdHereFuckFM",
        "Open with CMD",
        r#"cmd.exe /K cd /d "%V""#,
        false,
    )?;
    ensure_verb(
        r"Software\Classes\Directory\shell\OpenCmdHereAdminFuckFM",
        "Open with CMD (Admin)",
        r#"powershell.exe -Command "Start-Process cmd -Verb RunAs -ArgumentList '/K cd /d \"%V\"'"#,
        true,
    )?;
    ensure_verb(
        r"Software\Classes\Directory\Background\shell\OpenCmdHereFuckFM",
        "Open with CMD",
        r#"cmd.exe /K cd /d "%V""#,
        false,
    )?;
    ensure_verb(
        r"Software\Classes\Directory\Background\shell\OpenCmdHereAdminFuckFM",
        "Open with CMD (Admin)",
        r#"powershell.exe -Command "Start-Process cmd -Verb RunAs -ArgumentList '/K cd /d \"%V\"'"#,
        true,
    )?;
    Ok(())
}

fn ensure_verb(subkey: &str, title: &str, command: &str, has_lua_shield: bool) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // If an existing non-fuckFM CMD verb is present, still keep ours idempotent.
    if hkcu.open_subkey(subkey).is_ok() {
        return Ok(());
    }
    let (key, _) = hkcu.create_subkey(subkey)?;
    key.set_value("", &title)?;
    if has_lua_shield {
        key.set_value("HasLUAShield", &"")?;
    }
    let (cmd_key, _) = key.create_subkey("command")?;
    cmd_key.set_value("", &command)?;
    Ok(())
}

#[allow(dead_code)]
pub fn probe_shell() -> Result<()> {
    if which("cmd.exe") || Path::new(r"C:\Windows\System32\cmd.exe").exists() {
        Ok(())
    } else {
        bail!("cmd.exe not found")
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| p.join(bin).is_file())
        })
        .unwrap_or(false)
}