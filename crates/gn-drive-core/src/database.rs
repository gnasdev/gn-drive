//! Thin synchronous SQLite wrapper (port of `Database.swift`).
//!
//! All access is serialized on one `Mutex<Connection>` (SQLite single-writer).
//! Values cross the boundary as [`SqlValue`], with the same lenient accessors
//! as the Swift implementation.

use parking_lot::Mutex;
use rusqlite::types::{ToSqlOutput, Value, ValueRef};
use rusqlite::{Connection, ToSql, params_from_iter};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DatabaseError {
    #[error("{0}")]
    Open(String),
    #[error("{0}")]
    Prepare(String),
    #[error("{0}")]
    Step(String),
    #[error("{0}")]
    Bind(String),
    #[error("record not found")]
    NotFound,
    #[error("database is closed")]
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Int(i64),
    Double(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlValue {
    /// Lenient text view: numbers are stringified, null/blob → "".
    pub fn text(&self) -> String {
        match self {
            SqlValue::Text(s) => s.clone(),
            SqlValue::Int(i) => i.to_string(),
            SqlValue::Double(d) => format!("{d:?}"),
            _ => String::new(),
        }
    }

    /// Lenient integer view: doubles truncate, text parses (else 0).
    pub fn int(&self) -> i64 {
        match self {
            SqlValue::Int(i) => *i,
            SqlValue::Double(d) => *d as i64,
            SqlValue::Text(s) => s.parse::<i64>().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn double(&self) -> f64 {
        match self {
            SqlValue::Double(d) => *d,
            SqlValue::Int(i) => *i as f64,
            _ => 0.0,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, SqlValue::Null)
    }

    /// `Some(v)` → `Int(v)`, `None` → `Null`.
    pub fn from_opt_int(v: Option<i64>) -> SqlValue {
        v.map_or(SqlValue::Null, SqlValue::Int)
    }

    /// `Some(v)` → `Double(v)`, `None` → `Null`.
    pub fn from_opt_double(v: Option<f64>) -> SqlValue {
        v.map_or(SqlValue::Null, SqlValue::Double)
    }

    pub fn from_bool(b: bool) -> SqlValue {
        SqlValue::Int(i64::from(b))
    }
}

impl From<&str> for SqlValue {
    fn from(s: &str) -> Self {
        SqlValue::Text(s.to_string())
    }
}
impl From<String> for SqlValue {
    fn from(s: String) -> Self {
        SqlValue::Text(s)
    }
}
impl From<i64> for SqlValue {
    fn from(i: i64) -> Self {
        SqlValue::Int(i)
    }
}
impl From<f64> for SqlValue {
    fn from(d: f64) -> Self {
        SqlValue::Double(d)
    }
}
impl From<bool> for SqlValue {
    fn from(b: bool) -> Self {
        SqlValue::from_bool(b)
    }
}

impl ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            SqlValue::Null => ToSqlOutput::Owned(Value::Null),
            SqlValue::Int(i) => ToSqlOutput::Owned(Value::Integer(*i)),
            SqlValue::Double(d) => ToSqlOutput::Owned(Value::Real(*d)),
            SqlValue::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            SqlValue::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

fn sql_prefix(sql: &str) -> String {
    sql.chars().take(80).collect()
}

fn run_exec(conn: &Connection, sql: &str, args: &[SqlValue]) -> Result<i64, DatabaseError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| DatabaseError::Prepare(format!("{e} (sql: {})", sql_prefix(sql))))?;
    let step_err =
        |e: rusqlite::Error| DatabaseError::Step(format!("{e} (sql: {})", sql_prefix(sql)));
    // Use `query` (not `execute`) so statements such as `PRAGMA journal_mode`
    // that yield rows are accepted, like sqlite3_step in the Swift wrapper.
    let mut rows = stmt
        .query(params_from_iter(args.iter()))
        .map_err(step_err)?;
    while rows.next().map_err(step_err)?.is_some() {}
    drop(rows);
    Ok(conn.changes() as i64)
}

fn run_query(
    conn: &Connection,
    sql: &str,
    args: &[SqlValue],
) -> Result<Vec<Vec<SqlValue>>, DatabaseError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| DatabaseError::Prepare(format!("{e} (sql: {})", sql_prefix(sql))))?;
    let step_err =
        |e: rusqlite::Error| DatabaseError::Step(format!("{e} (sql: {})", sql_prefix(sql)));
    let n = stmt.column_count();
    let mut rows = stmt
        .query(params_from_iter(args.iter()))
        .map_err(step_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(step_err)? {
        let mut vals = Vec::with_capacity(n);
        for i in 0..n {
            vals.push(match row.get_ref(i).map_err(step_err)? {
                ValueRef::Null => SqlValue::Null,
                ValueRef::Integer(i) => SqlValue::Int(i),
                ValueRef::Real(d) => SqlValue::Double(d),
                ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(b) => SqlValue::Blob(b.to_vec()),
            });
        }
        out.push(vals);
    }
    Ok(out)
}

/// Statement handle valid inside [`Database::transaction`] (the connection
/// lock is already held, so `Database` methods must not be called there).
pub struct Tx<'a> {
    conn: &'a Connection,
}

impl Tx<'_> {
    pub fn exec(&self, sql: &str, args: &[SqlValue]) -> Result<i64, DatabaseError> {
        run_exec(self.conn, sql, args)
    }
    pub fn query(&self, sql: &str, args: &[SqlValue]) -> Result<Vec<Vec<SqlValue>>, DatabaseError> {
        run_query(self.conn, sql, args)
    }
    pub fn query_row(
        &self,
        sql: &str,
        args: &[SqlValue],
    ) -> Result<Option<Vec<SqlValue>>, DatabaseError> {
        Ok(run_query(self.conn, sql, args)?.into_iter().next())
    }
}

pub struct Database {
    conn: Mutex<Option<Connection>>,
    pub path: String,
}

impl Database {
    pub fn open(path: &str) -> Result<Database, DatabaseError> {
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| DatabaseError::Open(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| DatabaseError::Open(e.to_string()))?;
        let db = Database {
            conn: Mutex::new(Some(conn)),
            path: path.to_string(),
        };
        db.exec("PRAGMA journal_mode=WAL", &[])?;
        db.exec("PRAGMA foreign_keys=ON", &[])?;
        db.exec("PRAGMA busy_timeout=5000", &[])?;
        Ok(db)
    }

    pub fn close(&self) {
        // Dropping the connection closes it.
        self.conn.lock().take();
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DatabaseError>,
    ) -> Result<T, DatabaseError> {
        let guard = self.conn.lock();
        match guard.as_ref() {
            Some(c) => f(c),
            None => Err(DatabaseError::Closed),
        }
    }

    /// Execute a statement; returns the number of rows changed.
    pub fn exec(&self, sql: &str, args: &[SqlValue]) -> Result<i64, DatabaseError> {
        self.with_conn(|c| run_exec(c, sql, args))
    }

    pub fn query(&self, sql: &str, args: &[SqlValue]) -> Result<Vec<Vec<SqlValue>>, DatabaseError> {
        self.with_conn(|c| run_query(c, sql, args))
    }

    /// Query a single row or `None`.
    pub fn query_row(
        &self,
        sql: &str,
        args: &[SqlValue],
    ) -> Result<Option<Vec<SqlValue>>, DatabaseError> {
        Ok(self.query(sql, args)?.into_iter().next())
    }

    /// Run `body` inside `BEGIN IMMEDIATE … COMMIT`; rolls back on error.
    pub fn transaction<T, E>(&self, body: impl FnOnce(&Tx<'_>) -> Result<T, E>) -> Result<T, E>
    where
        E: From<DatabaseError>,
    {
        let guard = self.conn.lock();
        let conn = guard.as_ref().ok_or(DatabaseError::Closed)?;
        run_exec(conn, "BEGIN IMMEDIATE", &[])?;
        let tx = Tx { conn };
        match body(&tx) {
            Ok(v) => {
                run_exec(conn, "COMMIT", &[])?;
                Ok(v)
            }
            Err(e) => {
                let _ = run_exec(conn, "ROLLBACK", &[]);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_tmp() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("sub/dir/t.db").to_str().unwrap()).unwrap();
        (dir, db)
    }

    #[test]
    fn open_creates_parent_and_sets_pragmas() {
        let (dir, db) = open_tmp();
        assert!(dir.path().join("sub/dir/t.db").exists());
        let jm = db.query_row("PRAGMA journal_mode", &[]).unwrap().unwrap();
        assert_eq!(jm[0].text(), "wal");
        let fk = db.query_row("PRAGMA foreign_keys", &[]).unwrap().unwrap();
        assert_eq!(fk[0].int(), 1);
        let bt = db.query_row("PRAGMA busy_timeout", &[]).unwrap().unwrap();
        assert_eq!(bt[0].int(), 5000);
    }

    #[test]
    fn open_fails_when_parent_is_file() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let err = Database::open(blocker.join("a.db").to_str().unwrap());
        assert!(matches!(err, Err(DatabaseError::Open(_))));
    }

    #[test]
    fn exec_query_roundtrip_all_types() {
        let (_d, db) = open_tmp();
        db.exec(
            "CREATE TABLE t (a INTEGER, b REAL, c TEXT, d BLOB, e TEXT)",
            &[],
        )
        .unwrap();
        let n = db
            .exec(
                "INSERT INTO t VALUES (?,?,?,?,?)",
                &[
                    SqlValue::Int(7),
                    SqlValue::Double(1.5),
                    SqlValue::Text("héllo".into()),
                    SqlValue::Blob(vec![1, 2, 3]),
                    SqlValue::Null,
                ],
            )
            .unwrap();
        assert_eq!(n, 1);
        let rows = db.query("SELECT a,b,c,d,e FROM t", &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], SqlValue::Int(7));
        assert_eq!(rows[0][1], SqlValue::Double(1.5));
        assert_eq!(rows[0][2], SqlValue::Text("héllo".into()));
        assert_eq!(rows[0][3], SqlValue::Blob(vec![1, 2, 3]));
        assert!(rows[0][4].is_null());
        assert!(
            db.query_row("SELECT a FROM t WHERE a = ?", &[SqlValue::Int(99)])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn exec_reports_changes() {
        let (_d, db) = open_tmp();
        db.exec("CREATE TABLE t (a INTEGER)", &[]).unwrap();
        for i in 0..3 {
            db.exec("INSERT INTO t VALUES (?)", &[SqlValue::Int(i)])
                .unwrap();
        }
        assert_eq!(
            db.exec("DELETE FROM t WHERE a >= ?", &[SqlValue::Int(1)])
                .unwrap(),
            2
        );
        assert_eq!(
            db.exec("DELETE FROM t WHERE a >= ?", &[SqlValue::Int(5)])
                .unwrap(),
            0
        );
    }

    #[test]
    fn errors_are_classified() {
        let (_d, db) = open_tmp();
        assert!(matches!(
            db.exec("NOT SQL", &[]),
            Err(DatabaseError::Prepare(_))
        ));
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)", &[])
            .unwrap();
        db.exec("INSERT INTO t VALUES (1)", &[]).unwrap();
        let err = db.exec("INSERT INTO t VALUES (1)", &[]).unwrap_err();
        assert!(matches!(err, DatabaseError::Step(_)));
        assert!(err.to_string().contains("(sql: INSERT INTO t"));
    }

    #[test]
    fn lenient_accessors() {
        assert_eq!(SqlValue::Int(5).text(), "5");
        assert_eq!(SqlValue::Double(2.0).text(), "2.0");
        assert_eq!(SqlValue::Null.text(), "");
        assert_eq!(SqlValue::Blob(vec![1]).text(), "");
        assert_eq!(SqlValue::Double(3.9).int(), 3);
        assert_eq!(SqlValue::Text("42".into()).int(), 42);
        assert_eq!(SqlValue::Text("x".into()).int(), 0);
        assert_eq!(SqlValue::Null.int(), 0);
        assert_eq!(SqlValue::Int(2).double(), 2.0);
        assert_eq!(SqlValue::Text("2".into()).double(), 0.0);
        assert!(SqlValue::Null.is_null());
        assert!(!SqlValue::Int(0).is_null());
    }

    #[test]
    fn transaction_commits_and_rolls_back() {
        let (_d, db) = open_tmp();
        db.exec("CREATE TABLE t (a INTEGER)", &[]).unwrap();
        db.transaction::<_, DatabaseError>(|tx| {
            tx.exec("INSERT INTO t VALUES (1)", &[])?;
            tx.exec("INSERT INTO t VALUES (2)", &[])?;
            Ok(())
        })
        .unwrap();
        let r: Result<(), DatabaseError> = db.transaction(|tx| {
            tx.exec("INSERT INTO t VALUES (3)", &[])?;
            Err(DatabaseError::NotFound)
        });
        assert_eq!(r, Err(DatabaseError::NotFound));
        let rows = db.query("SELECT a FROM t ORDER BY a", &[]).unwrap();
        assert_eq!(
            rows.iter().map(|r| r[0].int()).collect::<Vec<_>>(),
            vec![1, 2]
        );
        // Connection is usable again after rollback.
        db.exec("INSERT INTO t VALUES (4)", &[]).unwrap();
    }

    #[test]
    fn closed_database_errors() {
        let (_d, db) = open_tmp();
        db.close();
        assert_eq!(db.exec("SELECT 1", &[]), Err(DatabaseError::Closed));
        assert!(db.transaction::<(), DatabaseError>(|_| Ok(())).is_err());
        db.close(); // idempotent
    }

    #[test]
    fn database_is_send_sync() {
        fn assert<T: Send + Sync>() {}
        assert::<Database>();
    }
}
