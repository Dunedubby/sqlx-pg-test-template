//! Those tests ensure that macro compiles

#[cfg(test)]
mod test {
    #[sqlx_pg_test_template::test]
    async fn test_macro_default_custom(_pool: sqlx::Pool<sqlx::Postgres>) {}

    #[sqlx_pg_test_template::test(max_connections = 5)]
    async fn test_macro_default_custom_mc(_pool: sqlx::Pool<sqlx::Postgres>) {}

    #[sqlx_pg_test_template::test(template = "postgres")]
    async fn test_macro_default_custom_mc_tpl(_pool: sqlx::Pool<sqlx::Postgres>) {}
}
