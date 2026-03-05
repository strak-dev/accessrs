use crate::App;
use crate::db::schema::SortDir;
use crate::ui::popover::{CellPopover, PopoverMode};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub struct GridActions {
    pub do_commit_edit: bool,
    pub do_cancel_edit: bool,
    pub new_editing_cell: Option<Option<(usize, usize)>>,
    pub new_edit_buffer: Option<String>,
    pub do_commit_insert: bool,
    pub new_row_updates: Vec<(usize, String)>,
    pub new_popover: Option<CellPopover>,
    pub sort_click: Option<usize>,
    pub navigate_to: Option<(String, String)>,
}

impl Default for GridActions {
    fn default() -> Self {
        Self {
            do_commit_edit: false,
            do_cancel_edit: false,
            new_editing_cell: None,
            new_edit_buffer: None,
            do_commit_insert: false,
            new_row_updates: vec![],
            new_popover: None,
            sort_click: None,
            navigate_to: None,
        }
    }
}

pub fn show(app: &App, ui: &mut egui::Ui, actions: &mut GridActions) {
    let view = match &app.table_view {
        Some(v) => v,
        None => return,
    };

    let columns = view.columns.clone();
    let rows = view.rows.clone();
    let new_row = view.new_row.clone();
    let editing_cell = view.editing_cell;
    let sort_col = view.sort_col;
    let sort_dir = view.sort_dir.clone();
    let date_columns = view.date_columns.clone();
    let note_columns = view.note_columns.clone();
    let foreign_keys = view.foreign_keys.clone();
    let highlighted_row = view.highlighted_row;
    let edit_buffer = view.edit_buffer.clone();

    egui::ScrollArea::both().show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(36.0))
            .columns(Column::auto().resizable(true).clip(true), columns.len())
            .header(24.0, |mut header| {
                header.col(|ui| {
                    ui.weak("#");
                });
                for (col_idx, col_name) in columns.iter().enumerate() {
                    header.col(|ui| {
                        let arrow = match sort_col {
                            Some(i) if i == col_idx => {
                                if sort_dir == SortDir::Asc { " ^" } else { " v" }
                            }
                            _ => "",
                        };
                        let label = format!("{}{}", col_name, arrow);
                        if ui.button(egui::RichText::new(label).strong()).clicked() {
                            actions.sort_click = Some(col_idx);
                        }
                    });
                }
            })
            .body(|body| {
                let total_rows = rows.len() + 1;
                body.rows(24.0, total_rows, |mut row| {
                    let row_idx = row.index();

                    // ── Insert strip ──────────────────────────────────────
                    if row_idx == rows.len() {
                        row.col(|ui| { ui.weak("*"); });
                        for (col_idx, val) in new_row.iter().enumerate() {
                            row.col(|ui| {
                                let mut buf = val.clone();
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut buf)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("…"),
                                );
                                if buf != *val {
                                    actions.new_row_updates.push((col_idx, buf));
                                }
                                if response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    actions.do_commit_insert = true;
                                }
                            });
                        }
                        return;
                    }

                    // ── Existing rows ─────────────────────────────────────
                    row.col(|ui| {
                        ui.weak((row_idx + 1).to_string());
                    });

                    for (col_idx, cell) in rows[row_idx].iter().enumerate() {
                        row.col(|ui| {
                            let is_editing = editing_cell == Some((row_idx, col_idx));

                            if is_editing {
                                let mut buf = match &actions.new_edit_buffer {
                                    Some(b) => b.clone(),
                                    None => edit_buffer.clone(),
                                };

                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut buf)
                                        .desired_width(f32::INFINITY),
                                );
                                response.request_focus();
                                actions.new_edit_buffer = Some(buf);

                                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

                                if enter {
                                    actions.do_commit_edit = true;
                                } else if escape {
                                    actions.do_cancel_edit = true;
                                } else if response.lost_focus() {
                                    actions.do_commit_edit = true;
                                }
                            } else {
                                // ── Display cell ──────────────────────────
                                let is_long = cell.len() > 60;
                                let is_fk = foreign_keys.iter().find(|fk| fk.col_idx == col_idx);
                                let is_highlighted = highlighted_row == Some(row_idx);
                                let is_note = note_columns.contains(&col_idx);

                                let display = if is_note {
                                    let first_line = cell.lines().next().unwrap_or("").trim();
                                    if first_line.is_empty() {
                                        " Empty note".to_string()
                                    } else if first_line.len() > 50 {
                                        format!(" {}…", &first_line[..50])
                                    } else {
                                        format!(" {}", first_line)
                                    }
                                } else if is_long {
                                    format!("{}…", &cell[..60])
                                } else {
                                    cell.clone()
                                };

                                if is_highlighted {
                                    ui.painter().rect_filled(
                                        ui.available_rect_before_wrap(),
                                        0.0,
                                        egui::Color32::from_rgba_unmultiplied(255, 255, 0, 15),
                                    );
                                }

                                let response = if let Some(fk) = is_fk {
                                    let link = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("-> {}", display))
                                                .color(egui::Color32::from_rgb(100, 160, 255))
                                                .underline(),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if link.clicked() {
                                        actions.navigate_to = Some((fk.ref_table.clone(), cell.clone()));
                                    }
                                    link
                                } else {
                                    ui.label(&display)
                                };

                                let rect_min = response.rect.min;
                                let double_clicked = response.double_clicked();

                                if double_clicked {
                                    let is_date = date_columns.contains(&col_idx);
                                    if is_note {
                                        actions.new_popover = Some(CellPopover {
                                            open: true,
                                            row_idx,
                                            col_idx,
                                            buffer: cell.clone(),
                                            pos: rect_min,
                                            mode: PopoverMode::Note { editing: false },
                                        });
                                    } else if is_date {
                                        let parsed = chrono::NaiveDate::parse_from_str(
                                            cell, "%Y-%m-%d",
                                        )
                                        .unwrap_or_else(|_| chrono::Local::now().date_naive());
                                        actions.new_popover = Some(CellPopover {
                                            open: true,
                                            row_idx,
                                            col_idx,
                                            buffer: cell.clone(),
                                            pos: rect_min,
                                            mode: PopoverMode::Date(parsed),
                                        });
                                    } else if is_long {
                                        actions.new_popover = Some(CellPopover {
                                            open: true,
                                            row_idx,
                                            col_idx,
                                            buffer: cell.clone(),
                                            pos: rect_min,
                                            mode: PopoverMode::Text,
                                        });
                                    } else {
                                        actions.new_editing_cell = Some(Some((row_idx, col_idx)));
                                        actions.new_edit_buffer = Some(cell.clone());
                                    }
                                }
                            }
                        });
                    }
                });
            });
    });
}