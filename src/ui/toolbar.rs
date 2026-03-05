use crate::App;
use eframe::egui;

pub fn show(app: &mut App, ui: &mut egui::Ui, table_name: &str) {
    ui.horizontal(|ui| {
        ui.heading(table_name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                if let Some(conn) = &app.conn {
                    if let Some(view) = &mut app.table_view {
                        view.reload_rows(conn);
                    }
                }
            }
        });
    });
    ui.separator();
}