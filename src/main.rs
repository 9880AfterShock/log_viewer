use std::{
    io::{Read, Write}, net::TcpStream, thread,
};

use crate::{app::LogViewer, log_api::LogGrabber};

mod log_api;
mod app;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("Log Viewer", native_options, Box::new(|cc| Ok(Box::new(LogViewer::new(cc))))).expect("failed to run");
}
