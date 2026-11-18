//! App-menu entries (~/.local/share/applications/uvm-unreal-*.desktop).

use crate::engine::Engine;
use std::path::PathBuf;

fn apps_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("applications")
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect()
}

pub fn entry_path(e: &Engine) -> PathBuf {
    apps_dir().join(format!("uvm-unreal-{}.desktop", slug(&e.folder_name())))
}

pub fn has_entry(e: &Engine) -> bool {
    entry_path(e).is_file()
}

fn quote_exec(p: &str) -> String {
    // Desktop Entry spec: quote, escape " ` $ \ inside quotes, then escape \ again for the value.
    let mut s = String::from("\"");
    for c in p.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s.replace('\\', "\\\\")
}

fn icon_for(e: &Engine) -> String {
    let candidates = [
        "Engine/Source/Runtime/Launch/Resources/Linux/UnrealEngine.png",
        "Engine/Source/Programs/UnrealVersionSelector/Private/Linux/Resources/Icon.png",
        "Engine/Content/Editor/Slate/Icons/UnrealEngine.png",
    ];
    candidates
        .iter()
        .map(|c| e.root.join(c))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "applications-games".into())
}

pub fn create_entry(e: &Engine) -> std::io::Result<()> {
    let dir = apps_dir();
    std::fs::create_dir_all(&dir)?;
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Unreal Engine {ver} ({folder})\n\
         GenericName=Game Engine Editor\n\
         Comment=Unreal Editor {ver}\n\
         Exec={exec} %f\n\
         Path={path}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Development;Graphics;3DGraphics;\n\
         StartupWMClass=UnrealEditor\n",
        ver = e.version,
        folder = e.folder_name(),
        exec = quote_exec(&e.editor.display().to_string()),
        path = e.editor.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        icon = icon_for(e),
    );
    std::fs::write(entry_path(e), content)?;
    let _ = std::process::Command::new("update-desktop-database").arg(&dir).output();
    Ok(())
}

pub fn remove_entry(e: &Engine) -> std::io::Result<()> {
    let p = entry_path(e);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}
