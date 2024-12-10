# sqlx_pg_test_template

Faster version of the `#[sqlx::test]` macro for PostgreSQL. Database for every test
is created using `CREATE DATABASE ... WITH TEMPLATE ...` and dropped after test is finished.

## Usage

```rust
use sqlx_pg_test_template::test;

#[sqlx_pg_test_template::test]
async fn test(pool: Postgres<Pool>) {
    // Do work
}

#[sqlx_pg_test_template::test(template = "my_db_with_seeds")]
async fn test_with_seeds(pool: Postgres<Pool>) {
    // Do work
}

#[sqlx_pg_test_template::test(max_connections=5)]
async fn test_with_cursor(pool: Postgres<Pool>) {
    // Do work
}
```

Run tests:

```sh
DATABASE_URL="postgres://postgres:postgres@localhost:5432/test_template" cargo test
```

Check [documentation](https://docs.rs/sqlx-pg-test-template/latest/sqlx_pg_test_template/) for details.