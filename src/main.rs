use eframe::egui::mutex::Mutex;
use eframe::{run_native, App};
use std::sync::Arc;

mod app;

use app::TypstScan;
use livesplit_hotkey::{Hook, Hotkey, KeyCode, Modifiers};

fn main() -> eframe::Result {
    // create a shared state for hotkey callback and the eframe app
    let hotkey_flag = Arc::new(Mutex::new(false));
    // Create a new hotkey hook
    let hook = Hook::new().expect("Failed to create hotkey hook");

    // Define the hotkey: Command + Shift + S
    let hotkey = Hotkey {
        key_code: KeyCode::KeyZ,
        modifiers: Modifiers::CONTROL | Modifiers::ALT,
    };

    // Register the hotkey with its associated action
    let hotkey_flag_clone = hotkey_flag.clone();
    hook.register(hotkey, move || {
        println!("Hotkey pressed!");
        let mut hotkey_flag = hotkey_flag_clone.lock();
        *hotkey_flag = !*hotkey_flag;
    })
    .expect("Failed to register hotkey");

    let native_options = eframe::NativeOptions::default();
    run_native(
        "Typst Scan",
        native_options,
        Box::new(|cc| Ok(Box::new(TypstScan::new(cc, hotkey_flag)))),
    )
}
