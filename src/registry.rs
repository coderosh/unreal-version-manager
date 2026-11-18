//! Reads and writes ~/.config/Epic/UnrealEngine/Install.ini, the file Unreal on Linux
//! uses to map engine GUIDs (a .uproject's "EngineAssociation") to install folders.

use crate::engine::same_path;
use std::path::{Path, PathBuf};

const SECTION: &str = "[Installations]";

fn ini_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Epic/UnrealEngine/Install.ini")
}

/// (GUID, path) pairs from the [Installations] section.
pub fn installations() -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(ini_path()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.eq_ignore_ascii_case(SECTION);
            continue;
        }
        if inside {
            if let Some((k, v)) = t.split_once('=') {
                out.push((k.trim().to_string(), PathBuf::from(v.trim())));
            }
        }
    }
    out
}

fn write(entries: &[(String, PathBuf)]) -> std::io::Result<()> {
    let path = ini_path();
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    // Keep every other section as-is, rewrite [Installations].
    let mut kept = String::new();
    let mut inside = false;
    for line in old.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.eq_ignore_ascii_case(SECTION);
        }
        if !inside {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    let mut out = kept.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(SECTION);
    out.push('\n');
    for (k, v) in entries {
        out.push_str(&format!("{k}={}\n", v.display()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, out)
}

pub fn guid_for(root: &Path, regs: &[(String, PathBuf)]) -> Option<String> {
    regs.iter()
        .find(|(_, p)| same_path(p, root))
        .map(|(k, _)| k.clone())
}

/// Registers the engine if needed and returns its GUID.
pub fn register(root: &Path) -> std::io::Result<String> {
    let mut regs = installations();
    if let Some(g) = guid_for(root, &regs) {
        return Ok(g);
    }
    let guid = format!("{{{}}}", uuid::Uuid::new_v4().hyphenated().to_string().to_uppercase());
    regs.push((guid.clone(), root.to_path_buf()));
    write(&regs)?;
    Ok(guid)
}

pub fn unregister(root: &Path) -> std::io::Result<()> {
    let mut regs = installations();
    let before = regs.len();
    regs.retain(|(_, p)| !same_path(p, root));
    if regs.len() != before {
        write(&regs)?;
    }
    Ok(())
}
