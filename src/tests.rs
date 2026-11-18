use crate::{engine, jobs, project, registry};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("uvm-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Builds a fake engine zip wrapped in a top-level folder, like Epic's downloads.
fn fake_zip(path: &PathBuf) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let exe = SimpleFileOptions::default().unix_permissions(0o755);
    let normal = SimpleFileOptions::default().unix_permissions(0o644);
    z.add_directory("Linux_Unreal_Engine_5.9.0_preview-1/Engine/", normal).unwrap();
    z.start_file("Linux_Unreal_Engine_5.9.0_preview-1/Engine/Build/Build.version", normal).unwrap();
    z.write_all(br#"{"MajorVersion":5,"MinorVersion":9,"PatchVersion":0,"BranchName":"++UE5+Release-5.9"}"#).unwrap();
    z.start_file("Linux_Unreal_Engine_5.9.0_preview-1/Engine/Binaries/Linux/UnrealEditor", exe).unwrap();
    z.write_all(b"#!/bin/sh\necho editor \"$@\"\n").unwrap();
    z.start_file("Linux_Unreal_Engine_5.9.0_preview-1/Engine/Binaries/Linux/libfoo.so.1", normal).unwrap();
    z.write_all(&vec![7u8; 3_000_000]).unwrap();
    z.add_symlink("Linux_Unreal_Engine_5.9.0_preview-1/Engine/Binaries/Linux/libfoo.so", "libfoo.so.1", normal).unwrap();
    z.finish().unwrap();
}

#[test]
fn install_scan_register_and_associate() {
    let base = tmpdir("all");
    std::env::set_var("XDG_CONFIG_HOME", base.join("config"));

    // Install from zip
    let zip = base.join("Linux_Unreal_Engine_5.9.0_preview-1.zip");
    fake_zip(&zip);
    assert_eq!(jobs::folder_name_for_zip(&zip), "5.9.0_preview-1");
    let root = base.join("engines");
    let ctx = eframe::egui::Context::default();
    let job = jobs::spawn_install(zip, root.clone(), false, ctx);
    for _ in 0..200 {
        if job.lock().unwrap().done { break }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let j = job.lock().unwrap();
    assert!(j.done && j.error.is_none(), "install failed: {:?}", j.error);
    drop(j);

    let dest = root.join("5.9.0_preview-1");
    let editor = dest.join("Engine/Binaries/Linux/UnrealEditor");
    assert!(std::fs::metadata(&editor).unwrap().permissions().mode() & 0o111 != 0, "editor not executable");
    assert!(std::fs::symlink_metadata(dest.join("Engine/Binaries/Linux/libfoo.so")).unwrap().file_type().is_symlink());
    assert!(!root.join(".partial-5.9.0_preview-1").exists());

    // Scan
    let engines = engine::scan(&root, &[]);
    assert_eq!(engines.len(), 1);
    assert_eq!(engines[0].version, "5.9.0");
    assert_eq!(engines[0].major_minor(), "5.9");
    assert!(engines[0].editor_exists());

    // Register (idempotent) and keep other ini sections
    let ini = base.join("config/Epic/UnrealEngine/Install.ini");
    std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
    std::fs::write(&ini, "[Other]\nfoo=bar\n").unwrap();
    let g1 = registry::register(&dest).unwrap();
    let g2 = registry::register(&dest).unwrap();
    assert_eq!(g1, g2);
    assert!(g1.starts_with('{') && g1.ends_with('}'));
    let text = std::fs::read_to_string(&ini).unwrap();
    assert!(text.contains("[Other]\nfoo=bar") && text.contains(&format!("{g1}={}", dest.display())), "{text}");

    // Project association round trip
    let proj = base.join("MyGame/MyGame.uproject");
    std::fs::create_dir_all(proj.parent().unwrap()).unwrap();
    std::fs::write(&proj, "{\n\t\"FileVersion\": 3,\n\t\"EngineAssociation\": \"5.4\",\n\t\"Category\": \"\"\n}\n").unwrap();
    let regs = registry::installations();
    let p = project::load(&proj);
    assert_eq!(project::resolve(&p.association, &engines, &regs), None);
    project::set_association(&proj, &g1).unwrap();
    let p = project::load(&proj);
    assert_eq!(project::resolve(&p.association, &engines, &regs), Some(0));
    let text = std::fs::read_to_string(&proj).unwrap();
    assert!(text.find("FileVersion").unwrap() < text.find("Category").unwrap(), "key order lost");
    assert!(text.contains("\t\"FileVersion\""), "tab indent lost");

    let mut found = Vec::new();
    project::find_projects(&base, 3, &mut found);
    assert_eq!(found, vec![proj]);

    registry::unregister(&dest).unwrap();
    assert!(registry::installations().is_empty());

    // Installing again into an existing folder must fail cleanly
    let zip2 = base.join("Linux_Unreal_Engine_5.9.0_preview-1.zip");
    let job = jobs::spawn_install(zip2, root.clone(), false, eframe::egui::Context::default());
    for _ in 0..100 {
        if job.lock().unwrap().done { break }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(job.lock().unwrap().error.as_deref().unwrap_or("").contains("already exists"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn download_watcher_waits_for_finished_zip() {
    let base = tmpdir("watch");
    let dl = base.join("Downloads");
    let root = base.join("engines");
    std::fs::create_dir_all(&dl).unwrap();
    let zip = dl.join("Linux_Unreal_Engine_5.8.1.zip");
    std::fs::write(&zip, b"abc").unwrap();
    std::fs::write(dl.join("Linux_Unreal_Engine_5.8.1.zip.part"), b"").unwrap();
    std::fs::write(dl.join("something_else.zip"), b"abc").unwrap();
    // still downloading (.part present)
    assert!(jobs::finished_downloads(&dl, &root).is_empty());
    // finished
    std::fs::remove_file(dl.join("Linux_Unreal_Engine_5.8.1.zip.part")).unwrap();
    let ready = jobs::finished_downloads(&dl, &root);
    assert_eq!(ready, vec![(zip.clone(), 3)]);
    // already installed -> not offered
    std::fs::create_dir_all(root.join("5.8.1")).unwrap();
    assert!(jobs::finished_downloads(&dl, &root).is_empty());
    let _ = std::fs::remove_dir_all(&base);
}
