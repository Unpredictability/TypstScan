use eframe::egui::mutex::Mutex;
use eframe::egui::{Color32, Image, Pos2, Stroke, UserData, ViewportCommand, Widget};
use eframe::{egui, App};
use egui_extras;
use egui_extras::Column;
use image::buffer::ConvertBuffer;
use image::{DynamicImage, ImageFormat, RgbImage, Rgba, RgbaImage};
use mouse_position;
use reqwest::blocking::multipart;
use reqwest::blocking::multipart::Part;
use reqwest::blocking::Client;
use reqwest::header;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
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

pub struct TypstScan {
    data: TypstScanData,
    overlay: Arc<Mutex<bool>>,
    screenshot_start: Option<(i32, i32)>,
    screenshot_end: Option<(i32, i32)>,
    result_queue: Arc<Mutex<Vec<MathpixResult>>>,
    image: Option<(Arc<egui::ColorImage>, egui::TextureHandle)>,
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

impl TypstScan {
    pub fn new(cc: &eframe::CreationContext<'_>, shared_flag: Arc<Mutex<bool>>) -> Self {
        if let Some(storage) = cc.storage {
            let typst_scan_data: TypstScanData = eframe::get_value(storage, "typst_scan_data").unwrap_or_default();
            return Self {
                data: typst_scan_data,
                overlay: shared_flag,
                screenshot_start: None,
                screenshot_end: None,
                result_queue: Arc::new(Mutex::new(Vec::new())),
                image: None,
            };
        }

        Self {
            data: TypstScanData::default(),
            overlay: shared_flag,
            screenshot_start: None,
            screenshot_end: None,
            result_queue: Arc::new(Mutex::new(Vec::new())),
            image: None,
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

        // check if there is any new result
        let mut result_queue = self.result_queue.lock();
        if !result_queue.is_empty() {
            let result = result_queue.pop().unwrap();
            println!("Result: {:?}", result);
            // add to the data.snip_items
            self.data.snip_items.push(SnipItem {
                id: Uuid::new_v4(),
                title: result.title,
                image_url: result.images.original.fullsize.url.clone(),
                tex: result.text.clone(),
                typst: text_and_tex2typst(&result.text),
            });
        }

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
                                            // if label.double_clicked() {
                                            //     self.snip_items.retain(|item| item.id != snip_item.id);
                                            // }
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
                                // show image based on the image url
                                ui.vertical_centered(|ui| {
                                    ui.add(egui::Image::from_uri(&snip_item.image_url).max_height(250.0));
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
            MainView::ReplaceRules => {
            }
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

                            ui.label("test stuff");
                            if ui.button("Add Snip Item").clicked() {
                                self.data.snip_items.push(SnipItem {
                                    id: Uuid::new_v4(),
                                    title: "New Snip Item asdasd asd as das das ad as asd as das das".to_string(),
                                    image_url: "https://picsum.photos/400/300".to_string(),
                                    tex: "test tex sada asdasd  efafadasf af as fasdas efaf asf asf a".to_string(),
                                    typst: "test typst adhuah uifa bas fash ash uihas fhasu hfau".to_string(),
                                });
                            }
                            ui.end_row();

                            ui.label("delete all");
                            if ui.button("delete all").clicked() {
                                self.data.snip_items.clear();
                            }
                            ui.end_row();

                            ui.label("API usage");
                            ui.add(egui::ProgressBar::new(0.618).show_percentage());
                            ui.end_row();

                            ui.checkbox(&mut self.overlay.lock(), "Shared Flag");
                            ui.end_row();

                            ui.label("Mouse Position");
                            ui.label(format!("{:?}", get_mouse_position()));
                        });
                });
            }
        });

        // start the screenshot
        if *self.overlay.lock() {
            let viewport_id = egui::ViewportId::from_hash_of("screenshot_viewport");
            let viewport_builder = egui::ViewportBuilder::default()
                .with_title("Screenshot Viewport")
                .with_resizable(false)
                .with_always_on_top()
                .with_maximized(true)
                .with_decorations(false)
                .with_transparent(true);
            ctx.show_viewport_immediate(viewport_id, viewport_builder, |ctx, class| {
                egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                    if let Some(mouse_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                        ui.label(format!("Mouse position: {:?}", mouse_pos));
                        let available_rect = ui.max_rect();
                        let painter = ui.painter();
                        // draw a cross-hair at the mouse position
                        painter.line_segment(
                            [
                                Pos2::new(available_rect.min.x, mouse_pos.y),
                                Pos2::new(available_rect.max.x, mouse_pos.y),
                            ],
                            Stroke::new(1.0, Color32::GRAY),
                        );
                        painter.line_segment(
                            [
                                Pos2::new(mouse_pos.x, available_rect.min.y),
                                Pos2::new(mouse_pos.x, available_rect.max.y),
                            ],
                            Stroke::new(1.0, Color32::GRAY),
                        );
                    } else {
                        ui.label("Mouse is not over the UI.");
                    }

                    if ctx.input(|i| i.pointer.any_click()) {
                        if self.screenshot_start.is_none() {
                            self.screenshot_start = get_mouse_position();
                        } else {
                            self.screenshot_end = get_mouse_position();
                            println!("Screenshot start: {:?}, end: {:?}", self.screenshot_start, self.screenshot_end);
                            let cropped_img = get_cropped_screenshot(self.screenshot_start.unwrap(), self.screenshot_end.unwrap());
                            let mut jpeg_data: Vec<u8> = Vec::new();
                            cropped_img
                                .write_to(&mut std::io::Cursor::new(&mut jpeg_data), ImageFormat::Png)
                                .expect("Failed to write cropped image to JPEG format");
                            let mathpix_api_key = self.data.mathpix_api_key.clone();
                            let result_queue = self.result_queue.clone();
                            thread::spawn(move || {
                                if let Err(e) = get_mathpix_result(&mathpix_api_key, jpeg_data, result_queue) {
                                    eprintln!("Error: {:?}", e);
                                }
                            });
                            *self.overlay.lock() = false;
                            self.screenshot_start = None;
                            self.screenshot_end = None;
                        }
                    }

                    if let Some(drag_start) = self.screenshot_start {
                        ui.label(format!("Drag start: {:?}", drag_start));
                    }

                    if let Some(drag_end) = self.screenshot_end {
                        ui.label(format!("Drag end: {:?}", drag_end));
                    }
                });
            });
        }
        // put into continuous mode (so that hotkey still work when the app is not in focus)
        ctx.request_repaint();
    }

    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "typst_scan_data", &self.data);
    }
}

fn get_cropped_screenshot(start_pos: (i32, i32), end_pos: (i32, i32)) -> DynamicImage {
    let monitor = xcap::Monitor::from_point(start_pos.0, start_pos.1).expect("Failed to get monitor");
    let image: RgbaImage = monitor.capture_image().expect("Failed to capture monitor");
    let image: RgbImage = image.convert();
    println!("Image size: {:?}", image.dimensions());
    let monitor_scale_factor = monitor.scale_factor();
    println!("monitor scale factor: {:?}", monitor_scale_factor);
    println!("monitor width and height: {:?}, {:?}", monitor.width(), monitor.height());
    let image = DynamicImage::ImageRgb8(image);
    image.crop_imm(
        (monitor_scale_factor * (start_pos.0 - monitor.x()) as f32) as u32,
        (monitor_scale_factor * (start_pos.1 - monitor.y()) as f32) as u32,
        (monitor_scale_factor * (end_pos.0 - start_pos.0) as f32) as u32,
        (monitor_scale_factor * (end_pos.1 - start_pos.1) as f32) as u32,
    )
}

fn get_mouse_position() -> Option<(i32, i32)> {
    let position = mouse_position::mouse_position::Mouse::get_mouse_position();
    match position {
        mouse_position::mouse_position::Mouse::Position { x, y } => Some((x, y)),
        mouse_position::mouse_position::Mouse::Error => None,
    }
}

fn get_mathpix_result(
    api_key: &str,
    screenshot_data: Vec<u8>,
    result_queue: Arc<Mutex<Vec<MathpixResult>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up headers
    let mut headers = header::HeaderMap::new();
    headers.insert("Authorization", header::HeaderValue::from_str(&format!("Bearer {}", api_key))?);
    headers.insert("Accept", header::HeaderValue::from_static("*/*"));
    headers.insert(
        "User-Agent",
        header::HeaderValue::from_static("Mathpix Snip MacOS App v3.4.11(3411.2)"),
    );

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
    let client = Client::new();
    let form = multipart::Form::new()
        .part("file", Part::bytes(screenshot_data).file_name("image.png").mime_str("image/png")?)
        .part(
            "options_json",
            Part::text(options_payload.to_string()).mime_str("application/json")?,
        );

    let response = client
        .post("https://snip-api.mathpix.com/v1/snips-multipart")
        .headers(headers)
        .multipart(form)
        .send()?;

    // print response raw text
    // println!("Response: {:?}", response.text()?);
    // return Ok(());

    // handle response
    let mathpix_result: MathpixResult = response.json::<MathpixResult>()?;
    // push to result queue
    let mut result_queue = result_queue.lock();
    result_queue.push(mathpix_result);
    println!("Result queue: {:?}", result_queue);

    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SnipItem {
    id: Uuid,
    title: String,
    image_url: String,
    tex: String,
    typst: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ReplaceRule {
    pattern: String,
    replacement: String,
}

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
