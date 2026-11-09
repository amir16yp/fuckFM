use super::{NetworkHost, RemoteMount};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn list_network_hosts() -> Result<Vec<NetworkHost>> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    // `net view` lists workgroup computers.
    if let Ok(text) = run_capture("net", &["view"]) {
        for line in text.lines() {
            let t = line.trim();
            if !t.starts_with("\\\\") {
                continue;
            }
            let host = t
                .trim_start_matches('\\')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_start_matches('\\');
            if host.is_empty() || !seen.insert(host.to_string()) {
                continue;
            }
            out.push(NetworkHost {
                name: host.to_string(),
                uri: format!("\\\\{host}"),
            });
        }
    }
    Ok(out)
}

pub fn mount_remote(
    uri: &str,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<RemoteMount> {
    let uri = uri.trim();
    let unc = to_unc(uri);
    if PathBuf::from(&unc).exists() {
        return Ok(RemoteMount {
            uri: uri.to_string(),
            path: PathBuf::from(&unc),
        });
    }
    let mut args = vec!["use".to_string(), unc.clone()];
    if let Some(p) = password {
        args.push(p.to_string());
    }
    if let Some(u) = user {
        args.push("/user:".to_string());
        args.push(u.to_string());
    }
    args.push("/persistent:no".into());
    let status = Command::new("net")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("net use")?;
    if !status.success() {
        bail!("net use failed for {unc}");
    }
    Ok(RemoteMount {
        uri: uri.to_string(),
        path: PathBuf::from(unc),
    })
}

pub fn unmount_remote(uri_or_path: &str) -> Result<()> {
    let unc = to_unc(uri_or_path);
    let status = Command::new("net")
        .args(["use", &unc, "/delete", "/y"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("net use /delete")?;
    if !status.success() {
        bail!("net use /delete failed for {unc}");
    }
    Ok(())
}

fn to_unc(uri: &str) -> String {
    let s = uri.trim();
    if s.starts_with("\\\\") {
        return s.trim_end_matches('\\').to_string();
    }
    if let Some(rest) = s.strip_prefix("smb://") {
        let parts: Vec<&str> = rest.trim_end_matches('/').split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return s.to_string();
        }
        return format!("\\\\{}", parts.join("\\"));
    }
    s.to_string()
}

fn run_capture(bin: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("spawn {bin}"))?;
    if !out.status.success() {
        bail!("{bin} failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
