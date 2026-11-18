use std::any::Any;
use std::future::Future;
use std::hash::Hasher;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::Duration;

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection,
    PgConnection,
    Pool,
    Postgres,
    Error as SqlxError,
    __rt::{sleep, timeout},
};

/// Number of retries for the service connection and for database creation
const NUM_CREATE_RETRIES: usize = 20;
const RETRY_DELAY_MS: u64 = 500;

/// How long a test pool gets to close before its database is dropped anyway
const CLOSE_TIMEOUT_SECS: u64 = 10;

/// Default size of a test pool
const DEFAULT_MAX_CONNECTIONS: u32 = 5;

/// Advisory lock key serialising `CREATE DATABASE ... WITH TEMPLATE`. Advisory locks are scoped
/// to the database of the session that takes them; every service connection uses the same
/// default database, so every test process pointed at the same server shares this lock.
const CLONE_LOCK_KEY: i64 = 0x73716c785f746d70; // "sqlx_tmp"

/// SQLSTATE `object_in_use`: "source database ... is being accessed by other users"
const OBJECT_IN_USE: &str = "55006";

/// SQLSTATE `insufficient_privilege`: "permission denied to terminate process"
const INSUFFICIENT_PRIVILEGE: &str = "42501";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("DATABASE_URL is missing or invalid")]
    InvalidDatabaseUrl,

    #[error("database not found for an open connection pool")]
    DatabaseNotFound,

    #[error("sqlx error: '{0}'")]
    Sqlx(#[from] sqlx::Error),
}

/// Individual test arguments
pub struct TestArgs {
    /// Template database name
    pub template_name: Option<String>,

    /// Max connections for this pool (5 by default)
    pub max_connections: Option<u32>,

    /// Test module path
    pub module_path: String,
}

fn is_template_in_use(e: &SqlxError) -> bool {
    matches!(e, SqlxError::Database(db) if db.code().as_deref() == Some(OBJECT_IN_USE))
}

/// A forced drop terminates every session in the database, and fails if one of them belongs to
/// a role this one may not signal. TimescaleDB's scheduler is such a session: it is told to stop
/// when the drop is analysed, but may still be exiting when the drop looks.
fn is_session_still_leaving(e: &SqlxError) -> bool {
    matches!(
        e,
        SqlxError::Database(db)
            if matches!(db.code().as_deref(), Some(OBJECT_IN_USE) | Some(INSUFFICIENT_PRIVILEGE))
    )
}

/// Drops a database and every session in it, retrying while a session is on its way out. Sent as
/// a simple query for the same reason as the clone below.
async fn drop_database(conn: &mut PgConnection, db_name: &str) -> Result<(), SqlxError> {
    let drop = format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", db_name);

    let mut i: usize = 0;
    loop {
        match sqlx::raw_sql(&drop).execute(&mut *conn).await {
            Ok(_) => return Ok(()),
            Err(e) if is_session_still_leaving(&e) && i + 1 < NUM_CREATE_RETRIES => {
                i += 1;
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Clones the template, retrying while another session is connected to it. Postgres refuses to
/// copy a database that has any other session in it (a background worker counts), and only waits
/// five seconds for that session to leave. On a TimescaleDB template the clone races the
/// scheduler that TimescaleDB restarts for it, and loses now and then even when clones are serial.
async fn clone_template(
    conn: &mut PgConnection,
    db_name: &str,
    template_db_name: &str,
) -> Result<(), SqlxError> {
    let create = format!(
        "CREATE DATABASE {} WITH TEMPLATE {}",
        db_name, template_db_name
    );

    let mut i: usize = 0;
    loop {
        // Sent as a simple query on purpose. `sqlx::query` caches a prepared statement per
        // connection, and a retry of a cached statement skips parse analysis. TimescaleDB stops
        // its background scheduler on the template from a parse-analysis hook, so a cached retry
        // would leave that scheduler connected and be refused every time.
        let result = sqlx::raw_sql(&create).execute(&mut *conn).await;

        match result {
            Ok(_) => return Ok(()),
            Err(e) if is_template_in_use(&e) && i + 1 < NUM_CREATE_RETRIES => {
                i += 1;
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Creates a new database from a template
pub async fn create_db_from_template(
    mut conn: PgConnection,
    template_db_name: &str,
    module_path: &str,
) -> Result<(String, PgConnection), Error> {
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(module_path.as_bytes());
    let id = hasher.finish();

    let db_name = format!("_sqlx_{}", id);

    // A database left behind by an aborted run can still have sessions in it
    drop_database(&mut conn, &db_name).await?;

    // One clone of the template at a time, across threads and processes. The lock is held for
    // the copy only; the test itself runs on its own database, in parallel with the others.
    // If this connection dies, the server releases the lock with the session.
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(CLONE_LOCK_KEY)
        .execute(&mut conn)
        .await?;

    let cloned = clone_template(&mut conn, &db_name, template_db_name).await;

    let unlocked = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(CLONE_LOCK_KEY)
        .execute(&mut conn)
        .await;

    cloned?;
    unlocked?;

    sqlx::query(&format!(
        "COMMENT ON DATABASE {} IS '{}'",
        db_name, module_path
    ))
    .execute(&mut conn)
    .await?;

    Ok((db_name, conn))
}

/// Spawns test pool with a new database
pub async fn spawn_test_pool(
    connect_options: &PgConnectOptions,
    db_name: &str,
    max_connections: Option<u32>,
) -> Result<Pool<Postgres>, Error> {
    let connect_options = connect_options.clone().database(db_name);
    let pool = PgPoolOptions::new()
        .max_connections(max_connections.unwrap_or(DEFAULT_MAX_CONNECTIONS))
        .idle_timeout(Some(Duration::from_secs(1)))
        .connect_with(connect_options)
        .await?;

    Ok(pool)
}

/// Returns the name of the database for the test pool or error
pub fn db_name_of_test_pool(connect_opts: &PgConnectOptions) -> Result<String, Error> {
    connect_opts
        .get_database()
        .map(|s| s.to_string())
        .ok_or(Error::DatabaseNotFound)
}

/// Closes test pool and drops the test database
pub async fn close_test_pool(
    conn: &mut PgConnection,
    pool: &sqlx::Pool<Postgres>,
) -> Result<(), Error> {
    let db_name = db_name_of_test_pool(&pool.connect_options())?;

    // `close()` waits for every connection to come back. A test that leaked one (a spawned task
    // still holding it, say) must not hang the run: the drop below is forced either way.
    let close_timed_out = timeout(Duration::from_secs(CLOSE_TIMEOUT_SECS), pool.close())
        .await
        .is_err();
    if close_timed_out {
        eprintln!("test database {db_name}: pool still held after the test, dropping it anyway");
    }

    drop_database(conn, &db_name).await?;

    Ok(())
}

async fn retry_connect(connect_opts: &PgConnectOptions) -> Result<PgConnection, SqlxError> {
    let mut i: usize = 0;
    loop {
        let result = PgConnection::connect_with(connect_opts).await;
        if result.is_ok() {
            return result;
        }

        sleep(Duration::from_millis(RETRY_DELAY_MS)).await;

        i += 1;
        if i >= NUM_CREATE_RETRIES {
            return result;
        }
    }
}

/// Resolves to the future's output, or to the payload of a panic raised while polling it
struct CatchUnwind<Fut>(Pin<Box<Fut>>);

impl<Fut: Future> Future for CatchUnwind<Fut> {
    type Output = Result<Fut::Output, Box<dyn Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match catch_unwind(AssertUnwindSafe(|| self.0.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Err(panic) => Poll::Ready(Err(panic)),
        }
    }
}

/// Closes the pool and drops the test database over a fresh service connection
async fn drop_test_db(
    service_connect_opts: &PgConnectOptions,
    pool: &Pool<Postgres>,
) -> Result<(), Error> {
    let mut conn = retry_connect(service_connect_opts).await?;
    close_test_pool(&mut conn, pool).await?;
    conn.close().await?;

    Ok(())
}

/// Runs an individual test
pub async fn wrap_run_test<F, Fut>(f: F, args: TestArgs) -> Result<(), Error>
where
    F: Fn(Pool<Postgres>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    // Get connection string
    let database_url = std::env::var("DATABASE_URL").map_err(|_| Error::InvalidDatabaseUrl)?;

    // Try to get template database name from args, defaulting to connection database name
    let connect_opts = PgConnectOptions::from_str(&database_url)?;

    // Get template name from args or use database name from connection options
    let template_name = &args
        .template_name
        .map(Ok)
        .unwrap_or_else(|| db_name_of_test_pool(&connect_opts))?;

    // Service connection == default database (postgres, username, etc)
    let default_database_name = std::env::var("DEFAULT_DATABASE_NAME")
        .unwrap_or("postgres".to_string());
    let service_connect_opts = connect_opts.clone().database(default_database_name.as_str());

    // Create a new database from the template
    let conn = retry_connect(&service_connect_opts).await?;
    let (db_name, conn) = create_db_from_template(conn, template_name, &args.module_path).await?;
    conn.close().await?;

    // Run test. A panic is held until the database is gone, then raised again, so a failing
    // test cleans up after itself exactly like a passing one.
    let pool = spawn_test_pool(&service_connect_opts, &db_name, args.max_connections).await?;

    let outcome = CatchUnwind(Box::pin(f(pool.clone()))).await;

    // Close the pool & drop the test database
    let dropped = drop_test_db(&service_connect_opts, &pool).await;

    match outcome {
        Ok(()) => dropped,
        Err(panic) => {
            if let Err(e) = dropped {
                eprintln!("test database {db_name} was not dropped: {e}");
            }
            resume_unwind(panic)
        }
    }
}

/// Runs an individual test
pub fn run_test<F, Fut>(f: F, args: TestArgs)
where
    F: Fn(Pool<Postgres>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    sqlx::test_block_on(async move {
        match wrap_run_test(f, args).await {
            Err(e) => panic!("test failed: {e}"),
            Ok(v) => v,
        }
    })
}
