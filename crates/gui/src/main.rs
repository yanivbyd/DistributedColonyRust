#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
mod call_be;

const SHARD_SIZE: f32 = 250.0;

struct BEImageApp {
    retained: Arc<Mutex<Vec<Option<RetainedImage>>>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
}

impl Default for BEImageApp {
    fn default() -> Self {
        let retained = Arc::new(Mutex::new(call_be::get_all_shard_retained_images()));
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
                    let images = call_be::get_all_shard_retained_images();
                    {
                        let mut locked = retained.lock().unwrap();
                        *locked = images;
                    }
                    ctx_clone.request_repaint();
                    thread::sleep(Duration::from_millis(1000));
                }
            });
            self.thread_started = true;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Distributed Colony");
            ui.separator();
            let locked = self.retained.lock().unwrap();
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                for row in 0..3 {
                    ui.horizontal(|ui| {
                        for col in 0..5 {
                            let idx = row * 5 + col;
                            if let Some(img) = locked.get(idx).and_then(|o| o.as_ref()) {
                                img.show_max_size(ui, egui::vec2(SHARD_SIZE, SHARD_SIZE));
                            } else {
                                ui.allocate_ui(egui::vec2(SHARD_SIZE, SHARD_SIZE), |ui| {
                                    ui.centered_and_justified(|ui| {
                                        ui.colored_label(egui::Color32::RED, "Failed");
                                    });
                                });
                            }
                        }
                    });
                }
            });
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
