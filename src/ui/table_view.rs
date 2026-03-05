use crate::App;
use crate::db::schema::SortDir;
use crate::ui::table_grid;
use eframe::egui;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let mut actions = table_grid::GridActions::default();
    table_grid::show(app, ui, &mut actions);
    apply_actions(app, ui, actions);
}

fn apply_actions(app: &mut App, ui: &mut egui::Ui, actions: table_grid::GridActions) {
    let table_grid::GridActions {
        do_commit_edit,
        do_cancel_edit,
        new_editing_cell,
        new_edit_buffer,
        mut do_commit_insert,
        new_row_updates,
        new_popover,
        sort_click,
        navigate_to,
    } = actions;

    // ── Below-table controls ──────────────────────────────────────────────
    if let Some(err) = app.table_view.as_ref().and_then(|v| v.new_row_error.clone()) {
        ui.colored_label(egui::Color32::RED, err);
    }
    ui.horizontal(|ui| {
        if ui.button("+ Insert row").clicked() {
            do_commit_insert = true;
        }
    });

    // ── Apply all deferred mutations ──────────────────────────────────────
    if let Some(buf) = new_edit_buffer {
        if let Some(view) = &mut app.table_view {
            view.edit_buffer = buf;
        }
    }
    if let Some(cell) = new_editing_cell {
        if let Some(view) = &mut app.table_view {
            view.editing_cell = cell;
        }
    }
    for (col_idx, val) in new_row_updates {
        if let Some(view) = &mut app.table_view {
            if col_idx < view.new_row.len() {
                view.new_row[col_idx] = val;
            }
        }
    }
    if do_cancel_edit {
        if let Some(view) = &mut app.table_view {
            view.editing_cell = None;
            view.edit_buffer.clear();
        }
    }
    if do_commit_edit {
        app.commit_edit();
    }
    if do_commit_insert {
        app.commit_insert();
    }
    if let Some(p) = new_popover {
        app.cell_popover = Some(p);
    }

    // ── Sort ──────────────────────────────────────────────────────────────
    if let Some(col_idx) = sort_click {
        if let Some(view) = &mut app.table_view {
            if view.sort_col == Some(col_idx) {
                view.sort_dir = if view.sort_dir == SortDir::Asc {
                    SortDir::Desc
                } else {
                    SortDir::Asc
                };
            } else {
                view.sort_col = Some(col_idx);
                view.sort_dir = SortDir::Asc;
            }
            view.apply_sort();
        }
    }

    // ── FK navigation ─────────────────────────────────────────────────────
    if let Some((ref_table, ref_id)) = navigate_to {
        app.select_table(&ref_table.clone());
        if let Some(view) = &mut app.table_view {
            let id_col = view.columns.iter().position(|c| c == "id").unwrap_or(0);
            view.highlighted_row = view.rows.iter().position(|r| {
                r.get(id_col).map(|v| v == &ref_id).unwrap_or(false)
            });
        }
        app.selected_table = Some(ref_table);
    }
}