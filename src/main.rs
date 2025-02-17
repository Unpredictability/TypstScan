use eframe::{run_native, App};
use std::sync::{mpsc, Arc, Mutex};

mod app;
mod worker;
mod tests;

use app::TypstScan;

fn main() {
    // Create a global API key that is shared between app and worker
    let global_api_key: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    // Create channels for sending tasks to the worker thread and receiving results
    let (task_sender, task_receiver) = mpsc::channel::<worker::SnipTask>();
    let (result_sender, result_receiver) = mpsc::channel::<worker::TaskResult>();

    worker::start_worker(task_receiver, result_sender, global_api_key.clone()); // need to get api key from app storage here

    let native_options = eframe::NativeOptions::default();
    run_native(
        "Typst Scan",
        native_options,
        Box::new(|cc| Ok(Box::new(TypstScan::new(cc, task_sender, result_receiver, global_api_key)))),
    )
    .unwrap();
}
