//! 9IME deployer: skin management + deploy trigger (egui).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use nineime_core::config;

fn exe_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default()
}

fn list_skins() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(config::skins_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.to_lowercase().ends_with(".ssf") {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

struct App {
    skins: Vec<String>,
    selected: String,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        load_cjk_font(&cc.egui_ctx);
        Self::default_state()
    }

    fn default_state() -> Self {
        App {
            skins: list_skins(),
            selected: config::load().skin,
            status: String::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("9IME 设置");
            ui.add_space(6.0);
            ui.label("皮肤（Sogou .ssf）");
            let mut remove: Option<usize> = None;
            for (i, name) in self.skins.iter().enumerate() {
                ui.horizontal(|ui| {
                    let is_sel = self.selected == *name;
                    if ui.selectable_label(is_sel, name).clicked() {
                        self.selected = name.clone();
                        let mut cfg = config::load();
                        cfg.skin = name.clone();
                        if config::save(&cfg).is_ok() {
                            self.status = format!("已启用皮肤: {name}");
                        }
                    }
                    if ui.small_button("删除").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                let name = self.skins.remove(i);
                let _ = std::fs::remove_file(config::skins_dir().join(&name));
                if self.selected == name {
                    self.selected.clear();
                }
                self.status = format!("已删除: {name}");
            }
            ui.add_space(8.0);
            if ui.button("导入皮肤 (.ssf)...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Sogou skin", &["ssf"])
                    .pick_file()
                {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let _ = std::fs::create_dir_all(config::skins_dir());
                    if std::fs::copy(&path, config::skins_dir().join(&name)).is_ok() {
                        self.skins = list_skins();
                        self.status = format!("已导入: {name}");
                    } else {
                        self.status = "导入失败".to_string();
                    }
                }
            }
            ui.add_space(8.0);
            if ui.button("重新部署输入方案 (deploy)").clicked() {
                let exe = exe_dir().join("nineime-server.exe");
                if exe.exists() {
                    let _ = std::process::Command::new(&exe)
                        .arg("--deploy")
                        .spawn();
                    self.status = "正在部署，请稍候...".to_string();
                } else {
                    self.status = "未找到 nineime-server.exe".to_string();
                }
            }
            ui.add_space(8.0);
            ui.label(&self.status);
        });
    }
}

/// egui ships no CJK glyphs; add a system font so Chinese UI text works.
fn load_cjk_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\msyhl.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let name = path.split("\\").last().unwrap_or("cjk").to_string();
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push(name.clone());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([420.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "9IME 设置",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)) as Box<dyn eframe::App>)),
    )
}
