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

/// Decoded preview of one skin (background image + style info).
struct Preview {
    title: String,
    font: String,
    font_size: i32,
    preedit: egui::Color32,
    candidate: egui::Color32,
    highlight: egui::Color32,
    tex: Option<egui::TextureHandle>,
    tex_size: egui::Vec2,
}

fn color32(colorref: u32) -> egui::Color32 {
    // skin colors are stored 0x00BBGGRR
    egui::Color32::from_rgb(
        ((colorref >> 16) & 0xFF) as u8,
        ((colorref >> 8) & 0xFF) as u8,
        (colorref & 0xFF) as u8,
    )
}

fn decode_png(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = dec.read_info().ok()?;
    let mut out = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).ok()?;
    out.truncate(info.buffer_size());
    Some((info.width as usize, info.height as usize, out))
}

fn load_preview(ctx: &egui::Context, file: &str) -> Option<Preview> {
    let bytes = std::fs::read(config::skins_dir().join(file)).ok()?;
    let files = nineime_core::ssf::extract(&bytes)?;
    let skin = nineime_core::skin::parse(&files)?;
    let (tex, tex_size) = match &skin.scheme.pic {
        Some(png_bytes) => match decode_png(png_bytes) {
            Some((w, h, rgba)) => {
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                let tex = ctx.load_texture(format!("preview-{file}"), img, Default::default());
                let scale = (240.0 / w as f32).min(120.0 / h as f32).min(2.0);
                (Some(tex), egui::vec2(w as f32 * scale, h as f32 * scale))
            }
            None => (None, egui::Vec2::ZERO),
        },
        None => (None, egui::Vec2::ZERO),
    };
    Some(Preview {
        title: skin.name.clone(),
        font: skin.font_name.clone(),
        font_size: skin.font_size,
        preedit: color32(skin.preedit_color),
        candidate: color32(skin.candidate_color),
        highlight: color32(skin.candidate_hl_color),
        tex,
        tex_size,
    })
}

struct App {
    skins: Vec<String>,
    selected: String,
    status: String,
    deploy_rx: Option<std::sync::mpsc::Receiver<String>>,
    preview: Option<Preview>,
    preview_for: String,
    confirm_remove: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        load_cjk_font(&cc.egui_ctx);
        style(&cc.egui_ctx);
        let mut app = App {
            skins: list_skins(),
            selected: config::load().skin,
            status: "就绪".to_string(),
            deploy_rx: None,
            preview: None,
            preview_for: String::new(),
            confirm_remove: None,
        };
        app.refresh_preview(&cc.egui_ctx);
        app
    }

    fn refresh_preview(&mut self, ctx: &egui::Context) {
        if self.preview_for != self.selected {
            self.preview_for = self.selected.clone();
            self.preview = if self.selected.is_empty() {
                None
            } else {
                load_preview(ctx, &self.selected.clone())
            };
        }
    }

    fn select_skin(&mut self, ctx: &egui::Context, name: &str) {
        self.selected = name.to_string();
        let mut cfg = config::load();
        cfg.skin = name.to_string();
        self.status = match config::save(&cfg) {
            Ok(()) => format!("已启用皮肤: {name}（下次打字生效）"),
            Err(e) => format!("保存配置失败: {e}"),
        };
        self.refresh_preview(ctx);
    }

    /// Run nineime-server --deploy in the background and report the result.
    fn start_deploy(&mut self) {
        let exe = exe_dir().join("nineime-server.exe");
        if !exe.exists() {
            self.status = "未找到 nineime-server.exe".to_string();
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.deploy_rx = Some(rx);
        self.status = "正在部署（首次部署可能需要几十秒）…".to_string();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let output = std::process::Command::new(&exe).arg("--deploy").output();
            let msg = match output {
                Ok(o) if o.status.success() => {
                    let _ = std::fs::write(config::appdata_dir().join("deploy.log"), o.stdout);
                    format!("部署完成（{} 秒）", started.elapsed().as_secs())
                }
                Ok(o) => {
                    let mut log = o.stderr.clone();
                    log.extend_from_slice(&o.stdout);
                    let _ = std::fs::write(config::appdata_dir().join("deploy.log"), &log);
                    let err = String::from_utf8_lossy(&log);
                    let err = err.trim();
                    let tail = if err.len() > 500 { &err[err.len() - 500..] } else { err };
                    format!("部署失败（exit={}）: {tail}", o.status.code().unwrap_or(-1))
                }
                Err(e) => format!("部署失败: {e}"),
            };
            let _ = tx.send(msg);
        });
    }

    fn restart_server(&mut self) {
        let _ = std::process::Command::new("taskkill")
            .args(["/f", "/im", "nineime-server.exe"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.status = "已停止输入服务，下次打字时自动重启".to_string();
    }
}

fn swatch(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, egui::CornerRadius::same(3), color);
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(3), egui::Stroke::new(1.0, egui::Color32::GRAY), egui::StrokeKind::Inside);
        ui.label(label);
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // poll the deploy thread for completion
        if let Some(rx) = &self.deploy_rx {
            if let Ok(msg) = rx.try_recv() {
                self.status = msg;
                self.deploy_rx = None;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("9IME 输入法设置")
                            .size(20.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{} · Rime 引擎 · 搜狗皮肤兼容", env!("CARGO_PKG_VERSION")))
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });
            ui.add_space(6.0);
        });

        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&self.status).size(12.0));
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            let cols = 2;
            ui.columns(cols, |uis| {
                // ---- left: skin list ----
                let ui = &mut uis[0];
                ui.label(egui::RichText::new("皮肤（Sogou .ssf）").strong());
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("skins")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        if self.skins.is_empty() {
                            ui.label(
                                egui::RichText::new("还没有皮肤，点击下方“导入皮肤”。")
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        let names = self.skins.clone();
                        for name in names {
                            let is_sel = self.selected == name;
                            let resp = ui.selectable_label(
                                is_sel,
                                egui::RichText::new(&name).size(13.0),
                            );
                            if resp.clicked() {
                                self.select_skin(ctx, &name);
                            }
                            resp.context_menu(|ui| {
                                if ui.button("删除").clicked() {
                                    self.confirm_remove = Some(name.clone());
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                ui.add_space(8.0);
                if ui
                    .add(egui::Button::new("导入皮肤 (.ssf)…").min_size(egui::vec2(120.0, 28.0)))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Sogou skin", &["ssf"])
                        .pick_file()
                    {
                        let name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let _ = std::fs::create_dir_all(config::skins_dir());
                        match std::fs::copy(&path, config::skins_dir().join(&name)) {
                            Ok(_) => {
                                self.skins = list_skins();
                                self.status = format!("已导入: {name}");
                                self.select_skin(ctx, &name);
                            }
                            Err(e) => self.status = format!("导入失败: {e}"),
                        }
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("打开皮肤目录").clicked() {
                        let _ = std::fs::create_dir_all(config::skins_dir());
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(config::skins_dir())
                            .spawn();
                    }
                    let sel_nonempty = !self.selected.is_empty();
                    if ui
                        .add_enabled(sel_nonempty, egui::Button::new("删除选中"))
                        .clicked()
                    {
                        self.confirm_remove = Some(self.selected.clone());
                    }
                });

                // ---- right: preview ----
                let ui = &mut uis[1];
                ui.label(egui::RichText::new("预览").strong());
                ui.add_space(4.0);
                self.refresh_preview(ctx);
                match &self.preview {
                    Some(p) => {
                        ui.label(egui::RichText::new(&p.title).size(14.0));
                        ui.label(
                            egui::RichText::new(format!("字体: {} · {}pt", p.font, p.font_size))
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(4.0);
                        if let Some(tex) = &p.tex {
                            egui::Frame::new()
                                .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                                .corner_radius(egui::CornerRadius::same(4))
                                .show(ui, |ui| {
                                    ui.add(egui::Image::new((tex.id(), p.tex_size)));
                                });
                        } else {
                            ui.label(
                                egui::RichText::new("（该皮肤无背景图）")
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            swatch(ui, "拼音", p.preedit);
                            swatch(ui, "候选", p.candidate);
                            swatch(ui, "高亮", p.highlight);
                        });
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("未选择皮肤，使用默认样式。")
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("引擎").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let deploying = self.deploy_rx.is_some();
                if ui
                    .add_enabled(!deploying, egui::Button::new("重新部署输入方案").min_size(egui::vec2(130.0, 28.0)))
                    .clicked()
                {
                    self.start_deploy();
                }
                if ui.button("重启输入服务").clicked() {
                    self.restart_server();
                }
                if ui.button("打开日志目录").clicked() {
                    let dir = config::appdata_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("修改皮肤或词库后如未生效，可点“重启输入服务”。")
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });

        // delete confirmation
        if let Some(name) = self.confirm_remove.clone() {
            let mut open = true;
            egui::Window::new("删除皮肤")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(format!("确定删除 {name} 吗？此操作不可撤销。"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_remove = None;
                        }
                        if ui
                            .add(egui::Button::new("删除").fill(egui::Color32::from_rgb(0xC0, 0x40, 0x40)))
                            .clicked()
                        {
                            let _ = std::fs::remove_file(config::skins_dir().join(&name));
                            self.skins = list_skins();
                            if self.selected == name {
                                self.selected.clear();
                                let mut cfg = config::load();
                                cfg.skin.clear();
                                let _ = config::save(&cfg);
                                self.refresh_preview(ctx);
                            }
                            self.status = format!("已删除: {name}");
                            self.confirm_remove = None;
                        }
                    });
                });
            if !open {
                self.confirm_remove = None;
            }
        }
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
            let name = path.split('\\').last().unwrap_or("cjk").to_string();
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, name.clone());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    let mut st = (*ctx.style()).clone();
    st.spacing.item_spacing = egui::vec2(8.0, 6.0);
    st.spacing.button_padding = egui::vec2(10.0, 5.0);
    st.spacing.window_margin = egui::Margin::same(12);
    st.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    st.visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    st.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    st.visuals.selection.bg_fill = egui::Color32::from_rgb(0x2E, 0x7D, 0xE0);
    ctx.set_style(st);
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 620.0])
            .with_min_inner_size([480.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "9IME 设置",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)) as Box<dyn eframe::App>)),
    )
}
