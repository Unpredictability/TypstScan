use eframe::{run_native, App};
use std::sync::{mpsc, Arc, Mutex};

mod app;
mod worker;
mod tests;

use app::TypstScan;
use crate::app::TypstScanData;

fn main() {
    // Create a global API key that is shared between app and worker
    let global_app_data: Arc<Mutex<TypstScanData>> = Arc::new(Mutex::new(TypstScanData::default()));

    // Create channels for sending tasks to the worker thread and receiving results
    let (task_sender, task_receiver) = mpsc::channel::<worker::SnipTask>();
    let (result_sender, result_receiver) = mpsc::channel::<worker::TaskResult>();

    worker::start_worker(task_receiver, result_sender, global_app_data.clone()); // need to get api key from app storage here

    let native_options = eframe::NativeOptions::default();
    run_native(
        "Typst Scan",
        native_options,
        Box::new(|cc| Ok(Box::new(TypstScan::new(cc, task_sender, result_receiver, global_app_data)))),
    )
    .unwrap();
}
