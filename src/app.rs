use eframe::egui::mutex::Mutex;
use eframe::egui::Widget;
use eframe::{egui, App};
use egui_extras;
use egui_extras::Column;
use image::buffer::ConvertBuffer;
use mouse_position;
use reqwest::blocking::multipart;
use reqwest::blocking::multipart::Part;
use reqwest::blocking::Client;
use reqwest::header;
use serde::Deserialize;
use serde_json::json;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::thread;
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
}

impl Default for TypstScanData {
    fn default() -> Self {
        Self {
            mathpix_api_key: String::new(),
            snip_items: Vec::new(),
            replace_rules: Vec::new(),
            main_view: MainView::default(),
            selected_snip_item: None,
        }
    }
}

pub struct TypstScan {
    data: TypstScanData,
    hotkey_flag: Arc<Mutex<bool>>,
    screenshot_start: Option<(i32, i32)>,
    screenshot_end: Option<(i32, i32)>,
    result_queue: Arc<Mutex<Vec<MathpixResult>>>,
    worker_thread: Option<thread::JoinHandle<()>>,
    task_sender: Sender<SnipTask>,
    result_receiver: Receiver<TaskResult>,
}

impl TypstScan {
    pub fn new(cc: &eframe::CreationContext<'_>, shared_flag: Arc<Mutex<bool>>) -> Self {
        let typst_scan_data = if let Some(storage) = cc.storage {
            eframe::get_value(storage, "typst_scan_data").unwrap_or_default()
        } else {
            TypstScanData::default()
        };

        let (task_sender, task_receiver) = mpsc::channel::<SnipTask>();
        let (result_sender, result_receiver) = mpsc::channel::<TaskResult>();
        let worker_thread = thread::spawn(move || {
            // Options payload (similar to the Swift `options` dictionary)
            let options_payload = json!({
                "config": {
                    "include_diagrams": true,
                    "idiomatic_eqn_arrays": true,
                    "math_display_delimiters": ["\n\\[\n", "\n\\]\n"],
                    "ocr_version": 2,
                    "mmd_version": "1.3.0",
                    "math_inline_delimiters": ["\\(", "\\)"],
                    "rm_fonts": false
                },
                "metadata": {
                    "version": "3.4.11",
                    "platform": "macOS 15.2.0",
                    "count": 6,
                    "input_type": "crop"
                }
            });
            let client = Client::builder()
                .pool_idle_timeout(None)
                .build()
                .expect("Failed to create reqwest client");

            for snip_task in task_receiver {
                let mut headers = header::HeaderMap::new();
                headers.insert(
                    "Authorization",
                    header::HeaderValue::from_str(&format!("Bearer {}", snip_task.api_key)).unwrap(),
                );
                headers.insert("Accept", header::HeaderValue::from_static("*/*"));
                headers.insert(
                    "User-Agent",
                    header::HeaderValue::from_static("Mathpix Snip MacOS App v3.4.11(3411.2)"),
                );

                let screenshot_data = std::fs::read(snip_task.screenshot_path).expect("Failed to read screenshot file");

                let form = multipart::Form::new()
                    .part(
                        "file",
                        Part::bytes(screenshot_data).file_name("image.png").mime_str("image/png").unwrap(),
                    )
                    .part(
                        "options_json",
                        Part::text(options_payload.to_string()).mime_str("application/json").unwrap(),
                    );

                let response = client
                    .post("https://snip-api.mathpix.com/v1/snips-multipart")
                    .headers(headers.clone())
                    .multipart(form)
                    .send()
                    .unwrap();

                match response.json::<MathpixResult>() {
                    Ok(mathpix_result) => {
                        result_sender
                            .send(TaskResult {
                                id: snip_task.id,
                                original_image: mathpix_result.images.original.fullsize.url.clone(),
                                rendered_image: mathpix_result.images.rendered.fullsize.url.clone(),
                                text: mathpix_result.text.clone(),
                                latex: mathpix_result.latex.clone(),
                                typst: text_and_tex2typst(&mathpix_result.text),
                                title: mathpix_result.title.clone(),
                                snip_count: mathpix_result.snip_count,
                                snip_limit: mathpix_result.snip_limit,
                            })
                            .unwrap();
                    }
                    Err(e) => {
                        eprintln!("Error: {:?}", e);
                    }
                }
            }
        });

        Self {
            data: typst_scan_data,
            hotkey_flag: shared_flag,
            screenshot_start: None,
            screenshot_end: None,
            result_queue: Arc::new(Mutex::new(Vec::new())),
            worker_thread: Some(worker_thread),
            task_sender,
            result_receiver,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
enum MainView {
    Main,
    ContinuousClipboard,
    ReplaceRules,
    Settings,
}

impl Default for MainView {
    fn default() -> Self {
        Self::Main
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
                    ui.selectable_value(&mut self.data.main_view, MainView::Main, "Main");
                    ui.selectable_value(&mut self.data.main_view, MainView::ContinuousClipboard, "Continuous Clipboard");
                    ui.selectable_value(&mut self.data.main_view, MainView::ReplaceRules, "Replace Rules");
                    ui.selectable_value(&mut self.data.main_view, MainView::Settings, "Settings");
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.data.main_view {
            MainView::Main => {
                const PANEL_WIDTH: f32 = 200.0;
                egui::SidePanel::left("main_left")
                    .resizable(false)
                    .exact_width(PANEL_WIDTH)
                    .show_inside(ui, |ui| {
                        if ui.button("Capture").clicked() {
                            *self.hotkey_flag.lock() = true;
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
                                for snip_item in &self.data.snip_items {
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
                                ui.vertical_centered(|ui| {
                                    ui.add(egui::Image::from_uri(&snip_item.display_image).max_height(250.0));
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
                                ui.heading("Typst");
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
            MainView::ContinuousClipboard => {}
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
                            ui.end_row();

                            ui.label("delete all");
                            if ui.button("delete all").clicked() {
                                self.data.snip_items.clear();
                                self.data.selected_snip_item = None;
                            }
                            ui.end_row();

                            ui.label("API usage");
                            ui.add(egui::ProgressBar::new(0.618).show_percentage());
                            ui.end_row();

                            ui.checkbox(&mut self.hotkey_flag.lock(), "Shared Flag");
                            ui.end_row();
                        });
                });
            }
        });

        // start the screenshot
        if *self.hotkey_flag.lock() {
            *self.hotkey_flag.lock() = false;
            if let Some(screenshot_path) = get_screenshot() {
                let id = Uuid::new_v4();
                self.data.snip_items.push(SnipItem {
                    id,
                    title: "Processing...".to_string(),
                    display_image: screenshot_path.to_string_lossy().to_string(),
                    original_image: String::new(),
                    rendered_image: String::new(),
                    tex: String::new(),
                    typst: String::new(),
                });
                self.task_sender
                    .send(SnipTask {
                        id: Uuid::new_v4(),
                        screenshot_path,
                        api_key: self.data.mathpix_api_key.clone(),
                    })
                    .unwrap();
            }
        }

        // check the results in the channel
        if let Ok(result) = self.result_receiver.try_recv() {
            println!("Result: {:?}", result);
        }

        // put into continuous mode (so that hotkey still work when the app is not in focus)
        ctx.request_repaint();
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
    display_image: String,
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

struct SnipTask {
    id: Uuid,
    screenshot_path: std::path::PathBuf,
    api_key: String,
}

#[derive(Debug)]
struct TaskResult {
    id: Uuid,
    original_image: String,
    rendered_image: String,
    text: String,
    latex: Option<String>,
    typst: String,
    title: String,
    snip_count: u64,
    snip_limit: u64,
}

// The following is the struct for the Mathpix API response
#[derive(Debug, Deserialize)]
struct MathpixResult {
    id: String,
    status: String,
    text: String,
    latex: Option<String>,
    title: String,
    images: Images,
    confidence: f64,
    auto_rotate_degrees: i64,
    auto_rotate_confidence: f64,
    font_size: f64,
    ocr_version: u64,
    created_at: String,
    modified_at: String,
    time_ms: TimeMs,
    snip_count: u64,
    snip_limit: u64,
    extra_snips: u64,
    snip_overage_count: u64,
    folder_id: String,
}

#[derive(Debug, Deserialize)]
struct Images {
    original: ImageDetails,
    rendered: ImageDetails,
}

#[derive(Debug, Deserialize)]
struct ImageDetails {
    fullsize: UrlDetail,
    thumbnail: UrlDetail,
}

#[derive(Debug, Deserialize)]
struct UrlDetail {
    url: String,
}

#[derive(Debug, Deserialize)]
struct TimeMs {
    ocr_api_response: u64,
    read_request_body: u64,
}

#[cfg(target_os = "macos")]
fn get_screenshot() -> Option<std::path::PathBuf> {
    let storage_path = get_storage_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")); // Fallback to /tmp if no storage path
    let file_name = storage_path.join(format!("screenshot_{}.png", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    std::process::Command::new("screencapture")
        .arg("-i")
        .arg(&file_name)
        .output()
        .unwrap();

    // check the path if teh file exists
    if file_name.exists() {
        println!("Screenshot saved to: {:?}", file_name);
        Some(file_name)
    } else {
        println!("Screenshot cancelled.");
        None
    }
}

#[cfg(target_os = "windows")]
fn get_screenshot() -> Option<std::path::PathBuf> {
    unimplemented!()
}

fn get_storage_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir("Typst Scan")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture() {
        get_screenshot();
    }

    #[test]
    fn test_get_storage() {
        if let Some(storage_path) = eframe::storage_dir("Typst Scan") {
            println!("Storage path: {:?}", storage_path);
        }
    }
}
