//! Envoltorio mínimo y seguro sobre `libsqlite3` para los importadores de Vault.
//!
//! Los importadores abren siempre una **copia temporal** de la base de datos del otro navegador
//! en modo solo lectura. La apertura en modo escritura existe únicamente para que los tests
//! generen bases de datos sintéticas en directorios temporales.
//!
//! Los valores de texto y blob se guardan en `Zeroizing` porque pueden contener contraseñas
//! cifradas o nombres de usuario; `Debug` nunca muestra su contenido.

use std::ffi::{CStr, CString};
use std::fmt;
use std::path::Path;

use libc::{c_char, c_int, c_void};
use zeroize::Zeroizing;

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}
#[repr(C)]
struct Stmt {
    _private: [u8; 0],
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;
const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
const SQLITE_OPEN_NOMUTEX: c_int = 0x0000_8000;
const SQLITE_INTEGER: c_int = 1;
const SQLITE_FLOAT: c_int = 2;
const SQLITE_TEXT: c_int = 3;
const SQLITE_BLOB: c_int = 4;
/// `SQLITE_TRANSIENT`: SQLite copia el buffer antes de volver.
const SQLITE_TRANSIENT: isize = -1;

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        db: *mut *mut Sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close_v2(db: *mut Sqlite3) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_prepare_v2(
        db: *mut Sqlite3,
        sql: *const c_char,
        n_byte: c_int,
        stmt: *mut *mut Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_finalize(stmt: *mut Stmt) -> c_int;
    fn sqlite3_step(stmt: *mut Stmt) -> c_int;
    fn sqlite3_column_count(stmt: *mut Stmt) -> c_int;
    fn sqlite3_column_type(stmt: *mut Stmt, col: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut Stmt, col: c_int) -> i64;
    fn sqlite3_column_double(stmt: *mut Stmt, col: c_int) -> f64;
    fn sqlite3_column_blob(stmt: *mut Stmt, col: c_int) -> *const c_void;
    fn sqlite3_column_text(stmt: *mut Stmt, col: c_int) -> *const u8;
    fn sqlite3_column_bytes(stmt: *mut Stmt, col: c_int) -> c_int;
    fn sqlite3_bind_null(stmt: *mut Stmt, idx: c_int) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut Stmt, idx: c_int, value: i64) -> c_int;
    fn sqlite3_bind_double(stmt: *mut Stmt, idx: c_int, value: f64) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut Stmt,
        idx: c_int,
        value: *const c_char,
        n: c_int,
        destructor: isize,
    ) -> c_int;
    fn sqlite3_bind_blob(
        stmt: *mut Stmt,
        idx: c_int,
        value: *const c_void,
        n: c_int,
        destructor: isize,
    ) -> c_int;
}

/// Error de SQLite. El mensaje viene de `sqlite3_errmsg` y nunca contiene valores de filas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteError {
    pub code: i32,
    pub message: String,
}

impl fmt::Display for SqliteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SQLite ({}): {}", self.code, self.message)
    }
}

impl std::error::Error for SqliteError {}

fn plain_error(code: c_int, message: &str) -> SqliteError {
    SqliteError {
        code,
        message: message.to_owned(),
    }
}

/// Un valor de columna o de parámetro.
#[derive(Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(Zeroizing<String>),
    Blob(Zeroizing<Vec<u8>>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("Null"),
            Value::Integer(v) => write!(f, "Integer({v})"),
            Value::Real(v) => write!(f, "Real({v})"),
            Value::Text(t) => write!(f, "Text(<{} bytes>)", t.len()),
            Value::Blob(b) => write!(f, "Blob(<{} bytes>)", b.len()),
        }
    }
}

impl Value {
    /// Texto de la columna (o `""` si no es texto).
    pub fn as_text(&self) -> &str {
        match self {
            Value::Text(t) => t.as_str(),
            _ => "",
        }
    }

    /// Bytes de la columna: blob, o el texto como UTF-8.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Value::Blob(b) => b.as_slice(),
            Value::Text(t) => t.as_bytes(),
            _ => &[],
        }
    }

    /// Entero de la columna (o 0).
    pub fn as_i64(&self) -> i64 {
        match self {
            Value::Integer(v) => *v,
            Value::Real(v) => *v as i64,
            _ => 0,
        }
    }

    /// Construye un valor de texto.
    pub fn text(s: &str) -> Self {
        Value::Text(Zeroizing::new(s.to_owned()))
    }

    /// Construye un valor blob.
    pub fn blob(b: &[u8]) -> Self {
        Value::Blob(Zeroizing::new(b.to_vec()))
    }
}

/// Una conexión SQLite.
pub struct Database {
    raw: *mut Sqlite3,
}

impl Drop for Database {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close_v2(self.raw);
        }
    }
}

struct Statement<'db> {
    raw: *mut Stmt,
    db: &'db Database,
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.raw);
        }
    }
}

impl Database {
    fn open(path: &Path, flags: c_int) -> Result<Self, SqliteError> {
        let c_path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| plain_error(-1, "ruta con byte NUL"))?;
        let mut raw: *mut Sqlite3 = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                c_path.as_ptr(),
                &mut raw,
                flags | SQLITE_OPEN_NOMUTEX,
                std::ptr::null(),
            )
        };
        let db = Database { raw };
        if raw.is_null() {
            return Err(plain_error(rc, "sin memoria para abrir la base de datos"));
        }
        if rc != SQLITE_OK {
            return Err(db.error(rc));
        }
        Ok(db)
    }

    /// Abre una base de datos existente en modo solo lectura.
    pub fn open_read_only(path: &Path) -> Result<Self, SqliteError> {
        Self::open(path, SQLITE_OPEN_READONLY)
    }

    /// Abre (o crea) una base de datos en lectura/escritura. Pensado para fixtures de tests.
    pub fn open_read_write(path: &Path) -> Result<Self, SqliteError> {
        Self::open(path, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE)
    }

    fn error(&self, code: c_int) -> SqliteError {
        let message = unsafe {
            let ptr = sqlite3_errmsg(self.raw);
            if ptr.is_null() {
                String::from("error desconocido")
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        SqliteError { code, message }
    }

    fn prepare(&self, sql: &str) -> Result<Statement<'_>, SqliteError> {
        let c_sql = CString::new(sql).map_err(|_| plain_error(-1, "SQL con byte NUL"))?;
        let mut raw: *mut Stmt = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_prepare_v2(self.raw, c_sql.as_ptr(), -1, &mut raw, std::ptr::null_mut())
        };
        if rc != SQLITE_OK || raw.is_null() {
            return Err(self.error(rc));
        }
        Ok(Statement { raw, db: self })
    }

    /// Ejecuta una sentencia con parámetros posicionales (`?`) y devuelve todas las filas.
    pub fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Vec<Value>>, SqliteError> {
        let stmt = self.prepare(sql)?;
        stmt.bind(params)?;
        let mut rows = Vec::new();
        loop {
            let rc = unsafe { sqlite3_step(stmt.raw) };
            match rc {
                SQLITE_ROW => rows.push(stmt.row()),
                SQLITE_DONE => break,
                _ => return Err(self.error(rc)),
            }
        }
        Ok(rows)
    }

    /// Ejecuta una sentencia sin filas de resultado.
    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<(), SqliteError> {
        self.query(sql, params).map(|_| ())
    }
}

impl Statement<'_> {
    fn bind(&self, params: &[Value]) -> Result<(), SqliteError> {
        for (i, value) in params.iter().enumerate() {
            let idx =
                c_int::try_from(i + 1).map_err(|_| plain_error(-1, "demasiados parámetros"))?;
            let rc = unsafe {
                match value {
                    Value::Null => sqlite3_bind_null(self.raw, idx),
                    Value::Integer(v) => sqlite3_bind_int64(self.raw, idx, *v),
                    Value::Real(v) => sqlite3_bind_double(self.raw, idx, *v),
                    Value::Text(t) => {
                        let len = c_int::try_from(t.len())
                            .map_err(|_| plain_error(-1, "texto demasiado grande"))?;
                        sqlite3_bind_text(self.raw, idx, t.as_ptr().cast(), len, SQLITE_TRANSIENT)
                    }
                    Value::Blob(b) => {
                        let len = c_int::try_from(b.len())
                            .map_err(|_| plain_error(-1, "blob demasiado grande"))?;
                        sqlite3_bind_blob(self.raw, idx, b.as_ptr().cast(), len, SQLITE_TRANSIENT)
                    }
                }
            };
            if rc != SQLITE_OK {
                return Err(self.db.error(rc));
            }
        }
        Ok(())
    }

    fn row(&self) -> Vec<Value> {
        let count = unsafe { sqlite3_column_count(self.raw) };
        (0..count).map(|col| self.column(col)).collect()
    }

    fn column(&self, col: c_int) -> Value {
        unsafe {
            match sqlite3_column_type(self.raw, col) {
                SQLITE_INTEGER => Value::Integer(sqlite3_column_int64(self.raw, col)),
                SQLITE_FLOAT => Value::Real(sqlite3_column_double(self.raw, col)),
                SQLITE_TEXT => {
                    let ptr = sqlite3_column_text(self.raw, col);
                    let len = usize::try_from(sqlite3_column_bytes(self.raw, col)).unwrap_or(0);
                    if ptr.is_null() {
                        Value::text("")
                    } else {
                        let bytes = std::slice::from_raw_parts(ptr, len);
                        Value::Text(Zeroizing::new(String::from_utf8_lossy(bytes).into_owned()))
                    }
                }
                SQLITE_BLOB => {
                    let ptr = sqlite3_column_blob(self.raw, col);
                    let len = usize::try_from(sqlite3_column_bytes(self.raw, col)).unwrap_or(0);
                    if ptr.is_null() || len == 0 {
                        Value::blob(&[])
                    } else {
                        Value::blob(std::slice::from_raw_parts(ptr.cast::<u8>(), len))
                    }
                }
                _ => Value::Null,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gp-sqlite-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_insert_and_read_back_read_only() {
        let dir = temp_dir("rt");
        let path = dir.join("synthetic.db");
        {
            let db = Database::open_read_write(&path).unwrap();
            db.execute("CREATE TABLE t (a TEXT, b BLOB, c INTEGER, d REAL)", &[])
                .unwrap();
            db.execute(
                "INSERT INTO t VALUES (?, ?, ?, ?)",
                &[
                    Value::text("hola"),
                    Value::blob(&[0, 1, 2, 255]),
                    Value::Integer(-7),
                    Value::Real(1.5),
                ],
            )
            .unwrap();
            db.execute("INSERT INTO t VALUES (NULL, NULL, NULL, NULL)", &[])
                .unwrap();
        }
        let db = Database::open_read_only(&path).unwrap();
        let rows = db
            .query("SELECT a, b, c, d FROM t ORDER BY rowid", &[])
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].as_text(), "hola");
        assert_eq!(rows[0][1].as_bytes(), &[0, 1, 2, 255]);
        assert_eq!(rows[0][2].as_i64(), -7);
        assert_eq!(rows[0][3], Value::Real(1.5));
        assert_eq!(rows[1][0], Value::Null);
        // Solo lectura: escribir debe fallar.
        assert!(db.execute("INSERT INTO t VALUES (1,2,3,4)", &[]).is_err());
        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn debug_never_prints_contents() {
        let v = Value::text("contraseña-secreta");
        let shown = format!("{v:?}");
        assert!(!shown.contains("secreta"));
        let b = Value::blob(b"v10secreto");
        assert!(!format!("{b:?}").contains("secreto"));
    }

    #[test]
    fn missing_file_read_only_fails_cleanly() {
        let dir = temp_dir("missing");
        let err = Database::open_read_only(&dir.join("nope.db"));
        assert!(err.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_sql_reports_error() {
        let dir = temp_dir("bad");
        let db = Database::open_read_write(&dir.join("x.db")).unwrap();
        assert!(db.query("SELECT FROM nowhere", &[]).is_err());
        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
