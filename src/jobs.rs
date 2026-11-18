//! Background jobs: extracting an engine zip, deleting an engine folder.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Job {
    pub title: String,
    /// 0..1, or negative for "unknown".
    pub progress: f32,
    pub status: String,
    pub done: bool,
    pub error: Option<String>,
    pub cancel: Arc<AtomicBool>,
}

pub type JobHandle = Arc<Mutex<Job>>;

fn new_job(title: String) -> JobHandle {
    Arc::new(Mutex::new(Job {
        title,
        progress: -1.0,
        status: "Starting…".into(),
        done: false,
        error: None,
        cancel: Arc::new(AtomicBool::new(false)),
    }))
}

fn finish(job: &JobHandle, result: Result<String, String>, ctx: &eframe::egui::Context) {
    let mut j = job.lock().unwrap();
    j.done = true;
    match result {
        Ok(msg) => {
            j.progress = 1.0;
            j.status = msg;
        }
        Err(e) => j.error = Some(e),
    }
    ctx.request_repaint();
}

pub fn gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
}

fn free_space(p: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some(s.f_bavail as u64 * s.f_frsize as u64)
}

/// "Linux_Unreal_Engine_5.9.0_preview-1.zip" -> "5.9.0_preview-1"
pub fn folder_name_for_zip(zip: &Path) -> String {
    let stem = zip.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = stem
        .strip_prefix("Linux_Unreal_Engine_")
        .or_else(|| stem.strip_prefix("Unreal_Engine_"))
        .unwrap_or(&stem);
    if name.is_empty() { "UnrealEngine".into() } else { name.to_string() }
}

pub fn spawn_install(zip: PathBuf, engines_root: PathBuf, delete_zip: bool, ctx: eframe::egui::Context) -> JobHandle {
    let name = folder_name_for_zip(&zip);
    let job = new_job(format!("Installing {name}"));
    let j2 = job.clone();
    std::thread::spawn(move || {
        let dest = engines_root.join(&name);
        let tmp = engines_root.join(format!(".partial-{name}"));
        let res = install(&zip, &engines_root, &tmp, &dest, &j2, &ctx);
        if res.is_err() && tmp.exists() {
            j2.lock().unwrap().status = "Cleaning up partial files…".into();
            let _ = fs::remove_dir_all(&tmp);
        }
        let res = res.map(|_| {
            if delete_zip && std::fs::remove_file(&zip).is_ok() {
                format!("Installed to {} (zip deleted)", dest.display())
            } else {
                format!("Installed to {}", dest.display())
            }
        });
        finish(&j2, res, &ctx);
    });
    job
}

fn install(
    zip: &Path,
    root: &Path,
    tmp: &Path,
    dest: &Path,
    job: &JobHandle,
    ctx: &eframe::egui::Context,
) -> Result<(), String> {
    let cancel = job.lock().unwrap().cancel.clone();
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    fs::create_dir_all(root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    if tmp.exists() {
        fs::remove_dir_all(tmp).map_err(|e| e.to_string())?;
    }

    job.lock().unwrap().status = "Reading zip index…".into();
    ctx.request_repaint();
    let file = File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(BufReader::with_capacity(1 << 20, file))
        .map_err(|e| format!("Not a valid zip: {e}"))?;

    let mut total: u64 = 0;
    for i in 0..archive.len() {
        total += archive.by_index_raw(i).map_err(|e| e.to_string())?.size();
    }
    if let Some(free) = free_space(root) {
        // A little headroom on top of the unpacked size.
        if free < total + total / 20 {
            return Err(format!(
                "Not enough disk space: needs about {}, {} free in {}",
                gib(total),
                gib(free),
                root.display()
            ));
        }
    }

    fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 20];
    let mut written: u64 = 0;
    let mut last = Instant::now();
    let n = archive.len();

    for i in 0..n {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let out = tmp.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mode = f.unix_mode();
        if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
            let mut target = String::new();
            f.read_to_string(&mut target).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&out);
            std::os::unix::fs::symlink(target, &out).map_err(|e| e.to_string())?;
            continue;
        }

        let mut w = BufWriter::with_capacity(1 << 20, File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?);
        loop {
            let k = f.read(&mut buf).map_err(|e| format!("{}: {e}", out.display()))?;
            if k == 0 {
                break;
            }
            w.write_all(&buf[..k]).map_err(|e| format!("{}: {e}", out.display()))?;
            written += k as u64;
            if last.elapsed().as_millis() > 150 {
                last = Instant::now();
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let mut j = job.lock().unwrap();
                j.progress = if total > 0 { written as f32 / total as f32 } else { -1.0 };
                j.status = format!("{} / {}  ·  file {} of {}", gib(written), gib(total), i + 1, n);
                drop(j);
                ctx.request_repaint();
            }
        }
        w.flush().map_err(|e| e.to_string())?;
        drop(w);
        if let Some(m) = mode {
            if m & 0o777 != 0 {
                let _ = fs::set_permissions(&out, fs::Permissions::from_mode(m & 0o7777));
            }
        }
    }

    job.lock().unwrap().status = "Finishing…".into();
    ctx.request_repaint();

    // If everything sits in one wrapper folder, move that folder into place instead.
    let engine_dir = if tmp.join("Engine").is_dir() {
        tmp.to_path_buf()
    } else {
        let children: Vec<PathBuf> = fs::read_dir(tmp)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        match children.as_slice() {
            [only] if only.join("Engine").is_dir() => only.clone(),
            _ => tmp.to_path_buf(),
        }
    };
    fs::rename(&engine_dir, dest).map_err(|e| e.to_string())?;
    if engine_dir != tmp {
        let _ = fs::remove_dir_all(tmp);
    }
    fix_exec_bits(dest);
    Ok(())
}

/// Zips without Unix modes would leave the editor non-executable.
fn fix_exec_bits(root: &Path) {
    let dirs = [root.join("Engine/Binaries/Linux"), root.join("Engine/Build/BatchFiles/Linux")];
    for d in dirs {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let Ok(meta) = fs::symlink_metadata(&p) else { continue };
            if !meta.is_file() {
                continue;
            }
            let ext = p.extension().map(|e| e.to_string_lossy().into_owned());
            let wants_exec = matches!(ext.as_deref(), None | Some("sh"));
            let mode = meta.permissions().mode();
            if wants_exec && mode & 0o111 == 0 {
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(mode | 0o755));
            }
        }
    }
}

pub fn spawn_delete(dir: PathBuf, label: String, ctx: eframe::egui::Context) -> JobHandle {
    let job = new_job(format!("Deleting {label}"));
    job.lock().unwrap().status = format!("Removing {}…", dir.display());
    let j2 = job.clone();
    std::thread::spawn(move || {
        let res = fs::remove_dir_all(&dir)
            .map(|_| format!("Deleted {}", dir.display()))
            .map_err(|e| e.to_string());
        finish(&j2, res, &ctx);
    });
    job
}

/// Engine zips in `dir` that are fully downloaded and aren't installed yet.
pub fn finished_downloads(dir: &Path, engines_root: &Path) -> Vec<(PathBuf, u64)> {
    let mut ready = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else { return ready };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("Linux_Unreal_Engine_") && name.ends_with(".zip")) {
            continue;
        }
        // Firefox writes "<name>.part", Chromium "<name>.crdownload" while downloading.
        let busy = ["part", "crdownload", "download"]
            .iter()
            .any(|ext| dir.join(format!("{name}.{ext}")).exists());
        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
        let folder = folder_name_for_zip(&p);
        let installed = engines_root.join(&folder).exists()
            || engines_root.join(format!(".partial-{folder}")).exists();
        if !busy && !installed && size > 0 {
            ready.push((p, size));
        }
    }
    ready.sort();
    ready
}
