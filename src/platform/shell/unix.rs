use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn open_terminal(dir: &Path) -> Result<()> {
    let dir = dir
        .canonicalize()
        .with_context(|| format!("canonicalize {}", dir.display()))?;

    if let Ok(term) = std::env::var("TERMINAL") {
        if !term.is_empty() && spawn_simple(&term, &dir)? {
            return Ok(());
        }
    }

    // Preferred modern helpers first, then common DE terminals.
    let candidates: &[(&str, &[&str])] = &[
        ("xdg-terminal-exec", &[]),
        ("exo-open", &["--working-directory", "__DIR__", "--launch", "TerminalEmulator"]),
        ("kgx", &["--working-directory", "__DIR__"]),
        ("gnome-terminal", &["--working-directory", "__DIR__"]),
        ("konsole", &["--workdir", "__DIR__"]),
        ("xfce4-terminal", &["--working-directory", "__DIR__"]),
        ("mate-terminal", &["--working-directory", "__DIR__"]),
        ("tilix", &["--working-directory", "__DIR__"]),
        ("alacritty", &["--working-directory", "__DIR__"]),
        ("kitty", &["--directory", "__DIR__"]),
        ("wezterm", &["start", "--cwd", "__DIR__"]),
        ("xterm", &["-e", "bash"]),
    ];

    for (bin, args) in candidates {
        if which(bin) {
            let args: Vec<String> = args
                .iter()
                .map(|a| a.replace("__DIR__", &dir.to_string_lossy()))
                .collect();
            let mut cmd = Command::new(bin);
            cmd.args(&args)
                .current_dir(&dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            match cmd.spawn() {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
    }

    bail!("no terminal emulator found (set $TERMINAL or install xdg-terminal-exec / a DE terminal)")
}

fn spawn_simple(term: &str, dir: &Path) -> Result<bool> {
    // Allow "$TERMINAL" to be a full command line; take first token as binary.
    let mut parts = shell_words(term);
    if parts.is_empty() {
        return Ok(false);
    }
    let bin = parts.remove(0);
    if !which(&bin) && !Path::new(&bin).exists() {
        return Ok(false);
    }
    let mut cmd = Command::new(&bin);
    cmd.args(&parts)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(cmd.spawn().is_ok())
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| {
                let candidate = p.join(bin);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

fn shell_words(s: &str) -> Vec<String> {
    // Minimal split; good enough for TERMINAL=kitty / "gnome-terminal".
    s.split_whitespace().map(|w| w.to_string()).collect()
}