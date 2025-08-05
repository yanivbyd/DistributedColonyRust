#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use shared::be_api::ShardLayer;
mod call_be;

const SHARD_SIZE: f32 = 250.0;
const REFRESH_INTERVAL_MS: u64 = 100;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Creatures,
    ExtraFood,
    Sizes,
}

struct BEImageApp {
    creatures: Arc<Mutex<Vec<Option<RetainedImage>>>>,
    extra_food: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    sizes: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
    current_tab: Tab,
    shared_current_tab: Arc<Mutex<Tab>>,
}

impl Default for BEImageApp {
    fn default() -> Self {
        let creatures = Arc::new(Mutex::new(call_be::get_all_shard_retained_images()));
        let extra_food = Arc::new(Mutex::new(vec![None; 10])); // Placeholder for extra food
        let sizes = Arc::new(Mutex::new(vec![None; 10])); // Placeholder for sizes
        let current_tab = Tab::Creatures;
        Self {
            creatures,
            extra_food,
            sizes,
            ctx: None,
            thread_started: false,
            current_tab,
            shared_current_tab: Arc::new(Mutex::new(current_tab)),
        }
    }
}

impl App for BEImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // On the first frame, store ctx and spawn the background thread
        if !self.thread_started {
            self.ctx = Some(ctx.clone());
            let creatures = self.creatures.clone();
            let extra_food = self.extra_food.clone();
            let sizes = self.sizes.clone();
            let ctx_clone = ctx.clone();
            let shared_current_tab = self.shared_current_tab.clone();
            thread::spawn(move || {
                loop {
                    // Look at the selected tab and get only the info required for the current Tab
                    let tab = *shared_current_tab.lock().unwrap();
                    match tab {
                        Tab::Creatures => {
                            let images = call_be::get_all_shard_retained_images();
                            {
                                let mut locked = creatures.lock().unwrap();
                                *locked = images;
                            }
                        }
                        Tab::ExtraFood => {
                            let extra_food_data = call_be::get_all_shard_layer_data(ShardLayer::ExtraFood);
                            {
                                let mut locked = extra_food.lock().unwrap();
                                *locked = extra_food_data;
                            }
                        }
                        Tab::Sizes => {
                            let sizes_data = call_be::get_all_shard_layer_data(ShardLayer::CreatureSize);
                            {
                                let mut locked = sizes.lock().unwrap();
                                *locked = sizes_data;
                            }
                        }
                    }
                    ctx_clone.request_repaint();
                    thread::sleep(Duration::from_millis(REFRESH_INTERVAL_MS));
                }
            });
            self.thread_started = true;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            
            // Tab control
            ui.horizontal(|ui| {
                let old_tab = self.current_tab;
                ui.selectable_value(&mut self.current_tab, Tab::Creatures, "Creatures");
                ui.selectable_value(&mut self.current_tab, Tab::ExtraFood, "Extra Food");
                ui.selectable_value(&mut self.current_tab, Tab::Sizes, "Sizes");
                
                // Update shared tab if changed
                if self.current_tab != old_tab {
                    if let Ok(mut shared_tab) = self.shared_current_tab.lock() {
                        *shared_tab = self.current_tab;
                    }
                }
            });
            ui.separator();
            
            match self.current_tab {
                Tab::Creatures => self.show_creatures_tab(ui),
                Tab::ExtraFood => self.show_extra_food_tab(ui),
                Tab::Sizes => self.show_sizes_tab(ui),
            }
        });
    }
}

impl BEImageApp {
    fn terrain_color(normalized: f32) -> egui::Color32 {
        fn lerp(a: u8, b: u8, t: f32) -> u8 {
            ((1.0 - t) * (a as f32) + t * (b as f32)).round() as u8
        }
    
        let (r, g, b) = if normalized <= 0.25 {
            // Dark green → Light green
            let t = normalized / 0.25;
            (
                lerp(0x00, 0x7C, t),
                lerp(0x64, 0xAD, t),
                lerp(0x00, 0x18, t),
            )
        } else if normalized <= 0.5 {
            // Light green → Tan
            let t = (normalized - 0.25) / 0.25;
            (
                lerp(0x7C, 0xD2, t),
                lerp(0xAD, 0xB4, t),
                lerp(0x18, 0x8C, t),
            )
        } else if normalized <= 0.75 {
            // Tan → Brown
            let t = (normalized - 0.5) / 0.25;
            (
                lerp(0xD2, 0x8B, t),
                lerp(0xB4, 0x45, t),
                lerp(0x8C, 0x13, t),
            )
        } else {
            // Brown → Gray 
            let t = (normalized - 0.75) / 0.25;
            (
                lerp(0x8B, 0xA9, t),
                lerp(0x45, 0xA9, t),
                lerp(0x13, 0xA9, t),
            )
        };
    
        egui::Color32::from_rgb(r, g, b)
    }
    
    fn show_creatures_tab(&self, ui: &mut egui::Ui) {
        let locked = self.creatures.lock().unwrap();
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
    }

    fn show_combined_image<T, F>(&self, ui: &mut egui::Ui, data: &[Option<T>], converter: F)
    where
        F: Fn(&Option<T>) -> Option<Vec<shared::be_api::Color>>,
    {
        // Create a combined image of 1250x750 pixels (total size)
        let total_width = 1250;
        let total_height = 750;
        let mut combined_img = egui::ColorImage::new([total_width, total_height], egui::Color32::BLACK);
        
        // Shard dimensions
        let fifth = total_width / 5;
        let third = total_height / 3;
        
        // Process each shard
        for (idx, shard_data) in data.iter().enumerate() {
            let row = idx / 5;
            let col = idx % 5;
            
            if let Some(colors) = converter(shard_data) {
                // Calculate shard position and size
                let shard_x = col * fifth;
                let shard_y = row * third;
                let shard_width = if col == 4 { total_width - 4 * fifth } else { fifth };
                let _shard_height = if row == 2 { total_height - 2 * third } else { third };
                
                // Copy shard data to combined image
                for (i, color) in colors.iter().enumerate() {
                    let local_x = i % shard_width;
                    let local_y = i / shard_width;
                    let global_x = shard_x + local_x;
                    let global_y = shard_y + local_y;
                    
                    if global_x < total_width && global_y < total_height {
                        let pixel_idx = global_y * total_width + global_x;
                        combined_img.pixels[pixel_idx] = egui::Color32::from_rgb(color.red, color.green, color.blue);
                    }
                }
            }
        }
        
        // Display the combined image
        let retained_image = egui_extras::RetainedImage::from_color_image("combined", combined_img);
        retained_image.show_max_size(ui, egui::vec2(800.0, 600.0));
    }

    fn show_layer_tab(&self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>) {
        let locked = data.lock().unwrap();
        
        // Find global maximum across all shards for consistent normalization
        let global_max = locked.iter()
            .filter_map(|shard_data| shard_data.as_ref())
            .flat_map(|data| data.iter())
            .max()
            .copied()
            .unwrap_or(0);
        
        self.show_combined_image(ui, &locked, |shard_data| {
            if let Some(data) = shard_data {
                if global_max > 0 {
                    // Convert i32 data to colors using global normalization
                    let mut colors = Vec::new();
                    for &val in data {
                        if val == 0 {
                            // Show white for zero values
                            colors.push(shared::be_api::Color { red: 255, green: 255, blue: 255, });
                        } else {
                            let normalized = val as f32 / global_max as f32;
                            let color = Self::terrain_color(normalized);
                            colors.push(shared::be_api::Color { red: color.r(), green: color.g(), blue: color.b(), });
                        }
                    }
                    Some(colors)
                } else {
                    // All values are 0, use white
                    let mut colors = Vec::new();
                    for _ in 0..data.len() {
                        colors.push(shared::be_api::Color {
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }
                    Some(colors)
                }
            } else {
                None
            }
        });
    }

    fn show_extra_food_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab(ui, &self.extra_food);
    }

    fn show_sizes_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab(ui, &self.sizes);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Colony Viewer",
        options,
        Box::new(|_cc| Box::new(BEImageApp::default())),
    )
}
