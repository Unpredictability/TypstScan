use eframe::{run_native, App};
use std::sync::{mpsc, Arc, Mutex};

mod app;
mod worker;

use app::TypstScan;
use livesplit_hotkey::{Hook, Hotkey, KeyCode, Modifiers};

fn main() {
    // Create a global API key that is shared between app and worker
    let global_api_key: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    // Create channels for sending tasks to the worker thread and receiving results
    let (task_sender, task_receiver) = mpsc::channel::<worker::SnipTask>();
    let (result_sender, result_receiver) = mpsc::channel::<worker::TaskResult>();

    let worker_thread = worker::start_worker(task_receiver, result_sender, global_api_key.clone()); // need to get api key from app storage here

    // Create a new hotkey hook
    let hook = Hook::new().expect("Failed to create hotkey hook");
    // Define the hotkey
    let hotkey = Hotkey {
        key_code: KeyCode::KeyZ,
        modifiers: Modifiers::CONTROL | Modifiers::ALT,
    };

    let task_sender_clone = task_sender.clone();
    hook.register(hotkey, move || {
        println!("Hotkey pressed!");
        task_sender_clone.send(worker::SnipTask::new()).unwrap()
    })
    .expect("Failed to register hotkey");


    let native_options = eframe::NativeOptions::default();
    run_native(
        "Typst Scan",
        native_options,
        Box::new(|cc| Ok(Box::new(TypstScan::new(cc, task_sender, result_receiver, global_api_key)))),
    ).unwrap();

    // Wait for the worker thread to finish
    worker_thread.join().unwrap();
}
