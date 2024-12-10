//!
//! Faster version of the `#[sqlx::test]` macro for PostgreSQL. Database for every test
//! is created using `CREATE DATABASE ... WITH TEMPLATE ...` and dropped after test
//! is finished.
//!
//! # Usage
//!
//! ```rust
//! use sqlx_pg_test_template::test;
//!
//! #[sqlx_pg_test_template::test]
//! async fn test(pool: Postgres<Pool>) {
//!     // Do work
//! }
//!
//! #[sqlx_pg_test_template::test(template = "my_db_with_seeds")]
//! async fn test_with_seeds(pool: Postgres<Pool>) {
//!     // Do work
//! }
//!
//! #[sqlx_pg_test_template::test(max_connections=5)]
//! async fn test_with_cursor(pool: Postgres<Pool>) {
//!     // Do work
//! }
//! ```
//!
//! Run tests:
//!
//! ```sh
//! DATABASE_URL="postgres://postgres:postgres@localhost:5432/test_template" cargo test
//! ```
//!
//! `test_template` would be used as a template for test databases. Connection to the
//! default `postgres` user database (`postgres`) will be used to manage test databases.
//!
//! You need to maintain the template database(s) manually. For example, you can use
//! a `nextest` startup script:
//!
//! ```sh
//! export DATABASE_URL="postgres://postgres:postgres@localhost:5432/test_template"
//! sqlx database drop -y -D $DATABASE_URL
//! sqlx database create -D $DATABASE_URL
//! sqlx migrate run -D $DATABASE_URL --source ./migrations
//! ```
//!
//! Macro uses the same runtime as the parent sqlx through `sqlx::test_block_on`.
//!
//! # Requirements
//!
//! * The user provided in `DATABASE_URL` should have permissions
//!   to create and drop databases.
//! * The user must have a default database created and accessible.
//!   A connection to the default database is used to manipulate test databases.
//! * If no `template` argument is provided for the macro, the database
//!   from `DATABASE_URL` will be used as the template.
//!
//! # Differences from standard `#[sqlx::test]`
//!
//! * Standard macro uses master connection pool. The specific test pools share
//!   semaphore with the parent pool ensuring that the total number of connections
//!   won't exceed the connection limit.
//!
//!   Unfortunately, this API is private. Moreover, it does not help when tests
//!   are run by `nextest` because tests are run in parallel processes.
//!
//!   So, the number of connections for individual test pool is limited by 2 or
//!   `max_connections` parameter (in case a specific test needs more).
//!
//!   If you encounter any issues with the number of connections, you can use `pgBouncer`
//!   or a similar tool or just decrease number of parallel runners.
//!
//! * No extra metadata is generated. We rely on the fact that module_path to a test function
//!   is unique and calculate hash from it. This hash is used as a database name. Module path
//!   to a test is set as a comment for a database.
//!
//! # Failed tests
//!   
//! To find failed database test, you can use the following SQL:
//!
//! ```sql
//! SELECT
//!     d.datname AS database_name,
//!     sd.description AS comment
//! FROM
//!     pg_database d
//! JOIN
//!     pg_shdescription sd ON d.oid = sd.objoid
//! WHERE
//!     sd.description ILIKE '%test%';
//! ```
pub use sqlx_pg_test_template_macros::test;

#[doc(hidden)]
pub use sqlx_pg_test_template_runner::TestArgs;

#[doc(hidden)]
pub use sqlx_pg_test_template_runner::run_test;
