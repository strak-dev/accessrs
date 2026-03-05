use crate::easy_mark;
use eframe::egui;

pub enum PopoverMode {
    Text,
    Date(chrono::NaiveDate),
    Note { editing: bool },
}

pub struct CellPopover {
    pub open: bool,
    pub row_idx: usize,
    pub col_idx: usize,
    pub buffer: String,
    pub pos: egui::Pos2,
    pub mode: PopoverMode,
}

impl CellPopover {
    /// Returns (commit, cancel)
    pub fn show(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let mut popover_commit = false;
        let mut popover_cancel = false;
        let mut win_open = self.open;

        let (title, window_size) = match &self.mode {
            PopoverMode::Date(_) => ("Pick Date", [280.0, 60.0]),
            PopoverMode::Text => ("Edit Cell", [320.0, 240.0]),
            PopoverMode::Note { .. } => ("Note", [500.0, 400.0]),
        };

        egui::Window::new(title)
            .open(&mut win_open)
            .default_pos(self.pos)
            .default_size(window_size)
            .min_size([200.0, 120.0])
            .max_size([800.0, 600.0])
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                match &mut self.mode {
                    PopoverMode::Date(date) => {
                        ui.horizontal(|ui| {
                            ui.label("Date:");
                            ui.add(
                                egui_extras::DatePickerButton::new(date)
                                    .id_salt("cell_date_picker"),
                            );
                        });
                        ui.add_space(4.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                self.buffer = date.format("%Y-%m-%d").to_string();
                                popover_commit = true;
                            }
                            if ui.button("Cancel").clicked() {
                                popover_cancel = true;
                            }
                        });
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            popover_cancel = true;
                        }
                    }
                    PopoverMode::Note { editing } => {
                        ui.horizontal(|ui| {
                            ui.heading("Note");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if *editing {
                                    if ui.button("Preview").clicked() {
                                        *editing = false;
                                    }
                                } else {
                                    if ui.button("Edit").clicked() {
                                        *editing = true;
                                    }
                                }
                            });
                        });
                        ui.separator();

                        if *editing {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.buffer)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(10)
                                        .font(egui::TextStyle::Monospace),
                                );
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Save").clicked() {
                                    popover_commit = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    popover_cancel = true;
                                }
                                ui.weak("Ctrl+Enter to save");
                            });
                            if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                                popover_commit = true;
                            }
                        } else {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                easy_mark::easy_mark(ui, &self.buffer);
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Close").clicked() {
                                    popover_cancel = true;
                                }
                            });
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            popover_cancel = true;
                        }
                    }
                    PopoverMode::Text => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.buffer)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(6),
                            );
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                popover_commit = true;
                            }
                            if ui.button("Cancel").clicked() {
                                popover_cancel = true;
                            }
                            ui.weak("Ctrl+Enter  •  Esc to cancel");
                        });
                        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                            popover_commit = true;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            popover_cancel = true;
                        }
                    }
                }
            });

        if !win_open {
            popover_cancel = true;
        }

        (popover_commit, popover_cancel)
    }
}