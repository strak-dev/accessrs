#[derive(Clone)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColType,
    pub primary_key: bool,
    pub not_null: bool,
}

#[derive(Clone, PartialEq)]
pub enum ColType {
    Text,
    Integer,
    Real,
    Blob,
    Date,
    ForeignKey(String),
    Note,
    Boolean,
}

impl ColType {
    pub fn label(&self) -> &str {
        match self {
            ColType::Text => "TEXT",
            ColType::Integer => "INTEGER",
            ColType::Real => "REAL",
            ColType::Blob => "BLOB",
            ColType::Date => "DATE",
            ColType::ForeignKey(_) => "FK",
            ColType::Note => "NOTE",
            ColType::Boolean => "BOOLEAN",
        }
    }
    pub fn sql_type(&self) -> &str {
        match self {
            ColType::Text => "TEXT",
            ColType::Integer => "INTEGER",
            ColType::Real => "REAL",
            ColType::Blob => "BLOB",
            ColType::Date => "DATE",
            ColType::ForeignKey(_) => "INTEGER",
            ColType::Note => "NOTE",
            ColType::Boolean => "INTEGER"
        }
    }
    pub fn base_types() -> &'static [ColType] {
        &[ColType::Text, ColType::Integer, ColType::Real, ColType::Blob, ColType::Date, ColType::Note, ColType::Boolean]
    }
}

impl Default for ColumnDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            col_type: ColType::Text,
            primary_key: false,
            not_null: false,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Clone)]
pub struct ForeignKeyInfo {
    pub col_idx: usize,
    pub ref_table: String,
    pub ref_col: String,
}