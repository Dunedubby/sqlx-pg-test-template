# sqlx_pg_test_template

Faster version of the `#[sqlx::test]` macro for PostgreSQL. Database for every test
is created using `CREATE DATABASE ... WITH TEMPLATE ...` and dropped after test is finished.
