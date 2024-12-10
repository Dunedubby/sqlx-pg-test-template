use std::hash::Hasher;
use std::str::FromStr;

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection, PgConnection, Pool, Postgres,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("DATABASE_URL is missing or invalid")]
    InvalidDatabaseUrl,

    #[error("database not found for an open connection pool")]
    DatabaseNotFound,

    #[error("'postgres' database can not act as a template")]
    InvalidTemplate,

    #[error("sqlx error: '{0}'")]
    Sqlx(#[from] sqlx::Error),
}

/// Individual test arguments
pub struct TestArgs {
    /// Template database name
    pub template_name: Option<String>,

    /// Max connections for this pool (1 by default)
    pub max_connections: Option<u32>,

    /// Test module path
    pub module_path: String,
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

    sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
        .execute(&mut conn)
        .await?;

    sqlx::query(&format!(
        "CREATE DATABASE {} WITH TEMPLATE {}",
        db_name, template_db_name
    ))
    .execute(&mut conn)
    .await?;

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
        .max_connections(max_connections.unwrap_or(1))
        .idle_timeout(Some(std::time::Duration::from_secs(1)))
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

    pool.close().await;

    sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
        .execute(conn)
        .await?;

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

    let template_name = &args
        .template_name
        .map(Ok)
        .unwrap_or_else(|| db_name_of_test_pool(&connect_opts))?;

    if connect_opts.get_database() == Some("postgres") {
        return Err(Error::InvalidTemplate);
    }

    let conn = PgConnection::connect_with(&connect_opts).await.unwrap();
    let (db_name, conn) = create_db_from_template(conn, template_name, &args.module_path)
        .await
        .unwrap();
    conn.close().await?;

    let pool = spawn_test_pool(&connect_opts, &db_name, args.max_connections).await?;

    f(pool.clone()).await;

    let mut conn = PgConnection::connect_with(&connect_opts).await?;
    close_test_pool(&mut conn, &pool).await.unwrap();
    conn.close().await?;
    drop(pool);

    Ok(())
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
