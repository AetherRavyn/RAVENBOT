//! Per-bot isolated Docker desktop (VNC/noVNC) lifecycle.
//!
//! Each agent can get its own disposable Linux desktop with a browser and
//! terminal. The container is named deterministically from the bot id, gets a
//! dedicated workspace volume, and publishes its noVNC port on loopback only.
//! All process calls use argv (no shell) and degrade with honest errors when
//! Docker is not installed.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default desktop image (Ubuntu LXDE + noVNC). Override with
/// `RAVENBOT_DESKTOP_IMAGE`.
pub const DEFAULT_IMAGE: &str = "dorowu/ubuntu-desktop-lxde-vnc:latest";

/// A running (or known) desktop session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesktopSession {
    pub container: String,
    pub running: bool,
    /// In-browser noVNC URL once the container is running.
    pub url: Option<String>,
    pub image: String,
}

/// Resolve the desktop image (env override wins).
pub fn desktop_image() -> String {
    std::env::var("RAVENBOT_DESKTOP_IMAGE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string())
}

/// Deterministic container name per bot.
pub fn container_name(bot_id: &Uuid) -> String {
    let simple = bot_id.simple().to_string();
    format!("ravenbot-desk-{}", &simple[..12])
}

/// `docker run` argv for a bot desktop. Pure so it can be unit-tested.
pub fn run_args(bot_id: &Uuid, image: &str, cpus: f32, memory_mb: u64) -> Vec<String> {
    let name = container_name(bot_id);
    let volume = format!("ravenbot-desk-{}:/workspace", bot_id.simple());
    vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        name,
        "--hostname".into(),
        "ravenbot".into(),
        // Publish noVNC on an ephemeral loopback port only.
        "-p".into(),
        "127.0.0.1::80".into(),
        "-v".into(),
        volume,
        "--cpus".into(),
        format!("{cpus}"),
        "--memory".into(),
        format!("{memory_mb}m"),
        image.to_string(),
    ]
}

async fn docker(args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Docker is not available: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

/// Docker server version, if the daemon is reachable.
pub async fn docker_version() -> Option<String> {
    docker(&["version", "--format", "{{.Server.Version}}"])
        .await
        .ok()
        .filter(|v| !v.is_empty())
}

/// Whether the desktop image is already pulled locally.
pub async fn image_present(image: &str) -> bool {
    docker(&["image", "inspect", image]).await.is_ok()
}

/// Whether a container with this name exists (`true`) and whether it runs.
async fn inspect(name: &str) -> Option<bool> {
    let out = docker(&["inspect", "-f", "{{.State.Running}}", name]).await.ok()?;
    Some(out.trim() == "true")
}

/// Map the container's port 80 to a loopback noVNC URL.
async fn desktop_url(name: &str) -> Option<String> {
    let out = docker(&["port", name, "80"]).await.ok()?;
    // e.g. "127.0.0.1:32768"
    let addr = out.lines().next()?.trim();
    if addr.is_empty() {
        return None;
    }
    Some(format!("http://{}/vnc.html", addr))
}

/// Current status for a bot's desktop without mutating anything.
pub async fn status(bot_id: &Uuid) -> Result<Option<DesktopSession>, String> {
    let name = container_name(bot_id);
    let image = desktop_image();
    match inspect(&name).await {
        None => Ok(None),
        Some(running) => Ok(Some(DesktopSession {
            container: name.clone(),
            running,
            url: if running { desktop_url(&name).await } else { None },
            image,
        })),
    }
}

/// Start (or restart) a bot's isolated desktop. Pulls the image if missing.
pub async fn start(bot_id: &Uuid, cpus: f32, memory_mb: u64) -> Result<DesktopSession, String> {
    let image = desktop_image();
    let name = container_name(bot_id);

    // Already running → reuse.
    if inspect(&name).await == Some(true) {
        return Ok(DesktopSession {
            container: name.clone(),
            running: true,
            url: desktop_url(&name).await,
            image,
        });
    }
    // Stopped leftover → remove before re-creating.
    if inspect(&name).await.is_some() {
        let _ = docker(&["rm", "-f", &name]).await;
    }

    if !image_present(&image).await {
        docker(&["pull", &image])
            .await
            .map_err(|e| format!("Failed to pull desktop image '{image}': {e}"))?;
    }

    let args = run_args(bot_id, &image, cpus, memory_mb);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    docker(&arg_refs)
        .await
        .map_err(|e| format!("Failed to start desktop container: {e}"))?;

    Ok(DesktopSession {
        container: name.clone(),
        running: true,
        url: desktop_url(&name).await,
        image,
    })
}

/// Stop and remove a bot's desktop container (workspace volume is kept).
pub async fn stop(bot_id: &Uuid) -> Result<(), String> {
    let name = container_name(bot_id);
    match inspect(&name).await {
        None => Ok(()),
        Some(_) => {
            docker(&["stop", &name]).await?;
            docker(&["rm", &name]).await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_name_is_stable_and_prefixed() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(container_name(&id), "ravenbot-desk-111111112222");
        assert_eq!(container_name(&id), container_name(&id));
    }

    #[test]
    fn run_args_are_safe_and_bounded() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let args = run_args(&id, "img:tag", 2.0, 2048);
        assert_eq!(args[0], "run");
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"--name".to_string()));
        // Loopback-only publishing with an ephemeral host port.
        assert!(args.contains(&"127.0.0.1::80".to_string()));
        // Resource limits present.
        assert!(args.contains(&"--cpus".to_string()));
        assert!(args.contains(&"2".to_string()));
        assert!(args.contains(&"2048m".to_string()));
        // Dedicated per-bot volume.
        assert!(args.iter().any(|a| a.starts_with("ravenbot-desk-") && a.ends_with(":/workspace")));
        // Image is last.
        assert_eq!(args.last().unwrap(), "img:tag");
    }

    /// Real Docker lifecycle, using a tiny image that listens on port 80 so the
    /// port-mapping/noVNC-URL logic is exercised without pulling a full desktop.
    /// Opt-in: `RAVENBOT_DESKTOP_TEST=1 cargo test -p ravenbot-sandbox -- --ignored`
    #[tokio::test]
    #[ignore = "requires Docker; opt in with RAVENBOT_DESKTOP_TEST=1"]
    async fn docker_lifecycle_end_to_end() {
        if std::env::var("RAVENBOT_DESKTOP_TEST").as_deref() != Ok("1") {
            return;
        }
        std::env::set_var("RAVENBOT_DESKTOP_IMAGE", "nginx:alpine");
        let id = Uuid::new_v4();

        let session = start(&id, 1.0, 256).await.expect("start desktop");
        assert!(session.running);
        assert!(session.url.as_deref().unwrap_or("").starts_with("http://127.0.0.1:"));

        let live = status(&id).await.unwrap().expect("status present");
        assert!(live.running);

        stop(&id).await.expect("stop desktop");
        assert!(status(&id).await.unwrap().is_none(), "container should be gone");
    }
}
