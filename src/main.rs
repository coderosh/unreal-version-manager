mod config;
mod desktop;
mod engine;
mod jobs;
mod project;
mod registry;

use config::Config;
use eframe::egui::{self, Align, Align2, Color32, Layout, RichText};
use engine::{same_path, Engine};
use jobs::JobHandle;
use project::Project;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Unreal Version Manager")
            .with_app_id("unreal-version-manager")
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Unreal Version Manager",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Engines,
    Projects,
}

enum Action {
    Refresh,
    GetVersions,
    InstallDownloaded(PathBuf),
    IgnoreDownload(PathBuf),
    ChangeProjectsRoot,
    SetX11(bool),
    SetDeleteZip(bool),
    ChangeRoot,
    InstallZip,
    AddEngineFolder,
    Launch(usize),
    OpenFolder(PathBuf),
    Register(usize),
    Unregister(usize),
    AddMenu(usize),
    RemoveMenu(usize),
    AskDelete(usize),
    Forget(usize),
    CancelJob(usize),
    DismissJob(usize),
    AddProject,
    ScanProjects,
    OpenProject(usize, usize),
    SetProjectEngine(usize, usize),
    RemoveProject(usize),
}

struct Confirm {
    root: PathBuf,
    label: String,
}

struct App {
    cfg: Config,
    engines: Vec<Engine>,
    regs: Vec<(String, PathBuf)>,
    projects: Vec<Project>,
    jobs: Vec<(JobHandle, bool)>,
    /// Finished engine zips found in the downloads folder on the last refresh.
    ready_downloads: Vec<(PathBuf, u64)>,
    ignored_downloads: HashSet<PathBuf>,
    tab: Tab,
    message: Option<(String, bool, Instant)>,
    confirm: Option<Confirm>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        cc.egui_ctx.set_style(style);

        let mut app = App {
            cfg: Config::load(),
            engines: Vec::new(),
            regs: Vec::new(),
            projects: Vec::new(),
            jobs: Vec::new(),
            tab: Tab::Engines,
            message: None,
            confirm: None,
            ready_downloads: Vec::new(),
            ignored_downloads: HashSet::new(),
        };
        app.refresh();
        app.scan_projects_root();
        app.check_downloads();
        app
    }

    /// Picks up .uproject files under the projects folder (default ~/UnrealProjects).
    fn scan_projects_root(&mut self) {
        let mut found = Vec::new();
        project::find_projects(&self.cfg.projects_root, 3, &mut found);
        let missing = found.iter().any(|p| !self.cfg.projects.iter().any(|q| same_path(q, p)));
        if missing {
            self.add_project_paths(found);
        }
    }

    /// Looks in the downloads folder for finished engine zips (on startup and on Refresh).
    fn check_downloads(&mut self) {
        let ready = jobs::finished_downloads(&self.cfg.downloads_dir, &self.cfg.engines_root);
        self.ready_downloads = ready.into_iter().filter(|(p, _)| !self.ignored_downloads.contains(p)).collect();
    }

    fn start_install(&mut self, zip: PathBuf, ctx: &egui::Context) {
        let job = jobs::spawn_install(zip, self.cfg.engines_root.clone(), self.cfg.delete_zip_after_install, ctx.clone());
        self.jobs.push((job, false));
    }

    fn refresh(&mut self) {
        self.engines = engine::scan(&self.cfg.engines_root, &self.cfg.extra_engines);
        self.regs = registry::installations();
        self.projects = self.cfg.projects.iter().map(|p| project::load(p)).collect();
    }

    fn save(&mut self) {
        if let Err(e) = self.cfg.save() {
            self.err(format!("Could not save settings: {e}"));
        }
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), false, Instant::now()));
    }

    fn err(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), true, Instant::now()));
    }

    fn guid(&self, e: &Engine) -> Option<String> {
        registry::guid_for(&e.root, &self.regs)
    }

    fn add_project_paths(&mut self, paths: Vec<PathBuf>) -> usize {
        let mut added = 0;
        for p in paths {
            if !self.cfg.projects.iter().any(|q| same_path(q, &p)) {
                self.cfg.projects.push(p);
                added += 1;
            }
        }
        self.save();
        self.refresh();
        added
    }

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Refresh => {
                self.refresh();
                self.check_downloads();
                let dl = match self.ready_downloads.len() {
                    0 => format!("no new engine zips in {}", self.cfg.downloads_dir.display()),
                    n => format!("{n} download(s) ready to install"),
                };
                self.info(format!("Found {} engine(s), {dl}", self.engines.len()));
            }
            Action::GetVersions => {
                match engine::open_url("https://www.unrealengine.com/linux") {
                    Ok(()) => self.info(format!(
                        "Sign in and download a version, then hit Refresh to install it from {}",
                        self.cfg.downloads_dir.display()
                    )),
                    Err(e) => self.err(format!("Could not open browser: {e}")),
                }
            }
            Action::InstallDownloaded(zip) => {
                self.ready_downloads.retain(|(p, _)| p != &zip);
                self.start_install(zip, ctx);
            }
            Action::IgnoreDownload(zip) => {
                self.ready_downloads.retain(|(p, _)| p != &zip);
                self.ignored_downloads.insert(zip);
            }
            Action::ChangeProjectsRoot => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Folder that holds your Unreal projects")
                    .pick_folder()
                {
                    self.cfg.projects_root = dir;
                    self.save();
                    self.scan_projects_root();
                }
            }
            Action::SetX11(v) => {
                self.cfg.use_x11 = v;
                self.save();
            }
            Action::SetDeleteZip(v) => {
                self.cfg.delete_zip_after_install = v;
                self.save();
            }
            Action::ChangeRoot => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Folder that holds your engine versions")
                    .set_directory(&self.cfg.engines_root)
                    .pick_folder()
                {
                    self.cfg.engines_root = dir;
                    self.save();
                    self.refresh();
                }
            }
            Action::InstallZip => {
                let start = dirs::download_dir().unwrap_or_default();
                if let Some(zip) = rfd::FileDialog::new()
                    .set_title("Pick a Linux Unreal Engine zip")
                    .add_filter("Zip archive", &["zip"])
                    .set_directory(start)
                    .pick_file()
                {
                    self.start_install(zip, ctx);
                }
            }
            Action::AddEngineFolder => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Pick an engine folder").pick_folder() {
                    match Engine::find_in(&dir, true) {
                        None => self.err(format!(
                            "No Engine/Build/Build.version found in {}",
                            dir.display()
                        )),
                        Some(e) => {
                            let inside_root = e.root.parent().is_some_and(|p| same_path(p, &self.cfg.engines_root));
                            if !inside_root && !self.cfg.extra_engines.iter().any(|p| same_path(p, &e.root)) {
                                self.cfg.extra_engines.push(e.root.clone());
                                self.save();
                            }
                            self.refresh();
                            self.info(format!("Added Unreal Engine {}", e.version));
                        }
                    }
                }
            }
            Action::Launch(i) => {
                let e = &self.engines[i];
                match engine::launch(e, None, self.cfg.use_x11) {
                    Ok(()) => self.info(format!("Launching Unreal Editor {}…", e.version)),
                    Err(err) => self.err(format!("Could not launch: {err}")),
                }
            }
            Action::OpenFolder(p) => {
                if let Err(e) = engine::open_folder(&p) {
                    self.err(format!("Could not open folder: {e}"));
                }
            }
            Action::Register(i) => match registry::register(&self.engines[i].root) {
                Ok(g) => {
                    self.regs = registry::installations();
                    self.info(format!("Registered as {g}"));
                }
                Err(e) => self.err(format!("Could not write Install.ini: {e}")),
            },
            Action::Unregister(i) => match registry::unregister(&self.engines[i].root) {
                Ok(()) => {
                    self.regs = registry::installations();
                    self.info("Unregistered");
                }
                Err(e) => self.err(format!("Could not write Install.ini: {e}")),
            },
            Action::AddMenu(i) => match desktop::create_entry(&self.engines[i]) {
                Ok(()) => self.info("Added to the app menu"),
                Err(e) => self.err(format!("Could not create menu entry: {e}")),
            },
            Action::RemoveMenu(i) => match desktop::remove_entry(&self.engines[i]) {
                Ok(()) => self.info("Removed from the app menu"),
                Err(e) => self.err(format!("Could not remove menu entry: {e}")),
            },
            Action::AskDelete(i) => {
                let e = &self.engines[i];
                self.confirm = Some(Confirm {
                    root: e.root.clone(),
                    label: format!("Unreal Engine {} ({})", e.version, e.folder_name()),
                });
            }
            Action::Forget(i) => {
                let root = self.engines[i].root.clone();
                self.cfg.extra_engines.retain(|p| !same_path(p, &root));
                self.save();
                self.refresh();
            }
            Action::CancelJob(i) => {
                let j = self.jobs[i].0.lock().unwrap();
                j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Action::DismissJob(i) => {
                self.jobs.remove(i);
            }
            Action::AddProject => {
                if let Some(files) = rfd::FileDialog::new()
                    .set_title("Add Unreal projects")
                    .add_filter("Unreal project", &["uproject"])
                    .pick_files()
                {
                    let n = self.add_project_paths(files);
                    self.info(format!("Added {n} project(s)"));
                }
            }
            Action::ScanProjects => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Scan a folder for .uproject files").pick_folder() {
                    let mut found = Vec::new();
                    project::find_projects(&dir, 4, &mut found);
                    let total = found.len();
                    let n = self.add_project_paths(found);
                    self.info(format!("Found {total} project(s), {n} new"));
                }
            }
            Action::OpenProject(pi, ei) => {
                let (p, e) = (&self.projects[pi], &self.engines[ei]);
                match engine::launch(e, Some(&p.path), self.cfg.use_x11) {
                    Ok(()) => self.info(format!("Opening {} in Unreal Editor {}…", p.name, e.version)),
                    Err(err) => self.err(format!("Could not launch: {err}")),
                }
            }
            Action::SetProjectEngine(pi, ei) => {
                let root = self.engines[ei].root.clone();
                let result = registry::register(&root)
                    .map_err(|e| e.to_string())
                    .and_then(|guid| project::set_association(&self.projects[pi].path, &guid));
                match result {
                    Ok(()) => {
                        let msg = format!("{} now uses Unreal Engine {}", self.projects[pi].name, self.engines[ei].version);
                        self.refresh();
                        self.info(msg);
                    }
                    Err(e) => self.err(format!("Could not switch engine: {e}")),
                }
            }
            Action::RemoveProject(i) => {
                let path = self.projects[i].path.clone();
                self.cfg.projects.retain(|p| p != &path);
                self.save();
                self.refresh();
            }
        }
    }

    // ---------- UI ----------

    fn top_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Unreal Version Manager").strong());
            ui.add_space(16.0);
            let mut tab = self.tab;
            ui.selectable_value(&mut tab, Tab::Engines, RichText::new(format!("Engines ({})", self.engines.len())).size(15.0));
            ui.selectable_value(&mut tab, Tab::Projects, RichText::new(format!("Projects ({})", self.projects.len())).size(15.0));
            if tab != self.tab {
                // Applied right after drawing via a tiny trick: store in memory.
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("tab"), tab == Tab::Projects));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("⟳ Refresh")
                    .on_hover_text("Rescan engines and projects, and check Downloads for finished engine zips")
                    .clicked()
                {
                    actions.push(Action::Refresh);
                }
            });
        });
        ui.add_space(4.0);
    }

    fn engines_tab(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label("Engines folder:");
            ui.monospace(self.cfg.engines_root.display().to_string());
            if ui.small_button("Change…").clicked() {
                actions.push(Action::ChangeRoot);
            }
            if ui.small_button("Open").clicked() {
                actions.push(Action::OpenFolder(self.cfg.engines_root.clone()));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("🌐 Get versions…").strong())
                .on_hover_text("Opens Epic's Linux downloads page (needs your Epic login).\nWhen the download finishes, hit Refresh to install it.")
                .clicked()
            {
                actions.push(Action::GetVersions);
            }
            if ui
                .button("＋ Install from zip…")
                .on_hover_text("Extract a Linux_Unreal_Engine_*.zip into the engines folder")
                .clicked()
            {
                actions.push(Action::InstallZip);
            }
            if ui
                .button("Add existing install…")
                .on_hover_text("Point at an engine that's already extracted somewhere else")
                .clicked()
            {
                actions.push(Action::AddEngineFolder);
            }
        });
        ui.horizontal(|ui| {
            let mut x11 = self.cfg.use_x11;
            if ui
                .checkbox(&mut x11, "Launch through XWayland")
                .on_hover_text("Sets SDL_VIDEODRIVER=x11. Unreal's native Wayland support is still shaky.")
                .changed()
            {
                actions.push(Action::SetX11(x11));
            }
            let mut del = self.cfg.delete_zip_after_install;
            if ui.checkbox(&mut del, "Delete zip after install").changed() {
                actions.push(Action::SetDeleteZip(del));
            }
        });
        ui.add_space(6.0);

        for (zip, size) in &self.ready_downloads {
            let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            egui::Frame::group(ui.style())
                .fill(ui.visuals().selection.bg_fill.gamma_multiply(0.25))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⬇ Download finished:").strong());
                        ui.label(format!("{name}  ({})", jobs::gib(*size)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Ignore").clicked() {
                                actions.push(Action::IgnoreDownload(zip.clone()));
                            }
                            if ui.button(RichText::new("Install").strong()).clicked() {
                                actions.push(Action::InstallDownloaded(zip.clone()));
                            }
                        });
                    });
                });
        }

        for (i, (job, _)) in self.jobs.iter().enumerate() {
            let j = job.lock().unwrap();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&j.title).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if j.done {
                            if ui.small_button("Dismiss").clicked() {
                                actions.push(Action::DismissJob(i));
                            }
                        } else if !j.cancel.load(std::sync::atomic::Ordering::Relaxed) && j.title.starts_with("Installing") {
                            if ui.small_button("Cancel").clicked() {
                                actions.push(Action::CancelJob(i));
                            }
                        }
                    });
                });
                if let Some(e) = &j.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                } else if j.done {
                    ui.colored_label(Color32::from_rgb(90, 180, 110), &j.status);
                } else {
                    let bar = if j.progress >= 0.0 {
                        egui::ProgressBar::new(j.progress).show_percentage()
                    } else {
                        egui::ProgressBar::new(0.0).animate(true)
                    };
                    ui.add(bar);
                    ui.label(RichText::new(&j.status).weak());
                }
            });
        }

        if self.engines.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No engines yet").size(18.0));
                ui.label(
                    RichText::new(format!(
                        "Click \"Get versions…\", download a version, then hit Refresh to install it from {}.\nEngines are extracted into {}",
                        self.cfg.downloads_dir.display(),
                        self.cfg.engines_root.display()
                    ))
                    .weak(),
                );
            });
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, e) in self.engines.iter().enumerate() {
                self.engine_card(ui, i, e, actions);
            }
        });
    }

    fn engine_card(&self, ui: &mut egui::Ui, i: usize, e: &Engine, actions: &mut Vec<Action>) {
        let guid = self.guid(e);
        let in_menu = desktop::has_entry(e);
        egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Unreal Engine {}", e.version)).size(17.0).strong());
                        let preview = e.folder_name().to_lowercase().contains("preview");
                        if preview {
                            ui.label(RichText::new("preview").small().color(ui.visuals().warn_fg_color));
                        }
                        if e.manual {
                            ui.label(RichText::new("external").small().weak());
                        }
                    });
                    ui.label(RichText::new(e.root.display().to_string()).small().weak());
                    ui.horizontal(|ui| {
                        match &guid {
                            Some(g) => {
                                ui.label(RichText::new("● registered").small().color(Color32::from_rgb(90, 180, 110)))
                                    .on_hover_text(format!("Install.ini GUID {g}"));
                            }
                            None => {
                                ui.label(RichText::new("○ not registered").small().weak())
                                    .on_hover_text("Projects can't be associated with it until it's registered");
                            }
                        }
                        if in_menu {
                            ui.label(RichText::new("● in app menu").small().weak());
                        }
                        if !e.branch.is_empty() {
                            ui.label(RichText::new(&e.branch).small().weak());
                        }
                        if !e.editor_exists() {
                            ui.label(RichText::new("⚠ UnrealEditor binary missing").small().color(ui.visuals().error_fg_color));
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("⋯", |ui| {
                        if guid.is_some() {
                            if ui.button("Unregister from Install.ini").clicked() {
                                actions.push(Action::Unregister(i));
                                ui.close();
                            }
                        }
                        if in_menu {
                            if ui.button("Remove from app menu").clicked() {
                                actions.push(Action::RemoveMenu(i));
                                ui.close();
                            }
                        }
                        if ui.button("Open folder").clicked() {
                            actions.push(Action::OpenFolder(e.root.clone()));
                            ui.close();
                        }
                        ui.separator();
                        if e.manual && ui.button("Forget (keep files)").clicked() {
                            actions.push(Action::Forget(i));
                            ui.close();
                        }
                        if ui
                            .button(RichText::new("Delete from disk…").color(ui.visuals().error_fg_color))
                            .clicked()
                        {
                            actions.push(Action::AskDelete(i));
                            ui.close();
                        }
                    });
                    if !in_menu && ui.button("Add to app menu").clicked() {
                        actions.push(Action::AddMenu(i));
                    }
                    if guid.is_none() && ui.button("Register").clicked() {
                        actions.push(Action::Register(i));
                    }
                    let launch = egui::Button::new(RichText::new("▶ Launch").strong().size(15.0));
                    if ui.add_enabled(e.editor_exists(), launch).clicked() {
                        actions.push(Action::Launch(i));
                    }
                });
            });
        });
        ui.add_space(4.0);
    }

    fn projects_tab(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui.button(RichText::new("＋ Add .uproject…").strong()).clicked() {
                actions.push(Action::AddProject);
            }
            if ui.button("Scan folder…").on_hover_text("Find every .uproject under a folder").clicked() {
                actions.push(Action::ScanProjects);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Projects folder:");
            ui.monospace(self.cfg.projects_root.display().to_string());
            if ui.small_button("Change…").on_hover_text("Scanned for .uproject files on startup").clicked() {
                actions.push(Action::ChangeProjectsRoot);
            }
        });
        ui.add_space(6.0);

        if self.projects.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No projects yet").size(18.0));
                ui.label(RichText::new("Add a .uproject to open it with a chosen engine version.").weak());
            });
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (pi, p) in self.projects.iter().enumerate() {
                let current = project::resolve(&p.association, &self.engines, &self.regs);
                egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&p.name).size(16.0).strong());
                            ui.label(RichText::new(p.path.display().to_string()).small().weak());
                            if let Some(err) = &p.error {
                                ui.colored_label(ui.visuals().error_fg_color, err);
                            } else if current.is_none() {
                                let msg = if p.association.is_empty() {
                                    "No engine set".to_string()
                                } else {
                                    format!("Engine \"{}\" isn't installed here", p.association)
                                };
                                ui.label(RichText::new(msg).small().color(ui.visuals().warn_fg_color));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.menu_button("⋯", |ui| {
                                if ui.button("Open folder").clicked() {
                                    if let Some(dir) = p.path.parent() {
                                        actions.push(Action::OpenFolder(dir.to_path_buf()));
                                    }
                                    ui.close();
                                }
                                if ui.button("Remove from list").clicked() {
                                    actions.push(Action::RemoveProject(pi));
                                    ui.close();
                                }
                            });
                            let open = egui::Button::new(RichText::new("▶ Open").strong().size(15.0));
                            let can_open = current.is_some_and(|ei| self.engines[ei].editor_exists());
                            if ui.add_enabled(can_open, open).clicked() {
                                if let Some(ei) = current {
                                    actions.push(Action::OpenProject(pi, ei));
                                }
                            }
                            let selected = match current {
                                Some(ei) => format!("UE {}  ·  {}", self.engines[ei].version, self.engines[ei].folder_name()),
                                None => "Choose engine…".into(),
                            };
                            let mut pick = current;
                            egui::ComboBox::from_id_salt(("engine", pi))
                                .width(260.0)
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (ei, e) in self.engines.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut pick,
                                            Some(ei),
                                            format!("UE {}  ·  {}", e.version, e.folder_name()),
                                        );
                                    }
                                });
                            if pick != current {
                                if let Some(ei) = pick {
                                    actions.push(Action::SetProjectEngine(pi, ei));
                                }
                            }
                        });
                    });
                });
                ui.add_space(4.0);
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Finished jobs -> rescan once.
        let mut newly_done = Vec::new();
        for (job, seen) in self.jobs.iter_mut() {
            let j = job.lock().unwrap();
            if j.done && !*seen {
                *seen = true;
                newly_done.push(j.error.clone().map(Err).unwrap_or(Ok(j.status.clone())));
            }
        }
        for r in newly_done {
            self.refresh();
            match r {
                Ok(m) => self.info(m),
                Err(e) => self.err(e),
            }
        }
        if self.jobs.iter().any(|(j, _)| !j.lock().unwrap().done) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        let mut actions = Vec::new();

        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui, &mut actions));
        if let Some(switch_to_projects) = ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("tab"))) {
            self.tab = if switch_to_projects { Tab::Projects } else { Tab::Engines };
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| match &self.message {
                Some((m, is_err, t)) if t.elapsed() < Duration::from_secs(8) => {
                    let color = if *is_err { ui.visuals().error_fg_color } else { ui.visuals().text_color() };
                    ui.colored_label(color, m);
                }
                _ => {
                    ui.label(RichText::new("Engines are registered in ~/.config/Epic/UnrealEngine/Install.ini").small().weak());
                }
            });
        });
        if let Some((_, _, t)) = &self.message {
            let left = Duration::from_secs(8).saturating_sub(t.elapsed());
            if !left.is_zero() {
                ctx.request_repaint_after(left);
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Engines => self.engines_tab(ui, &mut actions),
            Tab::Projects => self.projects_tab(ui, &mut actions),
        });

        if let Some(c) = &self.confirm {
            let mut close = false;
            let mut delete = false;
            egui::Window::new("Delete engine?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("This permanently deletes {} from disk:", c.label));
                    ui.monospace(c.root.display().to_string());
                    ui.label("It can't be undone.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Delete").color(ui.visuals().error_fg_color).strong())
                            .clicked()
                        {
                            delete = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if delete {
                let c = self.confirm.take().unwrap();
                let _ = registry::unregister(&c.root);
                if let Some(e) = self.engines.iter().find(|e| e.root == c.root) {
                    let _ = desktop::remove_entry(e);
                }
                self.cfg.extra_engines.retain(|p| !same_path(p, &c.root));
                self.save();
                let job = jobs::spawn_delete(c.root, c.label, ctx.clone());
                self.jobs.push((job, false));
                self.tab = Tab::Engines;
            } else if close {
                self.confirm = None;
            }
        }

        for a in actions {
            self.apply(a, ctx);
        }
    }
}

#[cfg(test)]
mod tests;
