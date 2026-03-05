use crate::db::schema::{ColumnDef, ColType};

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

        // First col is always the locked autoincrement id
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
}