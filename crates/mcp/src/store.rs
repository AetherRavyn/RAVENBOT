use sqlx::SqlitePool;

pub struct McpStore {
    pool: SqlitePool,
}
impl McpStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
    pub async fn ensure_tables(&self) -> Result<(), String> {
        // Delegates to registry
        crate::registry::McpRegistry::new(self.pool.clone()).ensure_tables().await
    }
}
