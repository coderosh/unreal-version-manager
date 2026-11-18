use crate::engine::{same_path, Engine};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Project {
    pub path: PathBuf,
    pub name: String,
    /// Raw "EngineAssociation": a GUID like "{...}", a version like "5.4", or "".
    pub association: String,
    pub error: Option<String>,
}

pub fn load(path: &Path) -> Project {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut p = Project { path: path.to_path_buf(), name, association: String::new(), error: None };
    match std::fs::read_to_string(path) {
        Err(e) => p.error = Some(e.to_string()),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Err(e) => p.error = Some(format!("Invalid .uproject: {e}")),
            Ok(v) => {
                p.association = v
                    .get("EngineAssociation")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            }
        },
    }
    p
}

/// Rewrites "EngineAssociation", keeping key order and Unreal's tab indentation.
pub fn set_association(path: &Path, association: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let obj = v.as_object_mut().ok_or("The .uproject is not a JSON object")?;
    obj.insert("EngineAssociation".into(), Value::String(association.into()));

    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(&v, &mut ser).map_err(|e| e.to_string())?;
    buf.push(b'\n');
    std::fs::write(path, buf).map_err(|e| e.to_string())
}

/// Which installed engine a project's association points to.
pub fn resolve(association: &str, engines: &[Engine], regs: &[(String, PathBuf)]) -> Option<usize> {
    if association.is_empty() {
        return None;
    }
    if association.starts_with('{') {
        let (_, path) = regs.iter().find(|(g, _)| g.eq_ignore_ascii_case(association))?;
        return engines.iter().position(|e| same_path(&e.root, path));
    }
    // Version string like "5.4" (engines are sorted newest first).
    engines.iter().position(|e| e.major_minor() == association)
}

/// Finds .uproject files under `dir`, a few levels deep, skipping build output folders.
pub fn find_projects(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            let skip = name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "Binaries" | "Intermediate" | "Saved" | "DerivedDataCache" | "Content" | "node_modules" | "target"
                );
            if depth > 0 && !skip {
                find_projects(&p, depth - 1, out);
            }
        } else if name.ends_with(".uproject") {
            out.push(p);
        }
    }
}
