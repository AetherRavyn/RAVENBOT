//! Install a built-in job as a real agent + paused routine.
//!
//! The catalog lives in `ravenbot_core::job` and is static. This module owns
//! the database side: it creates the agent, creates the routine **disabled in
//! its first row**, and leaves a visible welcome thread that explains what was
//! installed and what is still paused.
//!
//! It is deliberately separate from the Tauri commands so the whole install
//! path can be tested against a real SQLite database without a window.

use ravenbot_core::{Bot, JobCatalog, JobOversight, JobTemplate, Message, Routine, Thread};
use ravenbot_db::queries::{BotQueries, MessageQueries, ThreadQueries};
use ravenbot_scheduler::routine::RoutineManager;
use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

/// User choices layered over the template defaults.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Name for the installed agent. Defaults to the job's role title.
    pub name: Option<String>,
    /// Cron expression or `@manual`. Defaults to the template schedule.
    pub schedule: Option<String>,
    /// Interactive oversight. Defaults to the template level.
    pub oversight: Option<JobOversight>,
    /// Provider for the new agent. Defaults to the app's resolved model.
    pub provider: Option<String>,
    /// Model id for the new agent. Defaults to the app's resolved model.
    pub model: Option<String>,
}

/// What the UI needs after a successful install.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledJob {
    pub job_id: String,
    pub bot: Bot,
    pub routine: Routine,
    pub thread_id: Uuid,
    /// Another agent was already installed from this template. Installing a
    /// second one is allowed (two researchers, two offices), but the UI should
    /// say so rather than pretend this is the first.
    pub duplicate: bool,
}

/// Install one catalog job.
pub async fn install_job(
    pool: &SqlitePool,
    job_id: &str,
    options: InstallOptions,
) -> Result<InstalledJob, String> {
    let job = JobCatalog::find(job_id).ok_or_else(|| format!("Unknown job template '{job_id}'"))?;
    job.validate().map_err(|error| error.to_string())?;

    let name = options
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(job.role_title)
        .to_string();
    if name.chars().count() > 80 {
        return Err("Agent name is too long (80 characters maximum)".to_string());
    }

    let schedule = options
        .schedule
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(job.schedule)
        .to_string();
    // The scheduler owns the real grammar. Validate here so a bad override is
    // rejected before a bot exists, rather than leaving a half-installed job.
    ravenbot_scheduler::cron::CronParser::parse(&schedule)
        .map_err(|error| format!("Invalid schedule: {error}"))?;

    let oversight = options.oversight.unwrap_or(job.oversight);

    let existing = BotQueries::list(pool).await.map_err(|e| e.to_string())?;
    let duplicate = existing
        .iter()
        .any(|bot| bot.config.job_id.as_deref() == Some(job.id));

    let mut bot = Bot::new(&name, job.summary);
    bot.specialty = Some(job.role_title.to_string());
    bot.config.custom_prompt = Some(job.agent_prompt.to_string());
    bot.config.job_id = Some(job.id.to_string());
    bot.approval_mode = oversight.approval_mode();
    if let Some(provider) = options
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bot.config.model_provider = provider.to_string();
    }
    if let Some(model) = options
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bot.config.model_id = model.to_string();
    }

    BotQueries::insert(pool, &bot)
        .await
        .map_err(|e| format!("Could not create the agent: {e}"))?;

    // If the routine fails to insert, remove the agent so a failed install is
    // either complete or absent — never a bot with no job behind it.
    let manager = RoutineManager::new(pool.clone());
    let routine = match manager
        .create_paused(bot.id, job.name, &schedule, job.routine_instruction)
        .await
    {
        Ok(routine) => routine,
        Err(error) => {
            let _ = BotQueries::delete(pool, bot.id).await;
            return Err(format!("Could not create the routine: {error}"));
        }
    };

    // The routine is already paused; record the template's description.
    let mut described = routine.clone();
    described.description = job.summary.to_string();
    described.updated_at = chrono::Utc::now();
    if let Err(error) = manager.update(&described).await {
        let _ = manager.delete(routine.id).await;
        let _ = BotQueries::delete(pool, bot.id).await;
        return Err(format!("Could not finish the routine: {error}"));
    }

    let thread = Thread::new(bot.id, format!("Getting started: {}", job.name));
    if let Err(error) = ThreadQueries::create(pool, &thread).await {
        let _ = manager.delete(routine.id).await;
        let _ = BotQueries::delete(pool, bot.id).await;
        return Err(format!("Could not create the welcome thread: {error}"));
    }

    let welcome = Message::assistant_from_bot(
        thread.id,
        bot.id,
        bot.name.clone(),
        welcome_text(job, &schedule, oversight),
    );
    if let Err(error) = MessageQueries::insert(pool, &welcome).await {
        let _ = ThreadQueries::delete(pool, thread.id).await;
        let _ = manager.delete(routine.id).await;
        let _ = BotQueries::delete(pool, bot.id).await;
        return Err(format!("Could not write the welcome message: {error}"));
    }

    Ok(InstalledJob {
        job_id: job.id.to_string(),
        bot,
        routine: described,
        thread_id: thread.id,
        duplicate,
    })
}

fn welcome_text(job: &JobTemplate, schedule: &str, oversight: JobOversight) -> String {
    let cadence = match schedule {
        "@manual" => "Manual — use **Run now** in Routines.".to_string(),
        cron => format!("Scheduled: `{cron}`."),
    };
    format!(
        "I'm installed for **{}** — {}.\n\n\
         **What runs:** {}\n\n\
         **Deliverable:** {}\n\n\
         **Schedule:** {cadence}\n\n\
         **Oversight:** {}. Unattended runs still park high-risk tools for your approval.\n\n\
         The routine is **paused**. Open Routines to read the instruction and enable it when you are ready, \
         or press **Run now** for a single run.",
        job.name,
        job.role_title,
        job.routine_instruction,
        job.deliverable,
        oversight.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::{ApprovalMode, MessageContent};

    async fn temp_db(tag: &str) -> (ravenbot_db::Database, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("ravenbot-jobs-{tag}-{}.db", uuid::Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path)
            .await
            .expect("temp database");
        (db, path)
    }

    fn message_text(message: &Message) -> String {
        match &message.content {
            MessageContent::Text { text, .. } => text.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        }
    }

    #[test]
    fn every_shipped_schedule_matches_the_scheduler_grammar() {
        for job in JobCatalog::all() {
            if job.schedule == "@manual" {
                assert_eq!(
                    job.cadence,
                    ravenbot_core::JobCadence::OneOff,
                    "{} is manual but not one-off",
                    job.id
                );
            } else {
                ravenbot_scheduler::cron::CronParser::parse(job.schedule)
                    .unwrap_or_else(|error| panic!("{}: {error}", job.id));
            }
        }
    }

    #[tokio::test]
    async fn install_creates_a_paused_routine_and_a_visible_welcome_thread() {
        let (db, _path) = temp_db("basic").await;
        let installed = install_job(db.pool(), "morning-brief", InstallOptions::default())
            .await
            .expect("install");

        assert_eq!(installed.job_id, "morning-brief");
        assert_eq!(installed.bot.name, "Chief of staff");
        assert_eq!(
            installed.bot.config.job_id.as_deref(),
            Some("morning-brief")
        );
        assert_eq!(installed.bot.specialty.as_deref(), Some("Chief of staff"));
        assert_eq!(installed.bot.approval_mode, ApprovalMode::Full);
        assert!(installed
            .bot
            .config
            .custom_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("chief of staff"));
        assert_eq!(installed.routine.schedule, "30 7 * * 1-5");
        assert!(
            !installed.routine.is_enabled,
            "installed routines start paused"
        );
        assert_eq!(
            installed.routine.description,
            "The few things that matter today, in one screen."
        );
        assert!(!installed.duplicate);

        // The paused flag is in the first stored row, not just the return value.
        let stored: (i64,) = sqlx::query_as("SELECT is_enabled FROM routines WHERE id = ?")
            .bind(installed.routine.id.to_string())
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(stored.0, 0);

        let threads = ThreadQueries::list_visible_by_bot(db.pool(), installed.bot.id)
            .await
            .unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, installed.thread_id);

        let messages = MessageQueries::list_by_thread(db.pool(), installed.thread_id)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        let text = message_text(&messages[0]);
        assert!(text.contains("paused"), "{text}");
        assert!(text.contains("Run now"), "{text}");
        assert!(text.contains("30 7 * * 1-5"), "{text}");
    }

    #[tokio::test]
    async fn install_honours_name_schedule_oversight_and_model_overrides() {
        let (db, _path) = temp_db("overrides").await;
        let installed = install_job(
            db.pool(),
            "blog-draft",
            InstallOptions {
                name: Some("Ada".to_string()),
                schedule: Some("0 9 * * 1".to_string()),
                oversight: Some(JobOversight::Approval),
                provider: Some("ollama".to_string()),
                model: Some("llama3.2".to_string()),
            },
        )
        .await
        .expect("install");

        assert_eq!(installed.bot.name, "Ada");
        assert_eq!(installed.bot.approval_mode, ApprovalMode::Ask);
        assert_eq!(installed.bot.config.model_provider, "ollama");
        assert_eq!(installed.bot.config.model_id, "llama3.2");
        assert_eq!(installed.routine.schedule, "0 9 * * 1");
        assert!(!installed.routine.is_enabled);
    }

    #[tokio::test]
    async fn a_one_off_job_installs_a_manual_routine() {
        let (db, _path) = temp_db("manual").await;
        let installed = install_job(db.pool(), "travel-plan", InstallOptions::default())
            .await
            .expect("install");
        assert_eq!(installed.routine.schedule, "@manual");
        assert!(!installed.routine.is_enabled);
        ravenbot_scheduler::cron::CronParser::parse("@manual").expect("@manual is valid");
    }

    #[tokio::test]
    async fn a_second_install_is_allowed_and_flagged() {
        let (db, _path) = temp_db("duplicate").await;
        let first = install_job(db.pool(), "deep-research", InstallOptions::default())
            .await
            .expect("first install");
        let second = install_job(
            db.pool(),
            "deep-research",
            InstallOptions {
                name: Some("Second researcher".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("second install");
        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(BotQueries::list(db.pool()).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_bad_request_leaves_no_half_installed_agent() {
        let (db, _path) = temp_db("bad").await;
        assert!(
            install_job(db.pool(), "not-a-job", InstallOptions::default())
                .await
                .is_err()
        );
        assert!(install_job(
            db.pool(),
            "morning-brief",
            InstallOptions {
                schedule: Some("every morning".to_string()),
                ..Default::default()
            },
        )
        .await
        .is_err());
        assert!(install_job(
            db.pool(),
            "morning-brief",
            InstallOptions {
                name: Some("x".repeat(81)),
                ..Default::default()
            },
        )
        .await
        .is_err());
        assert!(
            BotQueries::list(db.pool()).await.unwrap().is_empty(),
            "a rejected install must not leave a bot behind"
        );
        let routines: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routines")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(routines.0, 0);
    }
}
