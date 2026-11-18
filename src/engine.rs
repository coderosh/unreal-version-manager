use serde::Deserialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct Engine {
    /// Folder that contains `Engine/`.
    pub root: PathBuf,
    /// "5.9.0"
    pub version: String,
    pub branch: String,
    pub editor: PathBuf,
    /// Added by hand (outside the engines folder).
    pub manual: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BuildVersion {
    major_version: u32,
    minor_version: u32,
    patch_version: u32,
    #[serde(default)]
    branch_name: String,
}

impl Engine {
    /// Reads `Engine/Build/Build.version` in `dir`.
    pub fn from_dir(dir: &Path, manual: bool) -> Option<Engine> {
        let text = std::fs::read_to_string(dir.join("Engine/Build/Build.version")).ok()?;
        let v: BuildVersion = serde_json::from_str(&text).ok()?;
        Some(Engine {
            root: dir.to_path_buf(),
            version: format!("{}.{}.{}", v.major_version, v.minor_version, v.patch_version),
            branch: v.branch_name,
            editor: dir.join("Engine/Binaries/Linux/UnrealEditor"),
            manual,
        })
    }

    /// Looks in `dir` itself, then one level down (zips often wrap everything in a folder).
    pub fn find_in(dir: &Path, manual: bool) -> Option<Engine> {
        if let Some(e) = Engine::from_dir(dir, manual) {
            return Some(e);
        }
        let mut found = None;
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(e) = Engine::from_dir(&p, manual) {
                    found = Some(e);
                    break;
                }
            }
        }
        found
    }

    pub fn folder_name(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    pub fn major_minor(&self) -> String {
        self.version.split('.').take(2).collect::<Vec<_>>().join(".")
    }

    pub fn editor_exists(&self) -> bool {
        self.editor.is_file()
    }

    fn version_key(&self) -> Vec<u32> {
        self.version.split('.').filter_map(|p| p.parse().ok()).collect()
    }
}

/// Newest first; ties broken by folder name.
fn newest_first(a: &Engine, b: &Engine) -> Ordering {
    b.version_key()
        .cmp(&a.version_key())
        .then_with(|| b.folder_name().cmp(&a.folder_name()))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let ca = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let cb = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

pub fn scan(root: &Path, extra: &[PathBuf]) -> Vec<Engine> {
    let mut out: Vec<Engine> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let p = entry.path();
            let hidden = p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if p.is_dir() && !hidden {
                if let Some(e) = Engine::find_in(&p, false) {
                    out.push(e);
                }
            }
        }
    }
    for p in extra {
        if let Some(e) = Engine::find_in(p, true) {
            if !out.iter().any(|o| same_path(&o.root, &e.root)) {
                out.push(e);
            }
        }
    }
    out.sort_by(newest_first);
    out
}

/// Starts a program detached from this app and reaps it in the background.
pub fn spawn_detached(
    program: &Path,
    args: &[&Path],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .envs(envs.iter().copied());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn launch(engine: &Engine, project: Option<&Path>, x11: bool) -> std::io::Result<()> {
    let args: Vec<&Path> = project.into_iter().collect();
    let envs: &[(&str, &str)] = if x11 { &[("SDL_VIDEODRIVER", "x11")] } else { &[] };
    spawn_detached(&engine.editor, &args, engine.editor.parent(), envs)
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    spawn_detached(Path::new("xdg-open"), &[Path::new(url)], None, &[])
}

pub fn open_folder(path: &Path) -> std::io::Result<()> {
    spawn_detached(Path::new("xdg-open"), &[path], None, &[])
}
