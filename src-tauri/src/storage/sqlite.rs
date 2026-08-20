use crate::error::{AppError, AppResult};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_void},
    path::Path,
    ptr::{self, NonNull},
};

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}

#[repr(C)]
struct Sqlite3Statement {
    _private: [u8; 0],
}

type ExecCallback =
    Option<unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int>;

#[cfg_attr(windows, link(name = "winsqlite3"))]
#[cfg_attr(not(windows), link(name = "sqlite3"))]
extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        database: *mut *mut Sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(database: *mut Sqlite3) -> c_int;
    fn sqlite3_errmsg(database: *mut Sqlite3) -> *const c_char;
    fn sqlite3_exec(
        database: *mut Sqlite3,
        sql: *const c_char,
        callback: ExecCallback,
        callback_arg: *mut c_void,
        error_message: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_free(pointer: *mut c_void);
    fn sqlite3_prepare_v2(
        database: *mut Sqlite3,
        sql: *const c_char,
        sql_length: c_int,
        statement: *mut *mut Sqlite3Statement,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(statement: *mut Sqlite3Statement) -> c_int;
    fn sqlite3_finalize(statement: *mut Sqlite3Statement) -> c_int;
    fn sqlite3_reset(statement: *mut Sqlite3Statement) -> c_int;
    fn sqlite3_clear_bindings(statement: *mut Sqlite3Statement) -> c_int;
    fn sqlite3_bind_null(statement: *mut Sqlite3Statement, index: c_int) -> c_int;
    fn sqlite3_bind_int64(statement: *mut Sqlite3Statement, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_text(
        statement: *mut Sqlite3Statement,
        index: c_int,
        value: *const c_char,
        length: c_int,
        destructor: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_column_count(statement: *mut Sqlite3Statement) -> c_int;
    fn sqlite3_column_type(statement: *mut Sqlite3Statement, index: c_int) -> c_int;
    fn sqlite3_column_int64(statement: *mut Sqlite3Statement, index: c_int) -> i64;
    fn sqlite3_column_text(statement: *mut Sqlite3Statement, index: c_int) -> *const u8;
    fn sqlite3_column_bytes(statement: *mut Sqlite3Statement, index: c_int) -> c_int;
    fn sqlite3_changes(database: *mut Sqlite3) -> c_int;
    fn sqlite3_busy_timeout(database: *mut Sqlite3, milliseconds: c_int) -> c_int;
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_INTEGER: c_int = 1;
const SQLITE_TEXT: c_int = 3;
const SQLITE_NULL: c_int = 5;
const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
const SQLITE_OPEN_FULLMUTEX: c_int = 0x0001_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Integer(i64),
    Text(String),
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    values: Vec<Value>,
}

impl Row {
    pub fn text(&self, index: usize) -> AppResult<String> {
        match self.values.get(index) {
            Some(Value::Text(value)) => Ok(value.clone()),
            Some(Value::Null) => Err(AppError::coded_with(
                "storage_null_value",
                [("column", index.to_string())],
            )),
            _ => Err(AppError::coded_with(
                "storage_type_mismatch",
                [
                    ("column", index.to_string()),
                    ("expected", "text".to_string()),
                ],
            )),
        }
    }

    pub fn optional_text(&self, index: usize) -> AppResult<Option<String>> {
        match self.values.get(index) {
            Some(Value::Text(value)) => Ok(Some(value.clone())),
            Some(Value::Null) => Ok(None),
            _ => Err(AppError::coded_with(
                "storage_type_mismatch",
                [
                    ("column", index.to_string()),
                    ("expected", "text_or_null".to_string()),
                ],
            )),
        }
    }

    pub fn integer(&self, index: usize) -> AppResult<i64> {
        match self.values.get(index) {
            Some(Value::Integer(value)) => Ok(*value),
            _ => Err(AppError::coded_with(
                "storage_type_mismatch",
                [
                    ("column", index.to_string()),
                    ("expected", "integer".to_string()),
                ],
            )),
        }
    }

    pub fn optional_integer(&self, index: usize) -> AppResult<Option<i64>> {
        match self.values.get(index) {
            Some(Value::Integer(value)) => Ok(Some(*value)),
            Some(Value::Null) => Ok(None),
            _ => Err(AppError::coded_with(
                "storage_type_mismatch",
                [
                    ("column", index.to_string()),
                    ("expected", "integer_or_null".to_string()),
                ],
            )),
        }
    }
}

pub struct Connection {
    raw: NonNull<Sqlite3>,
}

impl Connection {
    pub fn open(path: &Path) -> AppResult<Self> {
        let path_text = path.to_string_lossy();
        let filename = CString::new(path_text.as_bytes())
            .map_err(|_| AppError::coded("storage_path_contains_nul"))?;
        let mut raw = ptr::null_mut();
        let result = unsafe {
            sqlite3_open_v2(
                filename.as_ptr(),
                &mut raw,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX,
                ptr::null(),
            )
        };
        let raw = NonNull::new(raw).ok_or_else(|| AppError::Sqlite("open returned null".into()))?;
        let connection = Self { raw };
        if result != SQLITE_OK {
            return Err(connection.last_error("open"));
        }
        connection.check(
            unsafe { sqlite3_busy_timeout(connection.raw.as_ptr(), 5_000) },
            "busy_timeout",
        )?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )?;
        Ok(connection)
    }

    pub fn execute_batch(&self, sql: &str) -> AppResult<()> {
        let sql = CString::new(sql).map_err(|_| AppError::coded("storage_sql_contains_nul"))?;
        let mut error_message = ptr::null_mut();
        let result = unsafe {
            sqlite3_exec(
                self.raw.as_ptr(),
                sql.as_ptr(),
                None,
                ptr::null_mut(),
                &mut error_message,
            )
        };
        if result != SQLITE_OK {
            let message = if error_message.is_null() {
                self.error_message()
            } else {
                let message = unsafe { CStr::from_ptr(error_message) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { sqlite3_free(error_message.cast()) };
                message
            };
            return Err(AppError::Sqlite(message));
        }
        Ok(())
    }

    pub fn execute(&self, sql: &str, params: &[Value]) -> AppResult<usize> {
        let mut statement = Statement::prepare(self, sql)?;
        statement.bind(params)?;
        let result = unsafe { sqlite3_step(statement.raw.as_ptr()) };
        if result != SQLITE_DONE {
            return Err(self.last_error("execute"));
        }
        Ok(unsafe { sqlite3_changes(self.raw.as_ptr()) } as usize)
    }

    pub fn query(&self, sql: &str, params: &[Value]) -> AppResult<Vec<Row>> {
        let mut statement = Statement::prepare(self, sql)?;
        statement.bind(params)?;
        let mut rows = Vec::new();
        loop {
            match unsafe { sqlite3_step(statement.raw.as_ptr()) } {
                SQLITE_ROW => rows.push(statement.read_row()?),
                SQLITE_DONE => break,
                _ => return Err(self.last_error("query")),
            }
        }
        Ok(rows)
    }

    pub fn query_one(&self, sql: &str, params: &[Value]) -> AppResult<Option<Row>> {
        let rows = self.query(sql, params)?;
        if rows.len() > 1 {
            return Err(AppError::coded("storage_expected_single_row"));
        }
        Ok(rows.into_iter().next())
    }

    pub fn transaction<T>(&self, operation: impl FnOnce(&Self) -> AppResult<T>) -> AppResult<T> {
        self.execute_batch("BEGIN IMMEDIATE;")?;
        match operation(self) {
            Ok(value) => {
                if let Err(error) = self.execute_batch("COMMIT;") {
                    let _ = self.execute_batch("ROLLBACK;");
                    Err(error)
                } else {
                    Ok(value)
                }
            }
            Err(error) => {
                let _ = self.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }

    fn check(&self, result: c_int, context: &str) -> AppResult<()> {
        if result == SQLITE_OK {
            Ok(())
        } else {
            Err(self.last_error(context))
        }
    }

    fn last_error(&self, context: &str) -> AppError {
        AppError::Sqlite(format!("{context}: {}", self.error_message()))
    }

    fn error_message(&self) -> String {
        let pointer = unsafe { sqlite3_errmsg(self.raw.as_ptr()) };
        if pointer.is_null() {
            "unknown SQLite error".into()
        } else {
            unsafe { CStr::from_ptr(pointer) }
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = unsafe { sqlite3_close(self.raw.as_ptr()) };
    }
}

struct Statement<'a> {
    connection: &'a Connection,
    raw: NonNull<Sqlite3Statement>,
    bound_text: Vec<CString>,
}

impl<'a> Statement<'a> {
    fn prepare(connection: &'a Connection, sql: &str) -> AppResult<Self> {
        let sql = CString::new(sql).map_err(|_| AppError::coded("storage_sql_contains_nul"))?;
        let mut raw = ptr::null_mut();
        let result = unsafe {
            sqlite3_prepare_v2(
                connection.raw.as_ptr(),
                sql.as_ptr(),
                -1,
                &mut raw,
                ptr::null_mut(),
            )
        };
        if result != SQLITE_OK {
            return Err(connection.last_error("prepare"));
        }
        let raw = NonNull::new(raw)
            .ok_or_else(|| AppError::Sqlite("prepare returned null statement".into()))?;
        Ok(Self {
            connection,
            raw,
            bound_text: Vec::new(),
        })
    }

    fn bind(&mut self, params: &[Value]) -> AppResult<()> {
        self.connection.check(
            unsafe { sqlite3_reset(self.raw.as_ptr()) },
            "statement reset",
        )?;
        self.connection.check(
            unsafe { sqlite3_clear_bindings(self.raw.as_ptr()) },
            "clear bindings",
        )?;
        self.bound_text.clear();
        for value in params {
            if let Value::Text(text) = value {
                self.bound_text.push(
                    CString::new(text.as_bytes())
                        .map_err(|_| AppError::coded("storage_value_contains_nul"))?,
                );
            }
        }

        let mut text_index = 0usize;
        for (offset, value) in params.iter().enumerate() {
            let index = (offset + 1) as c_int;
            let result = match value {
                Value::Null => unsafe { sqlite3_bind_null(self.raw.as_ptr(), index) },
                Value::Integer(value) => unsafe {
                    sqlite3_bind_int64(self.raw.as_ptr(), index, *value)
                },
                Value::Text(_) => {
                    let value = &self.bound_text[text_index];
                    text_index += 1;
                    unsafe {
                        sqlite3_bind_text(
                            self.raw.as_ptr(),
                            index,
                            value.as_ptr(),
                            value.as_bytes().len() as c_int,
                            None,
                        )
                    }
                }
            };
            self.connection.check(result, "bind")?;
        }
        Ok(())
    }

    fn read_row(&self) -> AppResult<Row> {
        let count = unsafe { sqlite3_column_count(self.raw.as_ptr()) };
        let mut values = Vec::with_capacity(count as usize);
        for index in 0..count {
            let value = match unsafe { sqlite3_column_type(self.raw.as_ptr(), index) } {
                SQLITE_NULL => Value::Null,
                SQLITE_INTEGER => {
                    Value::Integer(unsafe { sqlite3_column_int64(self.raw.as_ptr(), index) })
                }
                SQLITE_TEXT => {
                    let pointer = unsafe { sqlite3_column_text(self.raw.as_ptr(), index) };
                    let length = unsafe { sqlite3_column_bytes(self.raw.as_ptr(), index) };
                    if pointer.is_null() || length < 0 {
                        return Err(AppError::coded("storage_invalid_text_column"));
                    }
                    let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
                    Value::Text(String::from_utf8_lossy(bytes).into_owned())
                }
                other => {
                    return Err(AppError::coded_with(
                        "storage_unsupported_column_type",
                        [("type", other.to_string())],
                    ))
                }
            };
            values.push(value);
        }
        Ok(Row { values })
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        let _ = unsafe { sqlite3_finalize(self.raw.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn db_path() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "s9lab-sqlite-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("db")
        ));
        fs::create_dir_all(&root).expect("root");
        root.join("test.db")
    }

    #[test]
    fn transaction_rolls_back_on_error() {
        let path = db_path();
        let connection = Connection::open(&path).expect("open");
        connection
            .execute_batch("CREATE TABLE values_table(value TEXT NOT NULL UNIQUE);")
            .expect("schema");
        let result: AppResult<()> = connection.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO values_table(value) VALUES (?1)",
                &[Value::from("first")],
            )?;
            transaction.execute(
                "INSERT INTO values_table(value) VALUES (?1)",
                &[Value::from("first")],
            )?;
            Ok(())
        });
        assert!(result.is_err());
        let row = connection
            .query_one("SELECT COUNT(*) FROM values_table", &[])
            .expect("count")
            .expect("row");
        assert_eq!(row.integer(0).expect("integer"), 0);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
