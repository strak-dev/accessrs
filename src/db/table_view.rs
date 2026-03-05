use rusqlite::Connection;
use super::schema::{ForeignKeyInfo, SortDir};

pub struct TableView {
    pub table_name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub error: Option<String>,
    pub editing_cell: Option<(usize, usize)>,
    pub edit_buffer: String,
    pub new_row: Vec<String>,
    pub new_row_error: Option<String>,
    pub sort_col: Option<usize>,
    pub sort_dir: SortDir,
    pub date_columns: std::collections::HashSet<usize>,
    pub note_columns: std::collections::HashSet<usize>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub highlighted_row: Option<usize>,
}

impl TableView {
    pub fn load(conn: &Connection, table_name: &str) -> Self {
        let mut view = Self {
            table_name: table_name.to_string(),
            columns: vec![],
            rows: vec![],
            error: None,
            editing_cell: None,
            edit_buffer: String::new(),
            new_row: vec![],
            new_row_error: None,
            sort_col: None,
            sort_dir: SortDir::Asc,
            date_columns: std::collections::HashSet::new(),
            note_columns: std::collections::HashSet::new(),
            foreign_keys: vec![],
            highlighted_row: None,
        };

        // Single PRAGMA call — load column names AND detect DATE columns together
        match conn.prepare(&format!("PRAGMA table_info({})", table_name)) {
            Err(e) => {
                view.error = Some(format!("Failed to load schema: {e}"));
                return view;
            }
            Ok(mut stmt) => {
                let results: Vec<(String, String)> = stmt
                    .query_map([], |row| {
                        let name: String = row.get(1)?;  // col 1 = name
                        let col_type: String = row.get(2)?; // col 2 = type
                        Ok((name, col_type))
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();

                for (idx, (name, col_type)) in results.into_iter().enumerate() {
                    view.columns.push(name);
                    match col_type.to_uppercase().as_str() {
                        "DATE" => { view.date_columns.insert(idx); }
                        "NOTE" => { view.note_columns.insert(idx); }
                        _ => {}
                    }
                }
            }
        }

        view.new_row = vec![String::new(); view.columns.len()];
        view.reload_rows(conn);
        // Load FK metadata
        if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA foreign_key_list({})", table_name)) {
            let fks: Vec<ForeignKeyInfo> = stmt
                .query_map([], |row| {
                    let ref_table: String = row.get(2)?; // table
                    let from_col: String = row.get(3)?;  // from (local col name)
                    let ref_col: String = row.get(4)?;   // to (remote col name)
                    Ok((from_col, ref_table, ref_col))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .filter_map(|(from_col, ref_table, ref_col)| {
                    let col_idx = view.columns.iter().position(|c| c == &from_col)?;
                    Some(ForeignKeyInfo { col_idx, ref_table, ref_col })
                })
                .collect();
            view.foreign_keys = fks;
        }
        view
    }

    pub fn reload_rows(&mut self, conn: &Connection) {
        self.rows.clear();
        let col_count = self.columns.len();
        let sql = format!("SELECT * FROM {} LIMIT 1000", self.table_name);
        match conn.prepare(&sql) {
            Err(e) => {
                self.error = Some(format!("Failed to query table: {e}"));
            }
            Ok(mut stmt) => {
                self.rows = stmt
                    .query_map([], |row| {
                        let cells = (0..col_count)
                            .map(|i| {
                                row.get_ref(i)
                                    .map(|v| match v {
                                        rusqlite::types::ValueRef::Null => String::new(),
                                        rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                                        rusqlite::types::ValueRef::Real(f) => f.to_string(),
                                        rusqlite::types::ValueRef::Text(t) => {
                                            String::from_utf8_lossy(t).to_string()
                                        }
                                        rusqlite::types::ValueRef::Blob(_) => "[BLOB]".to_string(),
                                    })
                                    .unwrap_or_else(|_| "ERR".to_string())
                            })
                            .collect();
                        Ok(cells)
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
            }
        }
    }

    pub fn apply_sort(&mut self) {
        if let Some(col) = self.sort_col {
            let dir = self.sort_dir.clone();
            self.rows.sort_by(|a, b| {
                let av = a.get(col).map(|s| s.as_str()).unwrap_or("");
                let bv = b.get(col).map(|s| s.as_str()).unwrap_or("");
                // try numeric sort first
                match (av.parse::<f64>(), bv.parse::<f64>()) {
                    (Ok(an), Ok(bn)) => {
                        let ord = an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal);
                        if dir == SortDir::Asc { ord } else { ord.reverse() }
                    }
                    _ => {
                        let ord = av.cmp(bv);
                        if dir == SortDir::Asc { ord } else { ord.reverse() }
                    }
                }
            });
        }
    }
}