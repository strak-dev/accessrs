use crate::db::schema::{ColumnDef, ColType};
use eframe::egui;

pub struct CreateTableDialog {
    pub open: bool,
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
    pub error: Option<String>,
}

impl Default for CreateTableDialog {
    fn default() -> Self {
        Self {
            open: false,
            table_name: String::new(),
            columns: vec![
                ColumnDef {
                    name: "id".to_string(),
                    col_type: ColType::Integer,
                    primary_key: true,
                    not_null: true,
                },
                ColumnDef::default(),
            ],
            error: None,
        }
    }
}

impl CreateTableDialog {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn to_sql(&self) -> Result<String, String> {
        if self.table_name.trim().is_empty() {
            return Err("Table name is required.".to_string());
        }

        let mut col_defs = vec!["id INTEGER PRIMARY KEY AUTOINCREMENT".to_string()];

        let rest: Vec<String> = self
            .columns
            .iter()
            .skip(1)
            .filter(|c| !c.name.trim().is_empty())
            .map(|c| {
                let mut def = format!("{} {}", c.name.trim(), c.col_type.sql_type());
                if c.primary_key {
                    def.push_str(" PRIMARY KEY");
                }
                if c.not_null && !c.primary_key {
                    def.push_str(" NOT NULL");
                }
                if let ColType::ForeignKey(ref_table) = &c.col_type {
                    def.push_str(&format!(" REFERENCES {}(id)", ref_table));
                }
                def
            })
            .collect();

        col_defs.extend(rest);

        Ok(format!(
            "CREATE TABLE {} ({})",
            self.table_name.trim(),
            col_defs.join(", ")
        ))
    }

    pub fn show(&mut self, ctx: &egui::Context, tables: &[String]) -> bool {
        if !self.open {
            return false;
        }

        let mut should_create = false;

        egui::Window::new("Create Table")
            .collapsible(false)
            .resizable(true)
            .min_width(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Table name:");
                    ui.text_edit_singleline(&mut self.table_name);
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Column Name").strong());
                    ui.add_space(80.0);
                    ui.label(egui::RichText::new("Type").strong());
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new("PK").strong());
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Not Null").strong());
                });
                ui.separator();

                let mut to_delete: Option<usize> = None;
                for (i, col) in self.columns.iter_mut().enumerate() {
                    let is_id = i == 0;
                    ui.horizontal(|ui| {
                        if is_id {
                            ui.add_enabled(
                                false,
                                egui::TextEdit::singleline(&mut col.name).desired_width(140.0),
                            );
                            ui.add_enabled(
                                false,
                                egui::Button::new("INTEGER  •  AUTOINCREMENT  •  PK"),
                            );
                        } else {
                            ui.add(
                                egui::TextEdit::singleline(&mut col.name).desired_width(140.0),
                            );
                            let type_label = match &col.col_type {
                                ColType::ForeignKey(t) => format!("FK → {}", t),
                                other => other.label().to_string(),
                            };
                            egui::ComboBox::from_id_salt(format!("col_type_{i}"))
                                .selected_text(type_label)
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for t in ColType::base_types() {
                                        ui.selectable_value(
                                            &mut col.col_type,
                                            t.clone(),
                                            t.label(),
                                        );
                                    }
                                    if !tables.is_empty() {
                                        ui.separator();
                                        for table in tables {
                                            let fk = ColType::ForeignKey(table.clone());
                                            let fk_label = format!("FK → {}", table);
                                            ui.selectable_value(
                                                &mut col.col_type,
                                                fk,
                                                fk_label,
                                            );
                                        }
                                    }
                                });
                            ui.checkbox(&mut col.primary_key, "");
                            ui.add_space(16.0);
                            ui.checkbox(&mut col.not_null, "");
                            ui.add_space(16.0);
                            if ui.small_button("x").clicked() {
                                to_delete = Some(i);
                            }
                        }
                    });
                }
                if let Some(i) = to_delete {
                    self.columns.remove(i);
                }
                ui.add_space(4.0);
                if ui.button("+ Add column").clicked() {
                    self.columns.push(ColumnDef::default());
                }
                ui.add_space(8.0);
                ui.separator();
                if let Ok(sql) = self.to_sql() {
                    ui.label(egui::RichText::new(&sql).monospace().weak());
                }
                if let Some(err) = self.error.clone() {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        should_create = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.reset();
                    }
                });
            });
        should_create
    }
}