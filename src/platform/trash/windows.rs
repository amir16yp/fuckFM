use crate::browser::{DirEntry, EntryKind};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Send a file or folder to the Windows Recycle Bin via VB FileIO.
pub fn trash_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("{} does not exist", path.display());
    }
    let abs = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let quoted = abs.to_string_lossy().replace('\'', "''");
    let method = if abs.is_dir() {
        "DeleteDirectory"
    } else {
        "DeleteFile"
    };
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; \
         [Microsoft.VisualBasic.FileIO.FileSystem]::{method}(\
            '{quoted}', \
            [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs, \
            [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin, \
            [Microsoft.VisualBasic.FileIO.UICancelOption]::ThrowException)"
    );
    run_ps(&script).with_context(|| format!("send to Recycle Bin: {}", path.display()))
}

pub fn restore_from_trash(path: &Path) -> Result<()> {
    let quoted = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject Shell.Application
$rb = $shell.NameSpace(0x0a)
if (-not $rb) {{ throw 'Recycle Bin unavailable' }}
$target = '{quoted}'
$item = $null
foreach ($i in $rb.Items()) {{
  if ($i.Path -eq $target) {{ $item = $i; break }}
}}
if (-not $item) {{ throw "Item not found in Recycle Bin: $target" }}
$verbs = @($item.Verbs() | ForEach-Object {{ $_.Name }})
$restore = $item.Verbs() | Where-Object {{
  ($_.Name -replace '&','') -match '^(Restore|Wiederherstellen|Restaurer|Restablecer|Ripristina)'
}} | Select-Object -First 1
if (-not $restore) {{
  # Fallback: first verb that looks like restore in any locale (contains "restor")
  $restore = $item.Verbs() | Where-Object {{
    ($_.Name -replace '&','').ToLower().Contains('restor')
  }} | Select-Object -First 1
}}
if (-not $restore) {{ throw ("No restore verb; available: " + ($verbs -join ', ')) }}
$restore.DoIt()
"#
    );
    run_ps(&script).with_context(|| format!("restore from Recycle Bin: {}", path.display()))
}

pub fn delete_permanently(path: &Path) -> Result<()> {
    let quoted = path.to_string_lossy().replace('\'', "''");
    // Prefer Shell verb when the item is in the Recycle Bin; otherwise hard delete.
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$target = '{quoted}'
$shell = New-Object -ComObject Shell.Application
$rb = $shell.NameSpace(0x0a)
$item = $null
if ($rb) {{
  foreach ($i in $rb.Items()) {{
    if ($i.Path -eq $target) {{ $item = $i; break }}
  }}
}}
if ($item) {{
  $del = $item.Verbs() | Where-Object {{
    $n = ($_.Name -replace '&','')
    $n -match '^(Delete|Löschen|Supprimer|Eliminar|Elimina)' -or $n.ToLower().Contains('delet')
  }} | Select-Object -First 1
  if ($del) {{ $del.DoIt(); exit 0 }}
}}
if (Test-Path -LiteralPath $target) {{
  Remove-Item -LiteralPath $target -Recurse -Force
}} else {{
  throw "Path not found: $target"
}}
"#
    );
    run_ps(&script).with_context(|| format!("permanent delete: {}", path.display()))
}

pub fn empty_trash() -> Result<()> {
    let script = r#"
$ErrorActionPreference = 'Continue'
# Win10+: Clear-RecycleBin. Fall back to Shell delete-all if missing.
if (Get-Command Clear-RecycleBin -ErrorAction SilentlyContinue) {
  Clear-RecycleBin -Force -ErrorAction SilentlyContinue
} else {
  $shell = New-Object -ComObject Shell.Application
  $rb = $shell.NameSpace(0x0a)
  if ($rb) {
    foreach ($item in @($rb.Items())) {
      $del = $item.Verbs() | Where-Object {
        $n = ($_.Name -replace '&','')
        $n -match '^(Delete|Löschen|Supprimer|Eliminar|Elimina)' -or $n.ToLower().Contains('delet')
      } | Select-Object -First 1
      if ($del) { $del.DoIt() }
    }
  }
}
"#;
    run_ps(script).context("empty Recycle Bin")
}

/// List Recycle Bin items via Shell.Application namespace 0x0a.
pub fn list_trash() -> Result<Vec<DirEntry>> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject Shell.Application
$rb = $shell.NameSpace(0x0a)
if (-not $rb) { exit 0 }
foreach ($item in $rb.Items()) {
    $name = $item.Name
    $path = $item.Path
    if ([string]::IsNullOrWhiteSpace($path)) { continue }
    $isDir = if ($item.IsFolder) { '1' } else { '0' }
    $size = 0
    try { $size = [int64]$item.Size } catch { $size = 0 }
    Write-Output ("{0}`t{1}`t{2}`t{3}" -f $name, $path, $isDir, $size)
}
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::null())
        .output()
        .context("list Recycle Bin")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            return Ok(Vec::new());
        }
        bail!("list Recycle Bin failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, '\t');
        let Some(name) = parts.next() else { continue };
        let Some(path) = parts.next() else { continue };
        let is_dir = parts.next().map(|s| s == "1").unwrap_or(false);
        let size = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| !is_dir);
        entries.push(DirEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            is_dir,
            kind: if is_dir {
                EntryKind::Folder
            } else {
                EntryKind::File
            },
            size,
            modified: None,
        });
    }
    Ok(entries)
}

fn run_ps(script: &str) -> Result<()> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::null())
        .output()
        .context("run powershell")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
    Ok(())
}
