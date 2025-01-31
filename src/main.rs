use std::sync::Arc;
use eframe::{egui, run_native, App};
use eframe::egui::mutex::Mutex;

mod app;

use app::TypstScan;
use livesplit_hotkey::{Hook, Hotkey, KeyCode, Modifiers};

fn main() -> eframe::Result {
    // create a shared state for hotkey callback and the eframe app
    let shared_flag = Arc::new(Mutex::new(false));
    // Create a new hotkey hook
    let hook = Hook::new().expect("Failed to create hotkey hook");

    // Define the hotkey: Command + Shift + S
    let hotkey = Hotkey {
        key_code: KeyCode::KeyZ,
        modifiers: Modifiers::CONTROL | Modifiers::ALT,
    };

    // Register the hotkey with its associated action
    let shared_flag_clone = shared_flag.clone();
    hook.register(hotkey, move || {
        println!("Hotkey pressed!");
        let mut shared_flag = shared_flag_clone.lock();
        *shared_flag = !*shared_flag;
    }).expect("Failed to register hotkey");

    // Run the event loop to listen for hotkey events
    // hook.run().expect("Failed to run hotkey hook");


    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport.transparent = Some(true);
    run_native("Typst Scan", native_options, Box::new(|cc| Ok(Box::new(TypstScan::new(cc, shared_flag)))))
}
