#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use rfd::FileDialog;
use rusqlite::Connection;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("AccessRs"),
        ..Default::default()
    };

    eframe::run_native(
        "AccessRs",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

struct App {
    conn: Option<Connection>,
    db_path: Option<String>,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            conn: None,
            db_path: None,
            status: "No database open.".to_string(),
        }
    }
}

impl App {
    fn open_db(&mut self, path: std::path::PathBuf) {
        match Connection::open(&path) {
            Ok(conn) => {
                self.db_path = Some(path.display().to_string());
                self.conn = Some(conn);
                self.status = format!("Opened: {}", path.display());
            }
            Err(e) => {
                self.status = format!("Error opening DB: {e}");
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open database…").clicked() {
                        ui.close_menu();
                        if let Some(path) = FileDialog::new()
                            .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                            .pick_file()
                        {
                            self.open_db(path);
                        }
                    }

                    if ui.button("New database…").clicked() {
                        ui.close_menu();
                        if let Some(path) = FileDialog::new()
                            .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                            .set_file_name("new_database.db")
                            .save_file()
                        {
                            // Opening a non-existent path creates the file in SQLite
                            self.open_db(path);
                        }
                    }

                    if self.conn.is_some() {
                        ui.separator();
                        if ui.button("Close database").clicked() {
                            ui.close_menu();
                            self.conn = None;
                            self.db_path = None;
                            self.status = "Database closed.".to_string();
                        }
                    }
                });
            });
        });

        // Status bar at the bottom
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
            });
        });

        // Main area
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.conn.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("No database open");
                        ui.add_space(8.0);
                        if ui.button("Open database…").clicked() {
                            if let Some(path) = FileDialog::new()
                                .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                                .pick_file()
                            {
                                self.open_db(path);
                            }
                        }
                        if ui.button("New database…").clicked() {
                            if let Some(path) = FileDialog::new()
                                .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                                .set_file_name("new_database.db")
                                .save_file()
                            {
                                self.open_db(path);
                            }
                        }
                    });
                });
            } else {
                ui.heading(
                    self.db_path.as_deref().unwrap_or("Unknown")
                );
                ui.label("Database open — table browser coming next.");
            }
        });
    }
}
