 # AccessRS

A native desktop SQLite database browser and editor built in Rust using [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe). AccessRS provides a clean, keyboard-friendly GUI for opening, inspecting, and editing SQLite databases — including full CRUD operations, type-aware cell rendering, foreign key navigation, and rich note editing.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Module Reference](#module-reference)
  - [main.rs](#mainrs)
  - [app.rs](#apprs)
  - [db/schema.rs](#dbschemars)
  - [db/table_view.rs](#dbtable_viewrs)
  - [ui/app_ui.rs](#uiapp_uirs)
  - [ui/sidebar.rs](#uisidebarrs)
  - [ui/toolbar.rs](#uitoolbarrs)
  - [ui/table_grid.rs](#uitable_gridrs)
  - [ui/table_view.rs](#uitable_viewrs)
  - [ui/create_dialog.rs](#uicreate_dialogrs)
  - [ui/popover.rs](#uipopoverrs)
  - [ui/empty_state.rs](#uiempty_staters)
  - [easy_mark/](#easy_mark)
- [Data Flow & Rendering Loop](#data-flow--rendering-loop)
- [Cell Editing System](#cell-editing-system)
- [Column Type System](#column-type-system)
- [Foreign Key Navigation](#foreign-key-navigation)
- [Sorting](#sorting)
- [Insert Row Flow](#insert-row-flow)
- [Popover System](#popover-system)
- [Note Columns & EasyMark Rendering](#note-columns--easymark-rendering)
- [Database Lifecycle](#database-lifecycle)
- [Table Creation](#table-creation)
- [Fonts](#fonts)
- [Keyboard Shortcuts](#keyboard-shortcuts)

---

## Overview

AccessRS opens SQLite `.db`, `.sqlite`, or `.sqlite3` files and lets you:

- Browse all user-defined tables from a resizable sidebar
- View table rows in a virtualized, striped, resizable-column grid (up to 1,000 rows)
- **Inline-edit** any cell by double-clicking it — changes are committed immediately to the database with a full-row `WHERE` clause
- **Insert new rows** via a persistent input strip at the bottom of the grid
- **Create new tables** with a schema designer dialog that generates and previews the `CREATE TABLE` SQL live
- **Navigate foreign key relationships** — FK columns render as clickable blue underlined links that jump to the referenced table and highlight the matching row
- **Pick dates** via a calendar widget for `DATE`-typed columns
- **Edit long text** in a resizable popover window
- **Edit and preview notes** using a built-in lightweight markup language (EasyMark) with `*bold*`, `/italics/`, `` `code` ``, `# headings`, bullet lists, and more
- **Toggle booleans** directly via checkboxes for `BOOLEAN`-typed columns
- **Sort any column** ascending/descending by clicking its header

---

## Architecture

```
AccessRS
├── main.rs              Entry point, eframe init, font setup
├── app.rs               Central App struct + database mutation methods
├── db/
│   ├── mod.rs
│   ├── schema.rs        Column type definitions, ForeignKeyInfo, SortDir
│   └── table_view.rs    TableView: schema introspection + row loading + sort
└── ui/
    ├── mod.rs
    ├── app_ui.rs        eframe::App impl — top-level update() render loop
    ├── sidebar.rs       Left panel: table list + "+" and "r" buttons
    ├── toolbar.rs       Table heading + Refresh button
    ├── table_grid.rs    Core grid widget + GridActions accumulator pattern
    ├── table_view.rs    Thin bridge: calls grid, then applies GridActions to App
    ├── create_dialog.rs Modal dialog for CREATE TABLE
    ├── popover.rs       Floating window for long text / date / note editing
    └── empty_state.rs   "No DB" and "No table selected" placeholder screens
easy_mark/
    ├── mod.rs
    ├── easy_mark_parser.rs   Lightweight markup parser
    └── easy_mark_viewer.rs   egui renderer for parsed EasyMark tokens
```

The application follows a **pure-render + deferred-mutation** pattern throughout: the `table_grid` rendering function is passed an `App` reference (read-only via shared clone snapshots) and writes user intentions into a `GridActions` struct. After the grid renders, `apply_actions` in `table_view.rs` applies those mutations back to `App` state and triggers SQL operations. This cleanly separates UI layout from state mutation and avoids borrow conflicts inside the egui rendering pass.

---

## Module Reference

### `main.rs`

The binary entry point. Sets up `eframe::NativeOptions` with an initial window size of 1024×768 and title `"AccessRS"`. Before handing off to the `App`, it installs **FiraCode Nerd Font** (bundled at compile time via `include_bytes!`) as the default font for both proportional and monospace text families, giving the app consistent ligature-aware rendering throughout. It then calls `eframe::run_native` with a boxed `App::default()`.

---

### `app.rs`

Defines the central `App` struct, which is the single source of truth for all runtime state:

| Field | Type | Purpose |
|---|---|---|
| `conn` | `Option<Connection>` | Active rusqlite database connection |
| `db_path` | `Option<String>` | Path string for display in the status bar |
| `status` | `String` | Status bar message (bottom panel) |
| `tables` | `Vec<String>` | List of user table names from `sqlite_master` |
| `selected_table` | `Option<String>` | Currently active table name |
| `create_dialog` | `CreateTableDialog` | State for the "Create Table" modal |
| `cell_popover` | `Option<CellPopover>` | Currently open floating cell editor |
| `table_view` | `Option<TableView>` | Loaded schema + row data for the selected table |

`App` also defines the core database mutation methods:

- **`open_db(path)`** — Opens a rusqlite connection, enables `PRAGMA foreign_keys = ON`, clears previous state, and calls `refresh_tables()`.
- **`refresh_tables()`** — Queries `sqlite_master` for all non-system tables ordered by name, populates `self.tables`.
- **`select_table(name)`** — Calls `TableView::load(conn, name)`, stores the result in `self.table_view`, and updates the status bar with the row count.
- **`create_table()`** — Calls `create_dialog.to_sql()` to validate and generate SQL, executes it, refreshes the table list, and selects the newly created table.
- **`commit_edit()`** — Reads the currently editing cell coordinates and value from `table_view`, constructs a parameterized `UPDATE … SET col = ? WHERE col1 = ? AND col2 = ? …` using all columns as the WHERE predicate (since SQLite tables without explicit rowid need a full-row match), executes it, and updates the in-memory row on success.
- **`commit_insert()`** — Reads the `new_row` buffer from `table_view`, filters out blank fields, constructs `INSERT INTO … (cols) VALUES (?, ?, …)`, executes it, clears the insert buffer, and reloads rows from the database.

---

### `db/schema.rs`

Defines shared data model types used across the `db` and `ui` layers:

**`ColType`** — An enum representing all supported SQLite column types as understood by the app:

| Variant | SQL type stored | Notes |
|---|---|---|
| `Text` | `TEXT` | General string |
| `Integer` | `INTEGER` | Numeric |
| `Real` | `REAL` | Float |
| `Blob` | `BLOB` | Renders as `[BLOB]` |
| `Date` | `DATE` | Triggers calendar popover on double-click |
| `Note` | `NOTE` | Multi-line text with EasyMark preview |
| `Boolean` | `INTEGER` | Renders as a checkbox; stores 0/1 |
| `ForeignKey(String)` | `INTEGER` | Stores the referenced table name; adds `REFERENCES table(id)` |

`ColType::base_types()` returns a static slice of all variants (excluding `ForeignKey`, which is handled separately in the UI with a per-table FK list).

**`ColumnDef`** — Represents a single column definition used in the `CreateTableDialog`: name, `ColType`, `primary_key` flag, and `not_null` flag.

**`SortDir`** — `Asc` / `Desc` enum used to track sort state in `TableView`.

**`ForeignKeyInfo`** — Holds resolved FK metadata for a column: `col_idx` (the column's position in the rendered grid), `ref_table` (the target table name), and `ref_col` (the target column name, always `id` in practice).

---

### `db/table_view.rs`

`TableView` is a pure data struct that caches everything needed to render a table:

| Field | Purpose |
|---|---|
| `table_name` | Name of the open table |
| `columns` | Ordered list of column names |
| `rows` | 2D `Vec<Vec<String>>` — all cells as strings (max 1,000 rows) |
| `editing_cell` | `Option<(row_idx, col_idx)>` — which cell is currently in inline-edit mode |
| `edit_buffer` | The live text content of the in-progress edit |
| `new_row` | Parallel `Vec<String>` for the insert strip at the bottom of the grid |
| `new_row_error` | Validation error for the insert strip |
| `sort_col` / `sort_dir` | Current sort state |
| `date_columns` | `HashSet<usize>` of column indices whose SQL type is `DATE` |
| `note_columns` | `HashSet<usize>` of column indices whose SQL type is `NOTE` |
| `bool_columns` | `HashSet<usize>` of column indices whose SQL type is `BOOLEAN` |
| `foreign_keys` | `Vec<ForeignKeyInfo>` from `PRAGMA foreign_key_list` |
| `highlighted_row` | `Option<usize>` — row painted with a yellow tint after FK navigation |

**`TableView::load(conn, table_name)`** performs two PRAGMA queries:

1. `PRAGMA table_info(table_name)` — retrieves column names and declared types. As it iterates results, it populates `date_columns`, `note_columns`, and `bool_columns` HashSets by matching the uppercased type string.
2. `PRAGMA foreign_key_list(table_name)` — retrieves FK definitions. For each FK it resolves the local column index by name lookup and stores a `ForeignKeyInfo`.

It then calls `reload_rows` which issues `SELECT * FROM table LIMIT 1000` and converts every cell value to a `String` (mapping `NULL` → `""`, `Integer` → `n.to_string()`, `Real` → `f.to_string()`, `Blob` → `"[BLOB]"`).

**`apply_sort()`** sorts `self.rows` in-place by the chosen column. It tries to parse both cell values as `f64` first (numeric sort), falling back to lexicographic string comparison. Sort direction is respected in both branches.

---

### `ui/app_ui.rs`

This file contains the `eframe::App` impl for `App`, making it the top-level render driver. Every frame, `update()` runs the following layout in order:

1. **Top menu bar** (`egui::TopBottomPanel::top`) — A `File` menu with "Open database…", "New database…", and conditionally "Close database". File pickers use the `rfd` (Rust File Dialog) crate, filtered to `.db`, `.sqlite`, `.sqlite3` extensions.

2. **Bottom status bar** (`egui::TopBottomPanel::bottom`) — Displays `self.status`, which is updated on every significant operation (open, select, insert, update, etc.).

3. **Create Table dialog** — `create_dialog.show(ctx, &tables)` is called unconditionally each frame; it returns `true` if the user clicked "Create", at which point `self.create_table()` is called.

4. **Cell popover** — If `self.cell_popover` is `Some`, it calls `popover.show(ctx)` which returns `(commit, cancel)`. On commit, it funnels the popover's buffer back into `table_view.editing_cell` + `edit_buffer` and calls `self.commit_edit()`. On cancel or close, it sets `cell_popover` to `None`.

5. **Sidebar** — `sidebar::show(self, ctx)` renders the left panel.

6. **Central panel** — Handles three display states:
   - No connection open → `empty_state::show_no_db`
   - Connection open but no table selected → `empty_state::show_no_table`
   - Table selected → renders the toolbar and then `table_view::show`

---

### `ui/sidebar.rs`

Renders a resizable left panel (default 200px wide) titled "Tables". It only appears when a database connection is open (`app.conn.is_some()`).

The panel header shows:
- A "Tables" heading
- A small **"+"** button (right-aligned) that sets `app.create_dialog.open = true`
- A small **"r"** button that calls `app.refresh_tables()`

Below the separator, it renders a scrollable list of `selectable_label` buttons for each table in `app.tables`. Clicking a label calls `app.select_table(table)`. The currently selected table is highlighted using egui's selection styling.

---

### `ui/toolbar.rs`

A thin horizontal strip rendered above the grid. It shows the table name as a heading on the left and a **"Refresh"** button on the right. Clicking Refresh calls `view.reload_rows(conn)` to re-query the database and discard any unsaved in-memory edits.

---

### `ui/table_grid.rs`

The most complex module. Contains two public items:

#### `GridActions`

A plain data struct that accumulates all user intentions during a single render pass. Because egui is an immediate-mode UI, it's unsafe to mutate app state while iterating over rows. Instead, the grid populates a `GridActions` at render time, and mutations are applied after the fact. Fields:

| Field | Meaning |
|---|---|
| `do_commit_edit` | User confirmed an inline edit (Enter or focus loss) |
| `do_cancel_edit` | User cancelled an inline edit (Escape) |
| `new_editing_cell` | Request to change which cell is in edit mode |
| `new_edit_buffer` | Updated content for the edit buffer |
| `do_commit_insert` | User pressed Enter in the insert strip |
| `new_row_updates` | Vec of `(col_idx, value)` changes to the insert strip |
| `new_popover` | A new `CellPopover` to open |
| `sort_click` | Column index whose header was clicked |
| `navigate_to` | `(table_name, row_id)` for FK navigation |

#### `show(app, ui, actions)`

Builds a `TableBuilder` (from `egui_extras`) with:
- Striped rows
- Resizable columns
- A fixed 36px first column for row numbers
- Auto-sized, resizable, clipped columns for each data column
- 24px row height throughout

**Header row:** For each column, renders a bold button whose label appends `" ^"` or `" v"` if that column is the active sort column. Clicking records the column index in `actions.sort_click`.

**Body rows:** Iterates `rows.len() + 1` rows. The last row (index == `rows.len()`) is the **insert strip**:
- Row number column shows `"*"` as a visual indicator
- Each data column renders either a `checkbox` (for bool columns) or a `TextEdit::singleline` with hint text `"…"`
- Changes to text fields push `(col_idx, new_val)` into `actions.new_row_updates`
- Pressing Enter while a text field has focus sets `actions.do_commit_insert = true`

For regular data rows, each cell renders one of these depending on column metadata:

**Boolean column:** A `checkbox` widget. If it changes, immediately sets the editing cell, buffer, and `do_commit_edit = true` — a single click is sufficient to toggle and persist.

**Foreign Key column:** An `egui::Label` styled in blue (`rgb(100, 160, 255)`) with underline and `Sense::click()`. Displays `"-> {value}"`. Clicking records `actions.navigate_to = Some((ref_table, cell_value))`.

**All other display cells:** A plain `ui.label(&display)`.

The `display` string is computed as:
- **Note column:** First line of text, truncated to 50 characters with `…`, or `" Empty note"` if blank.
- **Long text (> 60 chars):** First 60 characters followed by `…`.
- **Everything else:** The raw cell string.

If the row is the `highlighted_row`, a semi-transparent yellow rectangle (`rgba(255, 255, 0, 15)`) is painted behind the cell using `ui.painter().rect_filled(...)` before the label renders.

**Double-click behavior** (non-boolean cells only):
- **Note column** → opens a `CellPopover` in `PopoverMode::Note { editing: false }` (preview mode by default)
- **Date column** → parses the stored `YYYY-MM-DD` string (defaulting to today if unparseable), opens a `CellPopover` in `PopoverMode::Date(parsed_date)`
- **Long text** → opens a `CellPopover` in `PopoverMode::Text`
- **All other columns** → enters inline-edit mode: sets `new_editing_cell` and `new_edit_buffer`

**Inline edit mode** (when `editing_cell == Some((row_idx, col_idx))`): Renders a `TextEdit::singleline` that calls `response.request_focus()` each frame to keep the cursor there. Monitors for Enter (commit), Escape (cancel), or `lost_focus()` (also commit).

---

### `ui/table_view.rs`

A thin bridge module. Its `show(app, ui)` function:
1. Creates a default `GridActions`
2. Calls `table_grid::show(app, ui, &mut actions)` — the pure render pass
3. Calls `apply_actions(app, ui, actions)` — the mutation pass

`apply_actions` processes all `GridActions` fields in order:
- Renders the "Insert failed" error label if `new_row_error` is set
- Renders the **"+ Insert row"** button (which also triggers `do_commit_insert`)
- Flushes `new_edit_buffer`, `new_editing_cell`, and `new_row_updates` into `app.table_view`
- Applies cancel (clears editing state) or commit (calls `app.commit_edit()`) for inline edits
- Applies `do_commit_insert` by calling `app.commit_insert()`
- Stores a new popover into `app.cell_popover`
- Handles **sort**: if the clicked column is already the sort column, toggles direction; otherwise sets the new sort column and defaults to Asc. Then calls `view.apply_sort()`.
- Handles **FK navigation**: calls `app.select_table(&ref_table)` then searches the newly loaded view for the row whose `id` column matches the FK value, storing the result in `view.highlighted_row`.

---

### `ui/create_dialog.rs`

`CreateTableDialog` is a modal `egui::Window` for designing and creating new tables.

**State:**
- `open: bool` — controls whether the window is visible
- `table_name: String` — the user-typed name
- `columns: Vec<ColumnDef>` — the column definitions being built
- `error: Option<String>` — validation or SQL error message

The dialog always pre-populates with an `id INTEGER PRIMARY KEY NOT NULL` row (non-editable, shown as a disabled button label). Subsequent columns are editable rows with:
- A text field for the column name (140px wide)
- A `ComboBox` showing available types: TEXT, INTEGER, REAL, BLOB, DATE, NOTE, BOOLEAN, and one `FK → <table>` entry per existing table
- Checkboxes for `PRIMARY KEY` and `NOT NULL`
- A small `"x"` button to delete the column

A **"+ Add column"** button appends a new default `ColumnDef`.

Below the column list, `to_sql()` is called on every frame and the resulting SQL is rendered in a weak monospace label as a live preview. Any error from `to_sql()` (empty table name, etc.) is shown in red.

**`to_sql()`** validates that `table_name` is non-empty, then constructs `CREATE TABLE name (id INTEGER PRIMARY KEY AUTOINCREMENT, ...)`. For `ForeignKey(ref_table)` columns it appends `REFERENCES ref_table(id)`. For `NOT NULL` columns (that aren't already `PRIMARY KEY`) it appends `NOT NULL`.

Clicking **"Create"** returns `true` to the caller in `app_ui.rs` which then calls `app.create_table()`. Clicking **"Cancel"** calls `reset()` which reinstates the default state.

---

### `ui/popover.rs`

`CellPopover` is a floating `egui::Window` that appears at the cell's screen position and handles three editing modes defined by `PopoverMode`:

#### `PopoverMode::Date(NaiveDate)`
A compact 280×60 window titled "Pick Date". Shows an `egui_extras::DatePickerButton` calendar widget. "Save" formats the selected date as `YYYY-MM-DD`, writes it to `self.buffer`, and signals commit. "Cancel" or Escape signals cancel.

#### `PopoverMode::Text`
A 320×240 resizable window titled "Edit Cell". Shows a multi-line `TextEdit` (6 rows, full width). "Save" or `Ctrl+Enter` commits. "Cancel" or `Escape` cancels.

#### `PopoverMode::Note { editing: bool }`
A 500×400 resizable window titled "Note" with a toggle button in the top-right corner:

- **Preview mode** (`editing: false`): Renders `self.buffer` using `easy_mark::easy_mark(ui, &self.buffer)` — the EasyMark viewer. Shows a "Close" button and an "Edit" button to switch modes.
- **Edit mode** (`editing: true`): Renders a scrollable monospace `TextEdit::multiline`. Shows "Save", "Cancel", and a hint `"Ctrl+Enter to save"`. `Ctrl+Enter` commits; `Escape` cancels.

The window's `open` bool is bound to `win_open`; if the user closes it via the `×` button, `win_open` becomes false and the popover treats that as a cancel signal.

`show()` returns `(bool, bool)` — `(commit, cancel)` — which `app_ui.rs` uses to decide whether to call `commit_edit()` or just dismiss the popover.

---

### `ui/empty_state.rs`

Two simple placeholder functions:

- **`show_no_db(app, ui)`** — Renders a centered "No database open" heading with "Open database…" and "New database…" buttons. These invoke `rfd::FileDialog` directly without going through the menu.
- **`show_no_table(ui)`** — Renders a centered "Select a table from the sidebar." label.

---

### `easy_mark/`

A self-contained lightweight markup system. It is a trimmed-down Markdown-inspired format with no external dependencies beyond egui itself.

#### `easy_mark_parser.rs`

A zero-copy iterator-based parser. `Parser<'a>` holds a string slice `s` that it advances as it consumes tokens. It tracks `start_of_line: bool` and a `Style` struct that accumulates the current text style state.

Recognized syntax (processed in priority order):

| Syntax | Result |
|---|---|
| `\n` | `Item::Newline` + style reset |
| `\\\n` | Line continuation (ignored) |
| `\\x` | Escaped character (literal) |
| Leading spaces | `Item::Indentation(n)` |
| `# text` | Heading style on following text |
| `> text` | `Item::QuoteIndent` + quoted style |
| `- ` | `Item::BulletPoint` |
| `N. ` | `Item::NumberedPoint("N")` |
| `---` | `Item::Separator` |
| ` ```lang\ncode``` ` | `Item::CodeBlock(lang, code)` |
| `` `code` `` | Inline code style |
| `*text*` | Toggle bold |
| `_text_` | Toggle underline |
| `~text~` | Toggle strikethrough |
| `/text/` | Toggle italics |
| `$text$` | Toggle small |
| `^text^` | Toggle raised |
| `<url>` | `Item::Hyperlink(url, url)` |
| `[text](url)` | `Item::Hyperlink(text, url)` |

Styles are cumulative and stack — e.g. `*_bold underline_*` produces text with both `strong: true` and `underline: true`.

#### `easy_mark_viewer.rs`

Renders `Parser` output into egui widgets. `easy_mark(ui, text)` is the public entry point. It allocates a left-to-right wrapping layout and iterates tokens:

- `Text(style, text)` → `ui.label(rich_text_from_style(text, style))` with all style flags applied to the `RichText`
- `Hyperlink(style, text, url)` → `ui.add(Hyperlink::from_label_and_url(...))`
- `Separator` → horizontal `Separator` widget
- `Indentation(n)` → blank space allocation
- `QuoteIndent` → a thin vertical line drawn via `ui.painter().line_segment()`
- `BulletPoint` → a small filled circle via `ui.painter().circle_filled()`
- `NumberedPoint(n)` → right-aligned `"n."` text via `ui.painter().text()`
- `CodeBlock(lang, code)` → monospace text with a filled rectangle background using `ui.visuals().code_bg_color`

`rich_text_from_style` maps the `Style` struct directly to egui `RichText` modifiers: `.heading()`, `.strong()`, `.italics()`, `.underline()`, `.strikethrough()`, `.small()`, `.raised()`, `.code()`, `.weak()` (for quoted text).

---

## Data Flow & Rendering Loop

```
eframe calls App::update() every frame
│
├─ TopBottomPanel (menu bar)        ← File open/new/close
├─ TopBottomPanel (status bar)      ← self.status
├─ CreateTableDialog::show()        ← returns true if Create clicked
│   └─ App::create_table()
├─ CellPopover::show()              ← returns (commit, cancel)
│   └─ App::commit_edit()
├─ sidebar::show()                  ← Table list; calls App::select_table()
└─ CentralPanel
    ├─ [no db]  empty_state::show_no_db()
    ├─ [no tbl] empty_state::show_no_table()
    └─ [active]
        ├─ toolbar::show()          ← Refresh calls TableView::reload_rows()
        └─ table_view::show()
            ├─ table_grid::show()   ← RENDER PASS (read-only, fills GridActions)
            └─ apply_actions()      ← MUTATION PASS
                ├─ App::commit_edit()
                ├─ App::commit_insert()
                ├─ TableView::apply_sort()
                └─ App::select_table() (FK navigation)
```

---

## Cell Editing System

Inline editing uses a two-phase approach:

1. **Activation:** Double-clicking a plain cell sets `view.editing_cell = Some((row, col))` and `view.edit_buffer = cell_value.clone()`. On the next frame, the grid renders a `TextEdit` instead of a label and calls `request_focus()`.

2. **Confirmation:** The edit is committed when:
   - Enter is pressed while the text field has focus
   - The text field loses focus (click elsewhere)
   - A checkbox toggles (immediate commit, no Enter required)

3. **Cancellation:** Escape clears `editing_cell` and `edit_buffer` without writing to the database.

4. **SQL generation:** `commit_edit()` generates `UPDATE table SET col = ?N WHERE col1 = ?1 AND col2 = ?2 … AND colN = ?N-1` using the entire original row as the WHERE predicate. This avoids depending on `rowid` and works with any table structure, though it means edits can fail silently if two rows are identical.

---

## Column Type System

Column types are detected at table load time by reading `PRAGMA table_info` and matching the declared type string (uppercased):

| Declared type | Detected as |
|---|---|
| `DATE` | `date_columns` HashSet |
| `NOTE` | `note_columns` HashSet |
| `BOOLEAN` | `bool_columns` HashSet |
| Anything else | Plain text rendering |

Foreign keys are detected separately via `PRAGMA foreign_key_list` and stored as `ForeignKeyInfo` with a resolved column index. These three mechanisms are independent: a column can be a foreign key and still render as plain text (if it references a non-standard type).

---

## Foreign Key Navigation

FK columns are rendered as `"-> {value}"` blue underlined clickable labels. When clicked:

1. `actions.navigate_to = Some((ref_table, cell_value))` is set in `GridActions`
2. `apply_actions` calls `app.select_table(&ref_table)`, which loads the referenced table's `TableView`
3. It then scans the newly loaded `view.rows` for the row where the `id` column equals `cell_value`
4. The matching row index is stored in `view.highlighted_row`
5. On the next render pass, `table_grid` paints that row with a yellow highlight (`rgba(255,255,0,15)`)

This provides one-click relational navigation across any FK relationship defined in the SQLite schema.

---

## Sorting

Sorting is client-side only (no ORDER BY clause is sent to SQLite). `TableView::apply_sort()` sorts `self.rows` in-place using Rust's stable sort. For each compared pair of cells, it first attempts `f64` parsing and uses numeric ordering; if either parse fails, it falls back to lexicographic string ordering. Sort direction is toggled by clicking the same column header again.

The current sort column and direction are preserved in `view.sort_col` and `view.sort_dir`. Note that re-loading rows (e.g. after an insert) resets the rows to database order; the sort is not automatically re-applied after a reload.

---

## Insert Row Flow

The insert strip is always the last row in the grid, identified by `row_idx == rows.len()`. It holds live `TextEdit` fields (or checkboxes for bool columns) backed by `view.new_row: Vec<String>`. Changes are accumulated in `actions.new_row_updates` and flushed to `view.new_row` in `apply_actions`.

When the user presses Enter inside any insert field, `do_commit_insert` is set. `commit_insert()` then:
1. Zips column names with `new_row` values
2. Filters out any pair where the value is blank (allowing partial inserts — SQLite will use column defaults for omitted fields)
3. Validates that at least one value is provided
4. Generates `INSERT INTO table (cols) VALUES (?1, ?2, …)` with numbered placeholders
5. On success: clears `new_row`, clears `new_row_error`, and calls `reload_rows()` to reflect the database-assigned `id` and any defaults

---

## Popover System

The popover is a singleton: only one `CellPopover` can be open at a time, stored in `app.cell_popover`. The popover window opens at the screen position of the cell that was double-clicked (passed as `pos: egui::Pos2`). It is resizable and draggable. The `open` flag is bound to an `egui::Window::open()` call so the standard `×` close button also works.

When a popover commits, `app_ui.rs` sets `view.editing_cell` and `view.edit_buffer` to the popover's row/col and final buffer content, then calls `commit_edit()` — reusing the same update path as inline edits.

---

## Note Columns & EasyMark Rendering

Columns declared as type `NOTE` are treated as long-form rich text. In the grid, they show a truncated preview of the first line (up to 50 characters). Double-clicking opens the Note popover in preview mode, which renders the stored text through the EasyMark viewer.

EasyMark is a Markdown-subset format. Supported formatting in notes:

```
# Heading
*bold*   /italics/   _underline_   ~strikethrough~
`inline code`
$small text$   ^raised text^

> block quote

- bullet item
1. numbered item

---   (horizontal rule)

```lang
code block
```

[link text](https://url)   <https://url>
```

The user switches between preview (rendered) and edit (raw monospace text) mode via the "Edit" / "Preview" toggle button in the popover header.

---

## Database Lifecycle

```
App::default()          conn = None, no DB loaded
   │
   ├─ open_db(path)     rusqlite::Connection::open()
   │                    PRAGMA foreign_keys = ON
   │                    refresh_tables()
   │
   ├─ select_table(n)   TableView::load(conn, name)
   │                    PRAGMA table_info + foreign_key_list
   │                    SELECT * FROM table LIMIT 1000
   │
   ├─ commit_edit()     UPDATE … SET col = ? WHERE all_cols_match
   ├─ commit_insert()   INSERT INTO … (non-blank cols)
   ├─ create_table()    CREATE TABLE … from dialog SQL
   │
   └─ close (menu)     conn = None, all state cleared
```

The connection is held open for the lifetime of the app session. There is no explicit transaction management — each `commit_edit` and `commit_insert` runs as an auto-committed statement. Foreign key enforcement is active for the entire session (`PRAGMA foreign_keys = ON` is run on open).

---

## Table Creation

The "Create Table" dialog always includes an auto-managed `id INTEGER PRIMARY KEY AUTOINCREMENT` column as the first column (shown as a disabled button, not editable). User-defined columns start from index 1.

Generated SQL example for a table named `tasks` with a text column `title` and an FK to `projects`:

```sql
CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, project_id INTEGER REFERENCES projects(id))
```

The live SQL preview at the bottom of the dialog updates on every keystroke, giving immediate feedback. Errors (blank table name, SQL execution failure) appear in red below the preview.

---

## Fonts

FiraCode Nerd Font is embedded directly in the binary at compile time:

```rust
include_bytes!("../fonts/FiraCodeNerdFont-Regular.ttf")
```

It is registered under the key `"fira_nerd"` and inserted at position 0 of both `Proportional` and `Monospace` font families, making it the default for all text rendering including UI labels, cell values, and code blocks in EasyMark.

---

## Keyboard Shortcuts

| Shortcut | Context | Action |
|---|---|---|
| `Enter` | Inline cell edit | Commit edit |
| `Escape` | Inline cell edit | Cancel edit |
| `Enter` | Insert strip field | Commit insert |
| `Ctrl+Enter` | Text popover | Commit edit |
| `Ctrl+Enter` | Note popover (edit mode) | Commit edit |
| `Escape` | Text / Date / Note popover | Cancel / close |
| Double-click | Plain cell | Enter inline edit mode |
| Double-click | Long text cell (>60 chars) | Open text popover |
| Double-click | Date cell | Open date picker popover |
| Double-click | Note cell | Open note popover (preview mode) |
| Click | FK cell | Navigate to referenced table and row |
| Click | Column header | Sort ascending; click again to toggle desc |