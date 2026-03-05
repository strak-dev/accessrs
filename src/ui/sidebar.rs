use crate::App;
use eframe::egui;

pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.conn.is_none() {
        return;
    }

    egui::SidePanel::left("table_list")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Tables");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("r").clicked() {
                        app.refresh_tables();
                    }
                    if ui.small_button("+").clicked() {
                        app.create_dialog.open = true;
                    }
                });
            });
            ui.separator();
            if app.tables.is_empty() {
                ui.label("No tables yet.");
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let tables = app.tables.clone();
                    for table in &tables {
                        let selected = app.selected_table.as_deref() == Some(table.as_str());
                        if ui.selectable_label(selected, table).clicked() {
                            app.select_table(table);
                        }
                    }
                });
            }
        });
}