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
        // Background service management, for headless installs and diagnosis.
        Some("service") => service_command(&args),
        // Headless fleet daemon. No window, no display, no Tauri runtime.
        Some("daemon") => daemon_command(),
        // Headless administration for a daemon that owns the fleet.
        Some("remote") => remote_command(&args),
        // Native Telegram connector administration.
        Some("telegram") => telegram_command(&args),
        // Native Slack connector administration.
        Some("slack") => slack_command(&args),
        // Native Discord connector administration.
        Some("discord") => discord_command(&args),
        // Native WhatsApp Cloud API connector administration.
        Some("whatsapp") => whatsapp_command(&args),
        // Native Microsoft Teams connector administration.
        Some("teams") => teams_command(&args),
        // Local account registry administration.
        Some("account") => account_command(&args),
        // Service/autostart mode: run the full app with no visible window.
        Some("--hidden") | Some("--background") => {
            std::env::set_var("RAVENBOT_START_HIDDEN", "1");
            run_desktop();
        }
        Some("--help") | Some("-h") => print_cli_help(),
        // Default: the desktop app
        _ => run_desktop(),
    }
}

/// Launch the desktop shell, but only after the Vite dev server is reachable.
///
/// Running the debug binary directly instead of `npm run tauri dev` leaves the
/// webview pointing at `http://localhost:1420` with nothing listening. WebKit
/// then shows its connection-error page, which has no valid Origin for Tauri
/// IPC. Fail before that with the command a developer should run instead.
fn run_desktop() {
    if tauri::is_dev() && !dev_server_reachable() {
        eprintln!(
            "RAVENBOT's Vite dev server is not running at http://localhost:1420.\n\
             Start the app with `npm run tauri dev` (or run `npm run dev` in another terminal)."
        );
        std::process::exit(1);
    }
    ravenbot_lib::run(tauri::generate_context!());
}

fn dev_server_reachable() -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::{Duration, Instant};

    fn connects() -> bool {
        let Ok(mut addresses) = "127.0.0.1:1420".to_socket_addrs() else {
            return false;
        };
        addresses
            .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok())
    }

    // `tauri dev` starts Vite and usually waits for it, but giving the dev
    // server a short grace period avoids a race on slow first builds.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if connects() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn daemon_command() {
    init_tracing();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    if let Err(error) = runtime.block_on(ravenbot_lib::daemon::run()) {
        eprintln!("ravenbot daemon error: {error}");
        std::process::exit(1);
    }
}

fn telegram_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            match action {
                "status" => {
                    let status = ravenbot_lib::telegram::status(&db).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    );
                }
                "token" => {
                    use std::io::Read;
                    let mut token = String::new();
                    std::io::stdin()
                        .read_to_string(&mut token)
                        .map_err(|error| error.to_string())?;
                    ravenbot_lib::telegram::set_token(&db, &token).await?;
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::telegram::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Telegram token stored and connector enabled.");
                }
                "enable" => {
                    if ravenbot_lib::telegram::read_token(&db).await.is_none() {
                        return Err("Store a token first with `ravenbot telegram token`.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::telegram::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Telegram connector enabled.");
                }
                "disable" => {
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::telegram::ENABLED_KEY,
                        "0",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Telegram connector disabled.");
                }
                "pair" => {
                    let bot_arg = args
                        .iter()
                        .position(|arg| arg == "--bot")
                        .and_then(|index| args.get(index + 1));
                    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                    let bot = match bot_arg {
                        Some(value) => bots
                            .iter()
                            .find(|bot| {
                                bot.name.eq_ignore_ascii_case(value)
                                    || bot.id.to_string() == *value
                            })
                            .cloned(),
                        None => ravenbot_db::queries::AppSettingsQueries::get(
                            db.pool(),
                            ravenbot_lib::telegram::DEFAULT_BOT_KEY,
                        )
                        .await
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            bots.iter()
                                .find(|bot| bot.id.to_string() == value)
                                .cloned()
                        })
                        .or_else(|| bots.first().cloned()),
                    }
                    .ok_or_else(|| "No agent found. Pass --bot NAME or create an agent first.".to_string())?;
                    if !ravenbot_lib::telegram::enabled(&db).await {
                        return Err("Enable the Telegram connector first.".to_string());
                    }
                    if ravenbot_lib::telegram::read_token(&db).await.is_none() {
                        return Err("Store a token first with `ravenbot telegram token`.".to_string());
                    }
                    let pairing = ravenbot_lib::telegram::create_pairing(
                        &db,
                        bot.id,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "bot": bot.name,
                            "bot_id": bot.id,
                            "expires_at": pairing.expires_at,
                            "instruction": format!("Send /start {} to the Telegram bot.", pairing.code),
                        }))
                        .unwrap_or_default()
                    );
                }
                "unlink" => {
                    let chat_id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot telegram unlink <chat-id>".to_string())?;
                    let removed = ravenbot_db::queries::TelegramQueries::unlink(db.pool(), chat_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if removed { "unlinked" } else { "not found" });
                }
                other => {
                    return Err(format!(
                        "Unknown telegram action: {other}. Use status, token, pair, unlink, enable, or disable."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot telegram {action} error: {error}");
        std::process::exit(1);
    }
}

fn slack_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            match action {
                "status" => {
                    let status = ravenbot_lib::slack::status(&db).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    );
                }
                "tokens" => {
                    use std::io::Read;
                    let mut input = String::new();
                    std::io::stdin()
                        .read_to_string(&mut input)
                        .map_err(|error| error.to_string())?;
                    let mut lines = input.lines().map(str::trim).filter(|line| !line.is_empty());
                    let app = lines
                        .next()
                        .ok_or_else(|| "Expected the app token on the first line.".to_string())?;
                    let bot = lines.next().ok_or_else(|| {
                        "Expected the bot token on the second line.".to_string()
                    })?;
                    ravenbot_lib::slack::set_tokens(&db, app, bot).await?;
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::slack::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Slack tokens stored and connector enabled.");
                }
                "enable" => {
                    if ravenbot_lib::slack::app_token(&db).await.is_none()
                        || ravenbot_lib::slack::bot_token(&db).await.is_none()
                    {
                        return Err("Store both tokens first with `ravenbot slack tokens`.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::slack::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Slack connector enabled.");
                }
                "disable" => {
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::slack::ENABLED_KEY,
                        "0",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Slack connector disabled.");
                }
                "pair" => {
                    let bot_arg = args
                        .iter()
                        .position(|arg| arg == "--bot")
                        .and_then(|index| args.get(index + 1));
                    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                    let bot = match bot_arg {
                        Some(value) => bots
                            .iter()
                            .find(|bot| {
                                bot.name.eq_ignore_ascii_case(value)
                                    || bot.id.to_string() == *value
                            })
                            .cloned(),
                        None => ravenbot_db::queries::AppSettingsQueries::get(
                            db.pool(),
                            ravenbot_lib::slack::DEFAULT_BOT_KEY,
                        )
                        .await
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            bots.iter()
                                .find(|bot| bot.id.to_string() == value)
                                .cloned()
                        })
                        .or_else(|| bots.first().cloned()),
                    }
                    .ok_or_else(|| "No agent found. Pass --bot NAME or create an agent first.".to_string())?;
                    if !ravenbot_lib::slack::enabled(&db).await {
                        return Err("Enable the Slack connector first.".to_string());
                    }
                    if ravenbot_lib::slack::app_token(&db).await.is_none()
                        || ravenbot_lib::slack::bot_token(&db).await.is_none()
                    {
                        return Err("Store both tokens first with `ravenbot slack tokens`.".to_string());
                    }
                    let pairing = ravenbot_lib::slack::create_pairing(
                        &db,
                        bot.id,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "bot": bot.name,
                            "bot_id": bot.id,
                            "expires_at": pairing.expires_at,
                            "instruction": format!("In Slack, DM the bot or mention it with: pair {}", pairing.code),
                        }))
                        .unwrap_or_default()
                    );
                }
                "unlink" => {
                    let channel_id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot slack unlink <channel-id>".to_string())?;
                    let removed = ravenbot_db::queries::SlackQueries::unlink(db.pool(), channel_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if removed { "unlinked" } else { "not found" });
                }
                other => {
                    return Err(format!(
                        "Unknown slack action: {other}. Use status, tokens, pair, unlink, enable, or disable."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot slack {action} error: {error}");
        std::process::exit(1);
    }
}

fn discord_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            match action {
                "status" => {
                    let status = ravenbot_lib::discord::status(&db).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    );
                }
                "token" => {
                    use std::io::Read;
                    let mut token = String::new();
                    std::io::stdin()
                        .read_to_string(&mut token)
                        .map_err(|error| error.to_string())?;
                    ravenbot_lib::discord::set_token(&db, &token).await?;
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::discord::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Discord token stored and connector enabled.");
                }
                "enable" => {
                    if ravenbot_lib::discord::read_token(&db).await.is_none() {
                        return Err("Store a token first with `ravenbot discord token`.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::discord::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Discord connector enabled.");
                }
                "disable" => {
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::discord::ENABLED_KEY,
                        "0",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Discord connector disabled.");
                }
                "pair" => {
                    let bot_arg = args
                        .iter()
                        .position(|arg| arg == "--bot")
                        .and_then(|index| args.get(index + 1));
                    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                    let bot = match bot_arg {
                        Some(value) => bots
                            .iter()
                            .find(|bot| {
                                bot.name.eq_ignore_ascii_case(value)
                                    || bot.id.to_string() == *value
                            })
                            .cloned(),
                        None => ravenbot_db::queries::AppSettingsQueries::get(
                            db.pool(),
                            ravenbot_lib::discord::DEFAULT_BOT_KEY,
                        )
                        .await
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            bots.iter()
                                .find(|bot| bot.id.to_string() == value)
                                .cloned()
                        })
                        .or_else(|| bots.first().cloned()),
                    }
                    .ok_or_else(|| "No agent found. Pass --bot NAME or create an agent first.".to_string())?;
                    if !ravenbot_lib::discord::enabled(&db).await {
                        return Err("Enable the Discord connector first.".to_string());
                    }
                    if ravenbot_lib::discord::read_token(&db).await.is_none() {
                        return Err("Store a token first with `ravenbot discord token`.".to_string());
                    }
                    let pairing = ravenbot_lib::discord::create_pairing(
                        &db,
                        bot.id,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "bot": bot.name,
                            "bot_id": bot.id,
                            "expires_at": pairing.expires_at,
                            "instruction": format!("In Discord, DM the bot or mention it with: pair {}", pairing.code),
                        }))
                        .unwrap_or_default()
                    );
                }
                "unlink" => {
                    let channel_id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot discord unlink <channel-id>".to_string())?;
                    let removed = ravenbot_db::queries::DiscordQueries::unlink(db.pool(), channel_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if removed { "unlinked" } else { "not found" });
                }
                other => {
                    return Err(format!(
                        "Unknown discord action: {other}. Use status, token, pair, unlink, enable, or disable."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot discord {action} error: {error}");
        std::process::exit(1);
    }
}

fn whatsapp_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            match action {
                "status" => {
                    let status = ravenbot_lib::whatsapp::status(&db).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    );
                }
                "tokens" => {
                    use std::io::Read;
                    let mut input = String::new();
                    std::io::stdin()
                        .read_to_string(&mut input)
                        .map_err(|error| error.to_string())?;
                    let mut lines = input.lines().map(str::trim).filter(|line| !line.is_empty());
                    let token = lines
                        .next()
                        .ok_or_else(|| "Expected the access token on the first line.".to_string())?;
                    let app_secret = lines
                        .next()
                        .ok_or_else(|| "Expected the app secret on the second line.".to_string())?;
                    let verify_token = lines
                        .next()
                        .ok_or_else(|| "Expected the verify token on the third line.".to_string())?;
                    ravenbot_lib::whatsapp::set_token(&db, token).await?;
                    ravenbot_lib::whatsapp::set_app_secret(&db, app_secret).await?;
                    ravenbot_lib::whatsapp::set_verify_token(&db, verify_token).await?;
                    let phone_id = args
                        .iter()
                        .position(|arg| arg == "--phone-id")
                        .and_then(|index| args.get(index + 1));
                    if let Some(phone_id) = phone_id {
                        ravenbot_lib::whatsapp::set_phone_number_id(&db, phone_id).await?;
                    }
                    if ravenbot_lib::whatsapp::read_phone_number_id(&db).await.is_none() {
                        return Err("Pass --phone-id <id> to store the WhatsApp phone number id.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::whatsapp::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("WhatsApp credentials stored and connector enabled.");
                }
                "enable" => {
                    if ravenbot_lib::whatsapp::read_token(&db).await.is_none()
                        || ravenbot_lib::whatsapp::read_app_secret(&db).await.is_none()
                        || ravenbot_lib::whatsapp::read_verify_token(&db).await.is_none()
                        || ravenbot_lib::whatsapp::read_phone_number_id(&db).await.is_none()
                    {
                        return Err("Store all WhatsApp credentials first with `ravenbot whatsapp tokens --phone-id <id>`.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::whatsapp::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("WhatsApp connector enabled.");
                }
                "disable" => {
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::whatsapp::ENABLED_KEY,
                        "0",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("WhatsApp connector disabled.");
                }
                "pair" => {
                    let bot_arg = args
                        .iter()
                        .position(|arg| arg == "--bot")
                        .and_then(|index| args.get(index + 1));
                    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                    let bot = match bot_arg {
                        Some(value) => bots
                            .iter()
                            .find(|bot| {
                                bot.name.eq_ignore_ascii_case(value)
                                    || bot.id.to_string() == *value
                            })
                            .cloned(),
                        None => ravenbot_db::queries::AppSettingsQueries::get(
                            db.pool(),
                            ravenbot_lib::whatsapp::DEFAULT_BOT_KEY,
                        )
                        .await
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            bots.iter()
                                .find(|bot| bot.id.to_string() == value)
                                .cloned()
                        })
                        .or_else(|| bots.first().cloned()),
                    }
                    .ok_or_else(|| "No agent found. Pass --bot NAME or create an agent first.".to_string())?;
                    if !ravenbot_lib::whatsapp::enabled(&db).await {
                        return Err("Enable the WhatsApp connector first.".to_string());
                    }
                    let pairing = ravenbot_lib::whatsapp::create_pairing(
                        &db,
                        bot.id,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "bot": bot.name,
                            "bot_id": bot.id,
                            "expires_at": pairing.expires_at,
                            "webhook_url": ravenbot_lib::chat_webhook::local_url(),
                            "instruction": format!("Send `pair {}` to the WhatsApp business number.", pairing.code),
                        }))
                        .unwrap_or_default()
                    );
                }
                "unlink" => {
                    let wa_id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot whatsapp unlink <phone-number>".to_string())?;
                    let removed = ravenbot_db::queries::WhatsAppQueries::unlink(db.pool(), wa_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if removed { "unlinked" } else { "not found" });
                }
                other => {
                    return Err(format!(
                        "Unknown whatsapp action: {other}. Use status, tokens, pair, unlink, enable, or disable."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot whatsapp {action} error: {error}");
        std::process::exit(1);
    }
}

fn teams_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            match action {
                "status" => {
                    let status = ravenbot_lib::teams::status(&db).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    );
                }
                "configure" => {
                    let app_id = args
                        .iter()
                        .position(|arg| arg == "--app-id")
                        .and_then(|index| args.get(index + 1))
                        .ok_or_else(|| "Usage: ravenbot teams configure --app-id <guid> --tenants <guid[,guid]>".to_string())?;
                    let tenants = args
                        .iter()
                        .position(|arg| arg == "--tenants")
                        .and_then(|index| args.get(index + 1))
                        .ok_or_else(|| "Pass --tenants with at least one tenant GUID.".to_string())?;
                    use std::io::Read;
                    let mut password = String::new();
                    std::io::stdin()
                        .read_to_string(&mut password)
                        .map_err(|error| error.to_string())?;
                    ravenbot_lib::teams::set_app_id(&db, app_id).await?;
                    ravenbot_lib::teams::set_app_password(&db, &password).await?;
                    ravenbot_lib::teams::set_tenants(&db, tenants).await?;
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::teams::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Teams credentials stored and connector enabled.");
                }
                "enable" => {
                    if ravenbot_lib::teams::read_app_id(&db).await.is_none()
                        || ravenbot_lib::teams::read_app_password(&db).await.is_none()
                        || ravenbot_lib::teams::read_tenants(&db).await.is_empty()
                    {
                        return Err("Run `ravenbot teams configure --app-id <guid> --tenants <ids>` first.".to_string());
                    }
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::teams::ENABLED_KEY,
                        "1",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Teams connector enabled.");
                }
                "disable" => {
                    ravenbot_db::queries::AppSettingsQueries::set(
                        db.pool(),
                        ravenbot_lib::teams::ENABLED_KEY,
                        "0",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    println!("Teams connector disabled.");
                }
                "pair" => {
                    let bot_arg = args
                        .iter()
                        .position(|arg| arg == "--bot")
                        .and_then(|index| args.get(index + 1));
                    let bots = ravenbot_db::queries::BotQueries::list(db.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                    let bot = match bot_arg {
                        Some(value) => bots
                            .iter()
                            .find(|bot| {
                                bot.name.eq_ignore_ascii_case(value)
                                    || bot.id.to_string() == *value
                            })
                            .cloned(),
                        None => ravenbot_db::queries::AppSettingsQueries::get(
                            db.pool(),
                            ravenbot_lib::teams::DEFAULT_BOT_KEY,
                        )
                        .await
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            bots.iter()
                                .find(|bot| bot.id.to_string() == value)
                                .cloned()
                        })
                        .or_else(|| bots.first().cloned()),
                    }
                    .ok_or_else(|| "No agent found. Pass --bot NAME or create an agent first.".to_string())?;
                    if !ravenbot_lib::teams::enabled(&db).await {
                        return Err("Enable the Teams connector first.".to_string());
                    }
                    let pairing = ravenbot_lib::teams::create_pairing(
                        &db,
                        bot.id,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "bot": bot.name,
                            "bot_id": bot.id,
                            "expires_at": pairing.expires_at,
                            "webhook_url": ravenbot_lib::chat_webhook::local_teams_url(),
                            "instruction": format!("Send `pair {}` in Teams.", pairing.code),
                        }))
                        .unwrap_or_default()
                    );
                }
                "unlink" => {
                    let conversation_id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot teams unlink <conversation-id>".to_string())?;
                    let removed = ravenbot_db::queries::TeamsQueries::unlink(db.pool(), conversation_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if removed { "unlinked" } else { "not found" });
                }
                other => {
                    return Err(format!(
                        "Unknown teams action: {other}. Use status, configure, pair, unlink, enable, or disable."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot teams {action} error: {error}");
        std::process::exit(1);
    }
}

fn account_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("list");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let base = ravenbot_lib::accounts::base_root();
            match action {
                "list" | "status" => {
                    let accounts = ravenbot_lib::accounts::list_accounts(&base).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &accounts.iter().map(|account| account.summary()).collect::<Vec<_>>()
                        )
                        .unwrap_or_default()
                    );
                }
                "create" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account create <username>".to_string())?;
                    use std::io::Read;
                    let mut password = String::new();
                    std::io::stdin()
                        .read_to_string(&mut password)
                        .map_err(|error| error.to_string())?;
                    let password = password.trim_end_matches(['\r', '\n']).to_string();
                    let account = ravenbot_lib::accounts::create_account(&base, username, &password)
                        .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&account).unwrap_or_default()
                    );
                }
                "passwd" | "password" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account passwd <username>".to_string())?;
                    use std::io::Read;
                    let mut password = String::new();
                    std::io::stdin()
                        .read_to_string(&mut password)
                        .map_err(|error| error.to_string())?;
                    let password = password.trim_end_matches(['\r', '\n']).to_string();
                    ravenbot_lib::accounts::set_password(&base, username, &password).await?;
                    println!("Password updated and existing sessions revoked.");
                }
                "disable" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account disable <username>".to_string())?;
                    ravenbot_lib::accounts::set_disabled(&base, username, true).await?;
                    println!("Account disabled.");
                }
                "enable" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account enable <username>".to_string())?;
                    ravenbot_lib::accounts::set_disabled(&base, username, false).await?;
                    println!("Account enabled.");
                }
                "revoke" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account revoke <username>".to_string())?;
                    let account = ravenbot_lib::accounts::account_by_username(&base, username)
                        .await?
                        .ok_or_else(|| "That account does not exist.".to_string())?;
                    ravenbot_lib::accounts::revoke_account_sessions(&base, &account.id).await?;
                    println!("Sessions revoked.");
                }
                "grant" => {
                    let workspace = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account grant <workspace-owner> <account> owner|operator|viewer".to_string())?;
                    let grantee = args
                        .get(3)
                        .ok_or_else(|| "Pass the account that should receive access.".to_string())?;
                    let role = args
                        .get(4)
                        .ok_or_else(|| "Pass owner, operator or viewer.".to_string())?;
                    ravenbot_lib::accounts::grant_workspace(&base, workspace, grantee, role).await?;
                    println!("Workspace access granted.");
                }
                "ungrant" => {
                    let workspace = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account ungrant <workspace-owner> <account>".to_string())?;
                    let grantee = args
                        .get(3)
                        .ok_or_else(|| "Pass the account whose access should be removed.".to_string())?;
                    let removed = ravenbot_lib::accounts::revoke_workspace(&base, workspace, grantee).await?;
                    println!("{}", if removed { "Workspace access revoked." } else { "No grant found." });
                }
                "workspaces" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account workspaces <username>".to_string())?;
                    let account = ravenbot_lib::accounts::account_by_username(&base, username)
                        .await?
                        .ok_or_else(|| "That account does not exist.".to_string())?;
                    let workspaces = ravenbot_lib::accounts::list_workspaces_for_account(&base, &account.id).await?;
                    println!("{}", serde_json::to_string_pretty(&workspaces).unwrap_or_default());
                }
                "grants" => {
                    let workspace = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account grants <workspace-owner>".to_string())?;
                    let grants = ravenbot_lib::accounts::list_workspace_grants(&base, workspace).await?;
                    println!("{}", serde_json::to_string_pretty(&grants).unwrap_or_default());
                }
                "rename" => {
                    let old = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account rename <old> <new>".to_string())?;
                    let new = args
                        .get(3)
                        .ok_or_else(|| "Pass the new username.".to_string())?;
                    let account = ravenbot_lib::accounts::rename_account(&base, old, new).await?;
                    println!("{}", serde_json::to_string_pretty(&account).unwrap_or_default());
                }
                "delete" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account delete <username> [--purge]".to_string())?;
                    let purge = args.iter().any(|arg| arg == "--purge");
                    let account = ravenbot_lib::accounts::delete_account(&base, username, purge).await?;
                    println!("{}", serde_json::to_string_pretty(&account).unwrap_or_default());
                }
                "env" => {
                    let username = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot account env <username>".to_string())?;
                    let account = ravenbot_lib::accounts::account_by_username(&base, username)
                        .await?
                        .ok_or_else(|| "That account does not exist.".to_string())?;
                    println!("export RAVENBOT_BASE_HOME='{}'", base.display());
                    println!("export RAVENBOT_HOME='{}'", account.data_root);
                    println!("export RAVENBOT_WORKSPACE_ID='{}'", account.id);
                    println!(
                        "export RAVENBOT_DB='{}'",
                        std::path::Path::new(&account.data_root)
                            .join("ravenbot.db")
                            .display()
                    );
                    if !account.is_owner {
                        let short = &account.id[..account.id.len().min(8)];
                        println!("export RAVENBOT_KEYCHAIN_NAMESPACE='{}'", account.id);
                        println!("export RAVENBOT_WEBHOOK_PORT='{}'", account.port_base);
                        println!(
                            "export RAVENBOT_CHAT_WEBHOOK_PORT='{}'",
                            account.port_base + 1
                        );
                        println!(
                            "export RAVENBOT_REMOTE_PORT='{}'",
                            account.port_base + 2
                        );
                        println!("export RAVENBOT_SERVICE_NAME='ravenbot-{short}'");
                        println!("export RAVENBOT_SERVICE_LABEL='dev.ravenbot.background.{short}'");
                    }
                }
                other => {
                    return Err(format!(
                        "Unknown account action: {other}. Use list, create, passwd, disable, enable, revoke, or env."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot account {action} error: {error}");
        std::process::exit(1);
    }
}

fn remote_command(args: &[String]) {
    init_tracing();
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = (|| -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        rt.block_on(async move {
            let db = open_db().await?;
            let pool = db.pool();
            match action {
                "status" | "devices" => {
                    let enabled = ravenbot_db::queries::AppSettingsQueries::get(
                        pool,
                        "remote_enabled",
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
                    let port = ravenbot_db::queries::AppSettingsQueries::get(pool, "remote_port")
                        .await
                        .map_err(|error| error.to_string())?
                        .and_then(|value| value.parse::<u16>().ok())
                        .unwrap_or(ravenbot_remote::DEFAULT_PORT);
                    let tls = ravenbot_db::queries::AppSettingsQueries::get(pool, "remote_tls")
                        .await
                        .map_err(|error| error.to_string())?
                        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
                    let devices = ravenbot_db::queries::RemoteAccessQueries::list_devices(pool)
                        .await
                        .map_err(|error| error.to_string())?;
                    let audit = ravenbot_db::queries::RemoteAccessQueries::recent_audit(pool, 20)
                        .await
                        .map_err(|error| error.to_string())?;
                    let pending_approvals: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE status = 'pending'")
                            .fetch_one(pool)
                            .await
                            .map_err(|error| error.to_string())?;
                    let pending_questions: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM questions WHERE status = 'pending'")
                            .fetch_one(pool)
                            .await
                            .map_err(|error| error.to_string())?;
                    let running = std::net::TcpStream::connect_timeout(
                        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                        std::time::Duration::from_millis(300),
                    )
                    .is_ok();
                    let scheme = if tls { "https" } else { "http" };
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "enabled": enabled,
                            "listening": running,
                            "url": format!("{scheme}://127.0.0.1:{port}"),
                            "port": port,
                            "tls": tls,
                            "pending_approvals": pending_approvals,
                            "pending_questions": pending_questions,
                            "devices": devices,
                            "audit": audit,
                        }))
                        .unwrap_or_default()
                    );
                }
                "pair" => {
                    let role = args
                        .iter()
                        .position(|arg| arg == "--role")
                        .and_then(|index| args.get(index + 1))
                        .map(|value| ravenbot_remote::RemoteRole::parse(value))
                        .unwrap_or(ravenbot_remote::RemoteRole::Viewer);
                    let enabled = ravenbot_db::queries::AppSettingsQueries::get(
                        pool,
                        "remote_enabled",
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
                    if !enabled {
                        return Err(
                            "Remote access is disabled. Enable it in Settings → Remote access, or start the daemon with it enabled."
                                .to_string(),
                        );
                    }
                    let port = ravenbot_db::queries::AppSettingsQueries::get(pool, "remote_port")
                        .await
                        .map_err(|error| error.to_string())?
                        .and_then(|value| value.parse::<u16>().ok())
                        .unwrap_or(ravenbot_remote::DEFAULT_PORT);
                    if std::net::TcpStream::connect_timeout(
                        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                        std::time::Duration::from_millis(300),
                    )
                    .is_err()
                    {
                        return Err(format!(
                            "Remote access is enabled but nothing is listening on 127.0.0.1:{port}. Start the daemon or service first."
                        ));
                    }
                    let pairing = ravenbot_remote::create_pairing(
                        pool,
                        role,
                        std::time::Duration::from_secs(300),
                    )
                    .await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "code": pairing.code,
                            "role": pairing.role,
                            "expires_at": pairing.expires_at,
                        }))
                        .unwrap_or_default()
                    );
                }
                "revoke" => {
                    let id = args
                        .get(2)
                        .ok_or_else(|| "Usage: ravenbot remote revoke <device-id>".to_string())?;
                    let id = uuid::Uuid::parse_str(id).map_err(|error| error.to_string())?;
                    let revoked = ravenbot_db::queries::RemoteAccessQueries::revoke_device(pool, id)
                        .await
                        .map_err(|error| error.to_string())?;
                    println!("{}", if revoked { "revoked" } else { "not found or already revoked" });
                }
                other => {
                    return Err(format!(
                        "Unknown remote action: {other}. Use status, devices, pair, or revoke."
                    ))
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("ravenbot remote {action} error: {error}");
        std::process::exit(1);
    }
}

fn service_command(args: &[String]) {
    let action = args.get(1).map(|value| value.as_str()).unwrap_or("status");
    let result = match action {
        "status" => ravenbot_lib::service::status().map(|status| {
            println!(
                "{}",
                serde_json::to_string_pretty(&status).unwrap_or_default()
            );
        }),
        "install" => ravenbot_lib::service::install().map(|status| {
            println!(
                "{}",
                serde_json::to_string_pretty(&status).unwrap_or_default()
            );
        }),
        "uninstall" => ravenbot_lib::service::uninstall().map(|status| {
            println!(
                "{}",
                serde_json::to_string_pretty(&status).unwrap_or_default()
            );
        }),
        "handoff" => ravenbot_lib::service::handoff(),
        "start" => ravenbot_lib::service::start_running().map(|_| {
            println!("Background service started.");
        }),
        "stop" => ravenbot_lib::service::stop_running().map(|_| {
            println!("Background service stopped.");
        }),
        "unit" => ravenbot_lib::service::ServiceOptions::current()
            .and_then(|options| ravenbot_lib::service::render(&options))
            .map(|unit| print!("{unit}")),
        other => Err(format!(
            "Unknown service action: {other}. Use status, install, uninstall, or handoff."
        )),
    };
    if let Err(error) = result {
        eprintln!("ravenbot service {action} error: {error}");
        std::process::exit(1);
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
  ravenbot --hidden           Launch the desktop window hidden (tray mode)\n\
  ravenbot mcp-serve          Run RAVENBOT as an MCP server over stdio\n\
  ravenbot run --bot NAME --message \"TEXT\"\n\
                              Headless single run (name or bot id)\n\
  ravenbot list-bots          List bots\n\
  ravenbot service status     Show the background service state\n\
  ravenbot service install    Install the user service\n\
  ravenbot service uninstall  Remove the user service\n\
  ravenbot service handoff    Exit and start the service\n\
  ravenbot service start      Start the installed service\n\
  ravenbot service stop       Stop the running service\n\
  ravenbot service unit       Print the generated unit/plist\n\
  ravenbot daemon             Run the headless fleet daemon (service mode)\n\
  ravenbot remote status      Show remote access state and devices\n\
  ravenbot remote pair --role viewer|operator|admin\n\
                              Generate a one-time pairing code\n\
  ravenbot remote revoke ID   Revoke a paired device\n\
  ravenbot telegram status    Show native Telegram connector state\n\
  ravenbot telegram token     Read and store a bot token from stdin\n\
  ravenbot telegram pair --bot NAME|ID\n\
                              Generate a one-time chat pairing code\n\
  ravenbot telegram unlink ID Unlink a Telegram chat\n\
  ravenbot telegram enable|disable\n\
  ravenbot slack status       Show native Slack connector state\n\
  ravenbot slack tokens       Read app and bot tokens from stdin (2 lines)\n\
  ravenbot slack pair --bot NAME|ID\n\
                              Generate a one-time channel pairing code\n\
  ravenbot slack unlink ID    Unlink a Slack channel\n\
  ravenbot slack enable|disable\n\
  ravenbot discord status     Show native Discord connector state\n\
  ravenbot discord token      Read and store a bot token from stdin\n\
  ravenbot discord pair --bot NAME|ID\n\
                              Generate a one-time channel pairing code\n\
  ravenbot discord unlink ID  Unlink a Discord channel\n\
  ravenbot discord enable|disable\n\
  ravenbot whatsapp status    Show native WhatsApp connector state\n\
  ravenbot whatsapp tokens --phone-id ID\n\
                              Read access token, app secret and verify token\n\
                              from stdin (3 lines)\n\
  ravenbot whatsapp pair --bot NAME|ID\n\
                              Generate a one-time number pairing code\n\
  ravenbot whatsapp unlink NUM Unlink a WhatsApp number\n\
  ravenbot whatsapp enable|disable\n\
  ravenbot teams status       Show native Microsoft Teams connector state\n\
  ravenbot teams configure --app-id GUID --tenants GUID[,GUID]\n\
                              Read the app password from stdin and enable\n\
  ravenbot teams pair --bot NAME|ID\n\
                              Generate a one-time conversation pairing code\n\
  ravenbot teams unlink ID    Unlink a Teams conversation\n\
  ravenbot teams enable|disable\n\
  ravenbot account list       List local accounts\n\
  ravenbot account create NAME  Create an account (password from stdin)\n\
  ravenbot account passwd NAME  Change a password and revoke sessions\n\
  ravenbot account disable|enable NAME\n\
  ravenbot account revoke NAME  Revoke all sessions for an account\n\
  ravenbot account rename OLD NEW  Rename an account (revokes sessions)\n\
  ravenbot account delete NAME [--purge]  Delete a non-owner account\n\
  ravenbot account grant WORKSPACE ACCOUNT ROLE\n\
                              Grant owner/operator/viewer workspace access\n\
  ravenbot account ungrant WORKSPACE ACCOUNT\n\
  ravenbot account workspaces NAME  List workspaces an account can open\n\
  ravenbot account grants WORKSPACE  List grants on a workspace\n\
  ravenbot account env NAME   Print headless isolation environment\n\n\
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
            bot.id, bot.name, bot.config.model_provider, bot.config.model_id
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

        // Pre-populate saved provider API keys from SQLite into Runtime's ProviderManager
        if let Ok(saved_keys) = ravenbot_db::queries::ProviderKeyQueries::list(db.pool()).await {
            let mut manager = runtime.provider_manager().lock().await;
            for (provider, key) in saved_keys {
                if !key.trim().is_empty() {
                    manager.set_api_key(&provider, key);
                }
            }
        }

        // Pre-populate user-defined providers (e.g. OpenCode Go, LM Studio) and
        // base URL overrides, the same way the desktop app does at startup.
        if let Ok(rows) = ravenbot_db::queries::CustomProviderQueries::list(db.pool()).await {
            let mut manager = runtime.provider_manager().lock().await;
            for (
                id,
                display_name,
                kind,
                base_url,
                default_model,
                supports_tools,
                enabled,
                headers_json,
            ) in rows
            {
                if !enabled {
                    continue;
                }
                let headers = headers_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                manager.register_custom(ravenbot_models::manager::CustomProviderSpec {
                    id,
                    display_name,
                    kind,
                    base_url,
                    default_model,
                    supports_tools,
                    headers,
                });
            }
        }
        if let Ok(rows) = ravenbot_db::queries::ProviderBaseUrlQueries::list(db.pool()).await {
            let mut manager = runtime.provider_manager().lock().await;
            for (provider, url) in rows {
                manager.set_base_url(&provider, url);
            }
        }

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

        // Headless CLI: nobody can answer approval cards, so auto-allow
        // (each gate is still audited in the approvals table).
        runtime.set_auto_allow_approvals(true);
        let exec_result = runtime.execute_run(&mut run).await;
        runtime.set_auto_allow_approvals(false);
        exec_result.map_err(|e| e.to_string())?;

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
                        .map(|i| {
                            format!(
                                "{} {}",
                                match i.status {
                                    ravenbot_core::ChecklistStatus::Completed => "✓",
                                    ravenbot_core::ChecklistStatus::Failed => "✗",
                                    _ => "○",
                                },
                                i.label
                            )
                        })
                        .collect();
                    Some(text.clone().map_or_else(
                        || list.join("\n"),
                        |t| format!("{}\n{}", t, list.join("\n")),
                    ))
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
