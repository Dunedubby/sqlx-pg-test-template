#[cfg(test)]
mod test {
    #[sqlx_pg_test_template::test]
    async fn test_macro_default_custom(pool: sqlx::Pool<sqlx::Postgres>) {}
}
