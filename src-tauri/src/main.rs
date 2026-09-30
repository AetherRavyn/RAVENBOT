// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        // Own MCP server: expose RAVENBOT skills as MCP tools over stdio
        Some("mcp-serve") => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(e) = rt.block_on(ravenbot_mcp::server::run_stdio()) {
                eprintln!("ravenbot mcp-serve error: {}", e);
                std::process::exit(1);
            }
        }
        // Headless single run: ravenbot run --bot <name|id> --message "..."
        Some("run") => headless_run(args),
        // Headless bot listing: ravenbot list-bots
        Some("list-bots") => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(e) = rt.block_on(list_bots()) {
                eprintln!("ravenbot list-bots error: {}", e);
                std::process::exit(1);
            }
        }
        Some("--help") | Some("-h") => print_cli_help(),
        // Default: the desktop app
        _ => ravenbot_lib::run(),
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

fn print_cli_help() {
    println!(
        "RAVENBOT — sovereign multi-agent desktop OS\n\
\n\
Usage:\n\
  ravenbot                    Launch the desktop app\n\
  ravenbot mcp-serve          Run RAVENBOT as an MCP server over stdio\n\
  ravenbot run --bot NAME --message \"TEXT\"\n\
                              Headless single run (name or bot id)\n\
  ravenbot list-bots          List bots\n\n\
Database: RAVENBOT_DB env overrides the default app-data path.\n\
Kill switch (headless): set RAVENBOT_KILL_SWITCH=1 to pause tool calls."
    );
}

async fn list_bots() -> Result<(), String> {
    let db = open_db().await?;
    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
        .await
        .map_err(|e| e.to_string())?;
    for bot in &bots {
        println!(
            "{}\t{}\t{}\t{}",
            bot.id,
            bot.name,
            bot.config.model_provider,
            bot.config.model_id
        );
    }
    Ok(())
}

async fn open_db() -> Result<ravenbot_db::Database, String> {
    let db_path = ravenbot_mcp::server::default_db_path();
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    ravenbot_db::Database::new(&db_path)
        .await
        .map_err(|e| format!("Failed to open database at {:?}: {}", db_path, e))
}

/// Headless single run: create a thread, send, execute, print the response.
fn headless_run(args: Vec<String>) {
    let mut bot_arg: Option<String> = None;
    let mut message = String::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bot" | "-b" => {
                i += 1;
                bot_arg = args.get(i).cloned();
            }
            "--message" | "-m" => {
                i += 1;
                message = args.get(i).cloned().unwrap_or_default();
            }
            _ => {}
        }
        i += 1;
    }

    if message.trim().is_empty() {
        eprintln!("ravenbot run: --message is required (try --help)");
        std::process::exit(2);
    }

    init_tracing();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = rt.block_on(async move {
        let db = open_db().await?;
        let runtime = Arc::new(ravenbot_runtime::Runtime::new(db.clone()));

        let bots = ravenbot_db::queries::BotQueries::list(db.pool())
            .await
            .map_err(|e| e.to_string())?;
        let bot = match &bot_arg {
            Some(arg) => bots
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(arg) || b.id.to_string() == *arg)
                .cloned(),
            None => bots.first().cloned(),
        }
        .ok_or_else(|| match bot_arg {
            Some(arg) => format!("Bot not found: {}", arg),
            None => "No bots exist yet — create one in the desktop app first".to_string(),
        })?;

        let thread = ravenbot_core::Thread::new(
            bot.id,
            format!("CLI: {}", message.chars().take(30).collect::<String>()),
        );
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .map_err(|e| e.to_string())?;

        let user_msg = ravenbot_core::Message::user(thread.id, &message);
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
            .await
            .map_err(|e| e.to_string())?;

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .map_err(|e| e.to_string())?;

        runtime
            .execute_run(&mut run)
            .await
            .map_err(|e| e.to_string())?;

        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .map_err(|e| e.to_string())?;

        let response = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, ravenbot_core::MessageRole::Assistant))
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                ravenbot_core::MessageContent::Checklist { text, items } => {
                    let list: Vec<String> = items
                        .iter()
                        .map(|i| format!("{} {}", match i.status {
                            ravenbot_core::ChecklistStatus::Completed => "✓",
                            ravenbot_core::ChecklistStatus::Failed => "✗",
                            _ => "○",
                        }, i.label))
                        .collect();
                    Some(text.clone().map_or_else(|| list.join("\n"), |t| format!("{}\n{}", t, list.join("\n"))))
                }
                _ => None,
            })
            .ok_or_else(|| "No response generated".to_string())?;

        Ok::<String, String>(response)
    });

    match result {
        Ok(response) => println!("{}", response),
        Err(e) => {
            eprintln!("ravenbot run error: {}", e);
            std::process::exit(1);
        }
    }
}
