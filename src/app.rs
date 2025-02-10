use crate::worker::{SnipTask, TaskResult};
use eframe::egui::{FontData, FontFamily};
use eframe::{egui, App};
use egui_extras;
use egui_extras::Column;
use egui_keybind::{Keybind, Shortcut};
use livesplit_hotkey::{Hook, Hotkey, KeyCode, Modifiers};
use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use tex2typst_rs::text_and_tex2typst;
use uuid::Uuid;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TypstScanData {
    mathpix_api_key: String,
    snip_items: Vec<SnipItem>,
    replace_rules: Vec<ReplaceRule>,
    main_view: MainView,
    selected_snip_item: Option<Uuid>,
    api_used: u64,
    api_limit: u64,
    hide_when_capturing: bool,
    shortcut: Shortcut,
    hotkey: Hotkey,
    clipboard_mode: ClipboardMode,
    continuous_clipboard: String,
}

impl Default for TypstScanData {
    fn default() -> Self {
        Self {
            mathpix_api_key: String::new(),
            snip_items: Vec::new(),
            replace_rules: Vec::new(),
            main_view: MainView::default(),
            selected_snip_item: None,
            api_used: 0,
            api_limit: 60000,
            hide_when_capturing: false,
            shortcut: Shortcut::new(
                Some(egui::KeyboardShortcut::new(
                    egui::Modifiers::CTRL | egui::Modifiers::ALT,
                    egui::Key::Z,
                )),
                None,
            ),
            hotkey: Hotkey {
                key_code: KeyCode::from_str("Z").unwrap(),
                modifiers: Modifiers::CONTROL | Modifiers::ALT,
            },
            clipboard_mode: ClipboardMode::CopyTypst,
            continuous_clipboard: String::new(),
        }
    }
}

pub struct TypstScan {
    data: TypstScanData,
    task_sender: Sender<SnipTask>,
    result_receiver: Receiver<TaskResult>,
    global_api_key: Arc<Mutex<String>>,
    hotkey_hook: Hook,
}

impl TypstScan {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        task_sender: Sender<SnipTask>,
        result_receiver: Receiver<TaskResult>,
        global_api_key: Arc<Mutex<String>>,
    ) -> Self {
        // add font
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "JB".to_owned(),
            Arc::new(FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"))),
        );
        fonts.font_data.insert(
            "SC".to_owned(),
            Arc::new(FontData::from_static(include_bytes!("../assets/fonts/NotoSansSC-Regular.ttf"))),
        );
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "JB".to_owned());
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(1, "SC".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(1, "SC".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let typst_scan_data = if let Some(storage) = cc.storage {
            eframe::get_value(storage, "typst_scan_data").unwrap_or_default()
        } else {
            TypstScanData::default()
        };

        *global_api_key.lock().unwrap() = typst_scan_data.mathpix_api_key.clone();

        // Create a new hotkey hook
        let hook = Hook::new().expect("Failed to create hotkey hook");
        // Define the hotkey
        let hotkey = typst_scan_data.hotkey;

        let task_sender_clone = task_sender.clone();
        hook.register(hotkey, move || {
            println!("Hotkey pressed!");
            task_sender_clone.send(SnipTask::new()).unwrap();
        })
        .expect("Failed to register hotkey");

        Self {
            data: typst_scan_data,
            task_sender,
            result_receiver,
            global_api_key,
            hotkey_hook: hook,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
enum MainView {
    Snips,
    ContinuousClipboard,
    ReplaceRules,
    Settings,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
enum ClipboardMode {
    Continuous,
    CopyTeX,
    CopyTypst,
}

impl Default for MainView {
    fn default() -> Self {
        Self::Snips
    }
}

impl App for TypstScan {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // for showing images
        egui_extras::install_image_loaders(ctx);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.data.main_view, MainView::Snips, "Snips");
                    ui.selectable_value(&mut self.data.main_view, MainView::ContinuousClipboard, "Continuous Clipboard");
                    ui.selectable_value(&mut self.data.main_view, MainView::ReplaceRules, "Replace Rules");
                    ui.selectable_value(&mut self.data.main_view, MainView::Settings, "Settings");
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.data.main_view {
            MainView::Snips => {
                const PANEL_WIDTH: f32 = 200.0;
                egui::SidePanel::left("main_left")
                    .resizable(false)
                    .exact_width(PANEL_WIDTH)
                    .show_inside(ui, |ui| {
                        if ui.button("Capture").clicked() {
                            self.task_sender.send(SnipTask::new()).unwrap();
                        }

                        ui.separator();

                        const ROW_HEIGHT: f32 = 30.0;
                        egui_extras::TableBuilder::new(ui)
                            .striped(true)
                            .resizable(false)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .column(Column::remainder().at_most(PANEL_WIDTH).clip(true).resizable(true))
                            .sense(egui::Sense::click())
                            .header(0.0, |_| {})
                            .body(|mut body| {
                                for snip_item in self.data.snip_items.iter().rev() {
                                    body.row(ROW_HEIGHT, |mut row| {
                                        row.set_selected(self.data.selected_snip_item.as_ref() == Some(&snip_item.id));
                                        row.col(|ui| {
                                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                            let label = ui.label(&snip_item.title).on_hover_text(&snip_item.title);
                                            if label.clicked() {
                                                self.data.selected_snip_item = Some(snip_item.id);
                                            }
                                        });
                                        if row.response().clicked() {
                                            self.data.selected_snip_item = Some(snip_item.id);
                                        }
                                    });
                                }
                            });
                    });

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    // display the image of the selected snip item
                    if let Some(selected_snip_item) = self.data.selected_snip_item {
                        if let Some(snip_item) = self.data.snip_items.iter_mut().find(|item| item.id == selected_snip_item) {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.add_space(10.0);
                                ui.vertical_centered(|ui| {
                                    ui.add(egui::Image::from_uri(&snip_item.local_image).max_height(250.0).corner_radius(10.0));
                                });

                                ui.add_space(32.0);
                                ui.heading("Tex");
                                ui.add(
                                    egui::TextEdit::multiline(&mut snip_item.tex)
                                        .code_editor()
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(5),
                                );

                                ui.add_space(16.0);
                                ui.horizontal(|ui| {
                                    ui.heading("Typst");
                                    if ui.button("regenerate").clicked() {
                                        snip_item.typst = text_and_tex2typst(&snip_item.tex)
                                            .map_err(|e| eprintln!("Error: {:?}", e))
                                            .unwrap_or_default();
                                    }
                                });
                                ui.add(
                                    egui::TextEdit::multiline(&mut snip_item.typst)
                                        .code_editor()
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(5),
                                );
                            });
                        }
                    }
                });
            }
            MainView::ContinuousClipboard => {
                ui.heading("Clipboard Mode");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.data.clipboard_mode, ClipboardMode::Continuous, "Continuous");
                    ui.radio_value(&mut self.data.clipboard_mode, ClipboardMode::CopyTeX, "Copy TeX");
                    ui.radio_value(&mut self.data.clipboard_mode, ClipboardMode::CopyTypst, "Copy Typst");
                });
                ui.add_space(2.0);
                ui.separator();
                ui.add_space(8.0);
                ui.heading("Continuous Clipboard");
                ui.horizontal(|ui| {
                    if ui.button("copy all").clicked() {
                        ctx.copy_text(self.data.continuous_clipboard.clone());
                    }
                    if ui.button("take all").clicked() {
                        ctx.copy_text(self.data.continuous_clipboard.clone());
                        self.data.continuous_clipboard.clear();
                    }
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.data.continuous_clipboard).desired_width(f32::INFINITY));
                });
            }
            MainView::ReplaceRules => {}
            MainView::Settings => {
                ui.scope_builder(egui::UiBuilder::new(), |ui| {
                    egui::Grid::new("settings_grid")
                        .num_columns(2)
                        .spacing([60.0, 16.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Mathpix API Key");
                            ui.add(egui::TextEdit::singleline(&mut self.data.mathpix_api_key).password(true));
                            if let Ok(mut global_api_key) = self.global_api_key.lock() {
                                if *global_api_key != self.data.mathpix_api_key {
                                    *global_api_key = self.data.mathpix_api_key.clone();
                                }
                            }
                            ui.end_row();

                            ui.label("Global Hotkey");
                            ui.horizontal(|ui| {
                                ui.add(Keybind::new(&mut self.data.shortcut, "keybind_setter"));
                                if ui.button("register").clicked() {
                                    self.hotkey_hook.unregister(self.data.hotkey).unwrap();
                                    let logged_key = self.data.shortcut.keyboard().unwrap();
                                    let key_code: &str = logged_key.logical_key.name();
                                    let modifiers = logged_key.modifiers;
                                    let mut mods = Modifiers::empty();

                                    if modifiers.contains(egui::Modifiers::CTRL) {
                                        mods.insert(Modifiers::CONTROL);
                                    }
                                    if modifiers.contains(egui::Modifiers::ALT) {
                                        mods.insert(Modifiers::ALT);
                                    }
                                    if modifiers.contains(egui::Modifiers::SHIFT) {
                                        mods.insert(Modifiers::SHIFT);
                                    }

                                    self.data.hotkey = Hotkey {
                                        key_code: KeyCode::from_str(key_code).unwrap(),
                                        modifiers: mods,
                                    };
                                    dbg!(self.data.hotkey);
                                    let task_sender_clone = self.task_sender.clone();
                                    self.hotkey_hook
                                        .register(self.data.hotkey, move || {
                                            println!("Hotkey pressed!");
                                            task_sender_clone.send(SnipTask::new()).unwrap();
                                        })
                                        .expect("Failed to register hotkey");
                                }
                            });
                            ui.end_row();

                            ui.label("Delete All Snips");
                            if ui.button("delete!!!").clicked() {
                                self.data.snip_items.clear();
                                self.data.selected_snip_item = None;
                            }
                            ui.end_row();

                            ui.label("API usage");
                            ui.add(egui::ProgressBar::new(self.data.api_used as f32 / self.data.api_limit as f32).show_percentage());
                            ui.end_row();
                        });
                });
            }
        });

        // check the results in the channel
        if let Ok(result) = self.result_receiver.try_recv() {
            match self.data.clipboard_mode {
                ClipboardMode::Continuous => {
                    self.data.continuous_clipboard.push_str(&result.typst);
                    self.data.continuous_clipboard.push_str("\n");
                }
                ClipboardMode::CopyTeX => {
                    ctx.copy_text(result.text.clone());
                }
                ClipboardMode::CopyTypst => {
                    ctx.copy_text(result.typst.clone());
                }
            }

            self.data.snip_items.push(SnipItem {
                id: result.id,
                title: result.title,
                local_image: format!("file://{}", result.local_image),
                original_image: result.original_image,
                rendered_image: result.rendered_image,
                tex: result.text,
                typst: result.typst,
            });
            self.data.selected_snip_item = Some(result.id);
            self.data.api_used = result.snip_count;
            self.data.api_limit = result.snip_limit;
        }
    }

    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "typst_scan_data", &self.data);
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SnipItem {
    id: Uuid,
    title: String,
    local_image: String,
    original_image: String,
    rendered_image: String,
    tex: String,
    typst: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ReplaceRule {
    pattern: String,
    replacement: String,
}
