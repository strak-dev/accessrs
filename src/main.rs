#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewPortBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("AccessRs")
        ..Default::default()
    };

    eframe::run_native(
        "AccessRs",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

struct App {
    
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("AccessRS");
            ui.label("Hello, world.");
        });
    }
}
