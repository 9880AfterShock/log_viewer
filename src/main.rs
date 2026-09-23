use std::{
    io::{Read, Write}, net::TcpStream, thread,
};

use crate::{app::LogViewer, log_api::LogGrabber};

#[cfg(any(target_arch = "wasm64",target_arch = "wasm32"))]
mod wasm_stuff;

mod log_api;
mod app;
#[cfg(not(any(target_arch = "wasm64",target_arch = "wasm32")))]
fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("Log Viewer", native_options, Box::new(|cc| Ok(Box::new(LogViewer::new(cc))))).expect("failed to run");
}

#[cfg(any(target_arch = "wasm64",target_arch = "wasm32"))]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    
        // Redirect `log` message to `console.log` and friends:
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    
        let web_options = eframe::WebOptions::default();
    
        wasm_bindgen_futures::spawn_local(async {
            let document = web_sys::window()
                .expect("No window")
                .document()
                .expect("No document");
    
            let canvas = document
                .get_element_by_id("the_canvas_id")
                .expect("Failed to find the_canvas_id")
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("the_canvas_id was not a HtmlCanvasElement");
    
            let start_result = eframe::WebRunner::new()
                .start(
                    canvas,
                    web_options,
                    Box::new(|cc| Ok(Box::new(LogViewer::new(cc)))),
                )
                .await;
    
            // Remove the loading text and spinner:
            if let Some(loading_text) = document.get_element_by_id("loading_text") {
                match start_result {
                    Ok(()) => {
                        loading_text.remove();
                    }
                    Err(err) => {
                        loading_text.set_inner_html(
                            "<p> The app has crashed. See the developer console for details. </p>",
                        );
                        panic!("Failed to start eframe: {err:?}");
                    }
                }
            }
        });
}
