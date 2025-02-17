use crate::app::{ClipboardMode, TypstScanData};
use arboard::Clipboard;
use reqwest::blocking::multipart::Part;
use reqwest::blocking::{multipart, Client};
use reqwest::header;
use serde::Deserialize;
use serde_json::json;
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use tex2typst_rs::text_and_tex2typst;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use screen_snip;

pub fn start_worker(
    task_receiver: Receiver<SnipTask>,
    result_sender: Sender<TaskResult>,
    app_data: Arc<Mutex<TypstScanData>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
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
            if let Ok(app_data) = app_data.lock() {
                if app_data.bring_forward {
                    #[cfg(target_os = "macos")]
                    {
                        let process_name = app_data.target_process_name.clone();
                        let window_name = app_data.target_window_title.clone();
                        let script = format!(
                            r#"
                            tell application "System Events"
                                tell process "{process_name}"
                                    set frontmost to true
                                end tell
                            end tell
                        "#
                        );

                        let out = Command::new("osascript").arg("-e").arg(script).output().unwrap();
                        println!("{:?}", out);
                    }

                    #[cfg(target_os = "windows")]
                    {
                        unimplemented!()
                    }
                }
            }

            let mut headers = header::HeaderMap::new();
            headers.insert(
                "Authorization",
                header::HeaderValue::from_str(&format!("Bearer {}", app_data.lock().unwrap().mathpix_api_key)).unwrap(),
            );
            headers.insert("Accept", header::HeaderValue::from_static("*/*"));
            headers.insert(
                "User-Agent",
                header::HeaderValue::from_static("Mathpix Snip MacOS App v3.4.11(3411.2)"),
            );

            if let Some(screenshot_path) = get_screenshot() {
                let screenshot_data = std::fs::read(&screenshot_path).expect("Failed to read screenshot file");
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
                        let typst = text_and_tex2typst(&mathpix_result.text).unwrap_or_else(|e| format!("Error: {:?}", e));
                        let mut typst_replaced = typst.clone();
                        if let Ok(app_data) = app_data.lock() {
                            for rule in app_data.replace_rules.iter() {
                                typst_replaced = typst_replaced.replace(&rule.pattern, &rule.replacement);
                            }

                            match app_data.clipboard_mode {
                                ClipboardMode::Continuous => {
                                    // do nothing, let the UI thread handle it
                                }
                                ClipboardMode::CopyTeX => {
                                    Clipboard::new().unwrap().set_text(mathpix_result.text.clone()).unwrap();
                                }
                                ClipboardMode::CopyTypst => {
                                    Clipboard::new().unwrap().set_text(typst_replaced.clone()).unwrap();
                                }
                            }
                        }
                        result_sender
                            .send(TaskResult {
                                id: snip_task.id,
                                local_image: screenshot_path.to_string_lossy().to_string(),
                                original_image: mathpix_result.images.original.fullsize.url.clone(),
                                rendered_image: mathpix_result.images.rendered.fullsize.url.clone(),
                                text: mathpix_result.text.clone(),
                                latex: mathpix_result.latex.clone(),
                                typst: typst_replaced,
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
            } else {
                continue;
            }
        }
    })
}

pub(crate) struct SnipTask {
    id: Uuid,
}

impl SnipTask {
    pub(crate) fn new() -> Self {
        SnipTask { id: Uuid::new_v4() }
    }
}

#[derive(Debug)]
pub struct TaskResult {
    pub id: Uuid,
    pub local_image: String,
    pub original_image: String,
    pub rendered_image: String,
    pub text: String,
    pub latex: Option<String>,
    pub typst: String,
    pub title: String,
    pub snip_count: u64,
    pub snip_limit: u64,
}

// The following is the struct for the Mathpix API response
#[derive(Debug, Deserialize)]
pub struct MathpixResult {
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
    let storage_path = get_storage_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")); // Fallback to /tmp if no storage path
    let file_name = storage_path.join(format!("screenshot_{}.png", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    screen_snip::get_screen_snip(file_name.clone().into());
    Some(file_name)
}

fn get_storage_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir("Typst Scan")
}
