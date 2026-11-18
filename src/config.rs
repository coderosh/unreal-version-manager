use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted app settings: ~/.config/unreal-version-manager/config.json
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Folder that holds one sub-folder per engine version.
    pub engines_root: PathBuf,
    /// Engines installed somewhere else, added by hand.
    pub extra_engines: Vec<PathBuf>,
    /// Known .uproject files.
    pub projects: Vec<PathBuf>,
    /// Scanned for .uproject files on startup.
    pub projects_root: PathBuf,
    /// Watched for finished Linux_Unreal_Engine_*.zip downloads.
    pub downloads_dir: PathBuf,
    /// Start the editor with SDL_VIDEODRIVER=x11 (XWayland); more reliable on Wayland.
    pub use_x11: bool,
    /// Remove the zip once it's installed (they're ~20-40 GB).
    pub delete_zip_after_install: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            engines_root: dirs::home_dir().unwrap_or_default().join("UnrealEngine"),
            extra_engines: Vec::new(),
            projects: Vec::new(),
            projects_root: dirs::home_dir().unwrap_or_default().join("UnrealProjects"),
            downloads_dir: dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads")),
            use_x11: true,
            delete_zip_after_install: false,
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("unreal-version-manager")
        .join("config.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, serde_json::to_string_pretty(self)?)
    }
}
