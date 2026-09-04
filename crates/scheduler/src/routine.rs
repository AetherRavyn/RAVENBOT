//! Routine definition and management

use chrono::{Utc, Timelike};
use ravenbot_core::Routine;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Routine manager
pub struct RoutineManager {
    pool: SqlitePool,
}

impl RoutineManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new routine
    pub async fn create(
        &self,
        bot_id: Uuid,
        name: &str,
        schedule: &str,
        instruction: &str,
    ) -> Result<Routine, String> {
        // Validate cron expression
        crate::cron::CronParser::parse(schedule)?;

        let routine = Routine::new(bot_id, name, schedule, instruction);

        sqlx::query(
            "INSERT INTO routines (id, bot_id, name, description, schedule, instruction, is_enabled, last_run_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(routine.id.to_string())
        .bind(routine.bot_id.to_string())
        .bind(&routine.name)
        .bind(&routine.description)
        .bind(&routine.schedule)
        .bind(&routine.instruction)
        .bind(routine.is_enabled)
        .bind(routine.last_run_at.map(|dt| dt.to_rfc3339()))
        .bind(routine.created_at.to_rfc3339())
        .bind(routine.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::info!(routine_id = %routine.id, "Routine created");
        Ok(routine)
    }

    /// Update a routine
    pub async fn update(&self, routine: &Routine) -> Result<(), String> {
        sqlx::query(
            "UPDATE routines SET name = ?, description = ?, schedule = ?, instruction = ?, is_enabled = ?, updated_at = ?
             WHERE id = ?"
        )
        .bind(&routine.name)
        .bind(&routine.description)
        .bind(&routine.schedule)
        .bind(&routine.instruction)
        .bind(routine.is_enabled)
        .bind(routine.updated_at.to_rfc3339())
        .bind(routine.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Delete a routine
    pub async fn delete(&self, routine_id: Uuid) -> Result<(), String> {
        sqlx::query("DELETE FROM routines WHERE id = ?")
            .bind(routine_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Get a routine by ID
    pub async fn get(&self, routine_id: Uuid) -> Result<Option<Routine>, String> {
        let row: Option<(String, String, String, String, String, String, bool, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, name, description, schedule, instruction, is_enabled, last_run_at, created_at, updated_at
             FROM routines WHERE id = ?"
        )
        .bind(routine_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(row.and_then(|row| {
            Some(Routine {
                id: Uuid::parse_str(&row.0).ok()?,
                bot_id: Uuid::parse_str(&row.1).ok()?,
                name: row.2.clone(),
                description: row.3.clone(),
                schedule: row.4.clone(),
                instruction: row.5.clone(),
                is_enabled: row.6,
                last_run_at: row.7.as_ref().and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                }),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.8)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.9)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        }))
    }

    /// Get all routines for a bot
    pub async fn list_for_bot(&self, bot_id: Uuid) -> Result<Vec<Routine>, String> {
        let rows: Vec<(String, String, String, String, String, String, bool, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, name, description, schedule, instruction, is_enabled, last_run_at, created_at, updated_at
             FROM routines WHERE bot_id = ?
             ORDER BY name"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let routines = rows.iter()
            .filter_map(|row| {
                Some(Routine {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    name: row.2.clone(),
                    description: row.3.clone(),
                    schedule: row.4.clone(),
                    instruction: row.5.clone(),
                    is_enabled: row.6,
                    last_run_at: row.7.as_ref().and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.8)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.9)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .collect();

        Ok(routines)
    }

    /// Get all enabled routines
    pub async fn list_enabled(&self) -> Result<Vec<Routine>, String> {
        let rows: Vec<(String, String, String, String, String, String, bool, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, name, description, schedule, instruction, is_enabled, last_run_at, created_at, updated_at
             FROM routines WHERE is_enabled = 1"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let routines = rows.iter()
            .filter_map(|row| {
                Some(Routine {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    name: row.2.clone(),
                    description: row.3.clone(),
                    schedule: row.4.clone(),
                    instruction: row.5.clone(),
                    is_enabled: row.6,
                    last_run_at: row.7.as_ref().and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.8)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.9)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .collect();

        Ok(routines)
    }

    /// Mark a routine as executed
    pub async fn mark_executed(&self, routine_id: Uuid) -> Result<(), String> {
        sqlx::query(
            "UPDATE routines SET last_run_at = ?, updated_at = ? WHERE id = ?"
        )
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(routine_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Check which routines should run now
    pub async fn check_schedules(&self) -> Result<Vec<Routine>, String> {
        let enabled = self.list_enabled().await?;
        let now = Utc::now();
        
        let mut due = Vec::new();
        
        for routine in enabled {
            if let Ok(expr) = crate::cron::CronParser::parse(&routine.schedule) {
                // Check if routine was already executed this minute
                let already_run = routine.last_run_at
                    .map(|last| last.date_naive() == now.date_naive() && last.hour() == now.hour() && last.minute() == now.minute())
                    .unwrap_or(false);
                
                if !already_run && crate::cron::CronParser::matches(&expr, &now) {
                    due.push(routine);
                }
            }
        }
        
        Ok(due)
    }
}
