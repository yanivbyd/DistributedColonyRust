#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use shared::be_api::ShardLayer;
mod call_be;

const REFRESH_INTERVAL_MS: u64 = 100;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Creatures,
    ExtraFood,
    Sizes,
}

#[derive(Clone)]
pub struct ShardConfig {
    pub total_width: i32,
    pub total_height: i32,
    pub cols: usize,
    pub rows: usize,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            total_width: 1250,
            total_height: 750,
            cols: 5,
            rows: 3,
        }
    }
}

impl ShardConfig {
    fn shard_width(&self) -> i32 {
        self.total_width / self.cols as i32
    }
    
    fn shard_height(&self) -> i32 {
        self.total_height / self.rows as i32
    }
    
    fn total_shards(&self) -> usize {
        self.cols * self.rows
    }
    
    fn get_shard(&self, index: usize) -> shared::be_api::Shard {
        let row = index / self.cols;
        let col = index % self.cols;
        
        let x = col as i32 * self.shard_width();
        let y = row as i32 * self.shard_height();
        
        // Handle the last column and row to account for rounding
        let width = if col == self.cols - 1 {
            self.total_width - (self.cols - 1) as i32 * self.shard_width()
        } else {
            self.shard_width()
        };
        
        let height = if row == self.rows - 1 {
            self.total_height - (self.rows - 1) as i32 * self.shard_height()
        } else {
            self.shard_height()
        };
        
        shared::be_api::Shard { x, y, width, height }
    }
}

struct BEImageApp {
    creatures: Arc<Mutex<Vec<Option<RetainedImage>>>>,
    creatures_color_data: Arc<Mutex<Vec<Option<Vec<shared::be_api::Color>>>>>,
    extra_food: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    sizes: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
    current_tab: Tab,
    shared_current_tab: Arc<Mutex<Tab>>,
    shard_config: Arc<Mutex<ShardConfig>>,
    show_config: bool,
}

impl Default for BEImageApp {
    fn default() -> Self {
        let shard_config = Arc::new(Mutex::new(ShardConfig::default()));
        let total_shards = {
            let config_guard = shard_config.lock().unwrap();
            config_guard.total_shards()
        };
        let creatures = Arc::new(Mutex::new(call_be::get_all_shard_retained_images(&shard_config.lock().unwrap())));
        let creatures_color_data = Arc::new(Mutex::new(call_be::get_all_shard_color_data(&shard_config.lock().unwrap())));
        let extra_food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let sizes = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let current_tab = Tab::Creatures;
        Self {
            creatures,
            creatures_color_data,
            extra_food,
            sizes,
            ctx: None,
            thread_started: false,
            current_tab,
            shared_current_tab: Arc::new(Mutex::new(current_tab)),
            shard_config,
            show_config: false,
        }
    }
}

impl App for BEImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // On the first frame, store ctx and spawn the background thread
        if !self.thread_started {
            self.ctx = Some(ctx.clone());
            let creatures = self.creatures.clone();
            let creatures_color_data = self.creatures_color_data.clone();
            let extra_food = self.extra_food.clone();
            let sizes = self.sizes.clone();
            let ctx_clone = ctx.clone();
            let shared_current_tab = self.shared_current_tab.clone();
            let shard_config = self.shard_config.clone();
            thread::spawn(move || {
                loop {
                    // Look at the selected tab and get only the info required for the current Tab
                    let tab = *shared_current_tab.lock().unwrap();
                    let config = shard_config.lock().unwrap().clone();
                    match tab {
                        Tab::Creatures => {
                            let images = call_be::get_all_shard_retained_images(&config);
                            let color_data = call_be::get_all_shard_color_data(&config);
                            {
                                let mut locked = creatures.lock().unwrap();
                                *locked = images;
                            }
                            {
                                let mut locked = creatures_color_data.lock().unwrap();
                                *locked = color_data;
                            }
                        }
                        Tab::ExtraFood => {
                            let extra_food_data = call_be::get_all_shard_layer_data(ShardLayer::ExtraFood, &config);
                            {
                                let mut locked = extra_food.lock().unwrap();
                                *locked = extra_food_data;
                            }
                        }
                        Tab::Sizes => {
                            let sizes_data = call_be::get_all_shard_layer_data(ShardLayer::CreatureSize, &config);
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
            
            // Configuration button
            {
                let config = self.shard_config.lock().unwrap();
                ui.horizontal(|ui| {
                    if ui.button("⚙️ Config").clicked() {
                        self.show_config = !self.show_config;
                    }
                    ui.label(format!("Shards: {}x{} = {}", config.cols, config.rows, config.total_shards()));
                });
            }
            
            // Configuration panel
            if self.show_config {
                ui.collapsing("Shard Configuration", |ui| {
                    let mut config_changed = false;
                    let mut new_config = self.shard_config.lock().unwrap().clone();
                    
                    ui.horizontal(|ui| {
                        ui.label("Columns:");
                        let mut cols = new_config.cols as i32;
                        if ui.add(egui::DragValue::new(&mut cols).clamp_range(1..=10)).changed() {
                            new_config.cols = cols as usize;
                            config_changed = true;
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Rows:");
                        let mut rows = new_config.rows as i32;
                        if ui.add(egui::DragValue::new(&mut rows).clamp_range(1..=10)).changed() {
                            new_config.rows = rows as usize;
                            config_changed = true;
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Total Width:");
                        let mut width = new_config.total_width;
                        if ui.add(egui::DragValue::new(&mut width).clamp_range(100..=2000)).changed() {
                            new_config.total_width = width;
                            config_changed = true;
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Total Height:");
                        let mut height = new_config.total_height;
                        if ui.add(egui::DragValue::new(&mut height).clamp_range(100..=2000)).changed() {
                            new_config.total_height = height;
                            config_changed = true;
                        }
                    });
                    
                    if config_changed {
                        // Update the shared config
                        {
                            let mut config_guard = self.shard_config.lock().unwrap();
                            *config_guard = new_config;
                        }
                        
                        // Update data structures with new shard count
                        let total_shards = self.shard_config.lock().unwrap().total_shards();
                        self.creatures = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
                        self.creatures_color_data = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
                        self.extra_food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
                        self.sizes = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
                        
                        // Note: The background thread will pick up the new config on the next iteration
                        // since we pass the config by reference
                    }
                });
            }
            
            ui.separator();
            
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
        let locked = self.creatures_color_data.lock().unwrap();
        
        self.show_combined_image(ui, &locked, |shard_data| {
            shard_data.clone()
        });
    }

    fn show_combined_image<T, F>(&self, ui: &mut egui::Ui, data: &[Option<T>], converter: F)
    where
        F: Fn(&Option<T>) -> Option<Vec<shared::be_api::Color>>,
    {
        // Create a combined image using the shard configuration
        let config = self.shard_config.lock().unwrap();
        let total_width = config.total_width as usize;
        let total_height = config.total_height as usize;
        let mut combined_img = egui::ColorImage::new([total_width, total_height], egui::Color32::BLACK);
        
        // Process each shard
        for (idx, shard_data) in data.iter().enumerate() {
            let row = idx / config.cols;
            let col = idx % config.cols;
            
            if let Some(colors) = converter(shard_data) {
                // Calculate shard position and size
                let shard_x = col * config.shard_width() as usize;
                let shard_y = row * config.shard_height() as usize;
                let shard_width = if col == config.cols - 1 {
                    total_width - (config.cols - 1) * config.shard_width() as usize
                } else {
                    config.shard_width() as usize
                };
                let _shard_height = if row == config.rows - 1 {
                    total_height - (config.rows - 1) * config.shard_height() as usize
                } else {
                    config.shard_height() as usize
                };
                
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
