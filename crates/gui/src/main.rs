#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
mod call_be;

struct BEImageApp {
    retained: Arc<Mutex<Option<RetainedImage>>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
}

impl Default for BEImageApp {
    fn default() -> Self {
        let retained = Arc::new(Mutex::new(call_be::get_colony_retained_image()));
        Self {
            retained,
            ctx: None,
            thread_started: false,
        }
    }
}

impl App for BEImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // On the first frame, store ctx and spawn the background thread
        if !self.thread_started {
            self.ctx = Some(ctx.clone());
            let retained = self.retained.clone();
            let ctx_clone = ctx.clone();
            thread::spawn(move || {
                loop {
                    let img = call_be::get_colony_retained_image();
                    {
                        let mut locked = retained.lock().unwrap();
                        *locked = img;
                    }
                    ctx_clone.request_repaint();
                    thread::sleep(Duration::from_millis(100));
                }
            });
            self.thread_started = true;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Distributed Colony");
            ui.separator();
            let locked = self.retained.lock().unwrap();
            if let Some(img) = &*locked {
                img.show(ui);
            } else {
                ui.colored_label(egui::Color32::RED, "Failed to fetch image from backend");
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Distributed Colony",
        options,
        Box::new(|_cc| Box::new(BEImageApp::default())),
    )
}
