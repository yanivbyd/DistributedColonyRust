#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use shared::be_api::ShardLayer;
mod call_be;

const REFRESH_INTERVAL_MS: u64 = 100;
const WIDTH_IN_SHARDS: i32 = 5;
const HEIGHT_IN_SHARDS: i32 = 3;
const SHARD_WIDTH: i32 = 250;
const SHARD_HEIGHT: i32 = 250;

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
            total_width: WIDTH_IN_SHARDS * SHARD_WIDTH,
            total_height: HEIGHT_IN_SHARDS * SHARD_HEIGHT,
            cols: WIDTH_IN_SHARDS as usize,
            rows: HEIGHT_IN_SHARDS as usize,
        }
    }
}

impl ShardConfig {
    fn shard_width(&self) -> i32 {
        SHARD_WIDTH
    }
    
    fn shard_height(&self) -> i32 {
        SHARD_HEIGHT
    }
    
    fn total_shards(&self) -> usize {
        self.cols * self.rows
    }
    
    fn get_shard(&self, index: usize) -> shared::be_api::Shard {
        let row = index / self.cols;
        let col = index % self.cols;
        
        let x = col as i32 * SHARD_WIDTH;
        let y = row as i32 * SHARD_HEIGHT;
        
        shared::be_api::Shard { 
            x, 
            y, 
            width: SHARD_WIDTH, 
            height: SHARD_HEIGHT 
        }
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
    fn lerp(a: u8, b: u8, t: f32) -> u8 {
        ((1.0 - t) * (a as f32) + t * (b as f32)).round() as u8
    }

    fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
        (
            Self::lerp(a.0, b.0, t),
            Self::lerp(a.1, b.1, t),
            Self::lerp(a.2, b.2, t),
        )
    }

    fn terrain_color(normalized: f32) -> egui::Color32 {
        // From top to bottom in the image
        let palette = [
            (0, 102, 0),      // Dark Green
            (0, 204, 0),      // Green
            (153, 255, 102),  // Light Green
            (255, 255, 128),  // Yellow
            (222, 184, 135),  // Tan
            (204, 51, 0),     // Red
            (102, 51, 0),     // Dark Brown
        ];

        let clamped = normalized.clamp(0.0, 1.0);
        let scaled = clamped * (palette.len() - 1) as f32;
        let idx = scaled.floor() as usize;
        let t = scaled.fract();

        let (r, g, b) = if idx >= palette.len() - 1 {
            palette[palette.len() - 1]
        } else {
            Self::lerp_rgb(palette[idx], palette[idx + 1], t)
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
        
        // Add legend below the image
        if global_max > 0 {
            ui.add_space(20.0);
            
            let legend_width = 800.0;
            let legend_height = 5.0;
            let legend_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(legend_width, legend_height)
            );
            
            let painter = ui.painter();
            
            // Draw color gradient
            for i in 0..legend_width as usize {
                let normalized = i as f32 / legend_width;
                let color = Self::terrain_color(normalized);
                let x = legend_rect.min.x + i as f32;
                painter.line_segment(
                    [egui::pos2(x, legend_rect.min.y), egui::pos2(x, legend_rect.max.y)],
                    egui::Stroke::new(1.0, color)
                );
            }
            
            // Add labels
            ui.add_space(legend_height + 5.0);
            ui.horizontal(|ui| {
                ui.label(format!("0"));
                ui.add_space(legend_width / 2.0 - 30.0);
                ui.label(format!("{}", global_max / 2));
                ui.add_space(legend_width / 2.0 - 30.0);
                ui.label(format!("{}", global_max));
            });
        }
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
