//! RAVENBOT - A Sovereign, Local-First, Rust-Native Multi-Agent Desktop OS
//!
//! This crate provides the Tauri shell and IPC handlers.

use ravenbot_core::*;
use ravenbot_db::Database;
use ravenbot_models::ModelDiscovery;
use ravenbot_runtime::graph::TaskGraph;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;
use tauri::State;
use uuid::Uuid;

mod office_org;

/// Application state shared across handlers
pub struct AppState {
    pub db: Database,
    pub runtime: Arc<ravenbot_runtime::Runtime>,
    pub scheduler: Arc<ravenbot_scheduler::Scheduler>,
    pub model_discovery: Arc<ModelDiscovery>,
    /// Cached, searchable OpenAPI directory for Plugins & Native Tools.
    pub plugin_catalog: Arc<ravenbot_plugins::catalog::CatalogService>,
    /// Base URL of the running webhook receiver (set during setup).
    pub webhook_base_url: std::sync::RwLock<String>,
}

// ── Interactive visual replies ─────────────────────────────────────────────
//
// An agent can publish a self-contained HTML page as a visual reply. The page
// runs scripts, but on its own `ravenvisual://` origin, in a sandboxed frame
// without `allow-same-origin`: it cannot read the app, its storage, or its IPC
// bridge. The page reports its height and asks the app to open web links over
// postMessage; the frontend validates both.
//
// The HTML is held in memory under an unguessable id. A page is released when
// the message that owns it unmounts, and the store is capped so a long chat
// cannot grow without bound.

struct VisualPageStore {
    pages: std::collections::HashMap<String, String>,
    order: std::collections::VecDeque<String>,
}

static VISUAL_PAGES: std::sync::OnceLock<std::sync::Mutex<VisualPageStore>> =
    std::sync::OnceLock::new();

fn visual_page_store() -> &'static std::sync::Mutex<VisualPageStore> {
    VISUAL_PAGES.get_or_init(|| {
        std::sync::Mutex::new(VisualPageStore {
            pages: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
        })
    })
}

const MAX_VISUAL_BYTES: usize = 512 * 1024;
const MAX_VISUAL_PAGES: usize = 96;

fn visual_page_url(id: &str) -> String {
    // Tauri custom protocols are `<scheme>://localhost/<path>` on macOS/Linux
    // and `http://<scheme>.localhost/<path>` on Windows/Android.
    if cfg!(any(windows, target_os = "android")) {
        format!("http://ravenvisual.localhost/{id}")
    } else {
        format!("ravenvisual://localhost/{id}")
    }
}

#[tauri::command]
async fn register_visual(html: String) -> Result<String, String> {
    if html.trim().is_empty() {
        return Err("The visual page is empty.".to_string());
    }
    if html.len() > MAX_VISUAL_BYTES {
        return Err(format!(
            "The visual page is larger than the {}KB limit.",
            MAX_VISUAL_BYTES / 1024
        ));
    }
    let id = Uuid::new_v4().to_string();
    {
        let mut store = visual_page_store()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        store.pages.insert(id.clone(), html);
        store.order.push_back(id.clone());
        while store.order.len() > MAX_VISUAL_PAGES {
            if let Some(oldest) = store.order.pop_front() {
                store.pages.remove(&oldest);
            }
        }
    }
    Ok(visual_page_url(&id))
}

#[tauri::command]
async fn release_visual(id: String) {
    let mut store = visual_page_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    store.pages.remove(&id);
    store.order.retain(|candidate| candidate != &id);
}

fn visual_protocol_response(
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let id = request
        .uri()
        .path()
        .trim_start_matches('/')
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let html = visual_page_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .pages
        .get(id)
        .cloned();
    match html {
        Some(html) => tauri::http::Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Cache-Control", "no-store")
            .header(
                "Content-Security-Policy",
                "default-src 'none'; style-src 'unsafe-inline' https:;                  script-src 'unsafe-inline' 'unsafe-eval' https:;                  img-src data: https: blob:; media-src data: https: blob:;                  font-src data: https:; connect-src https:; worker-src blob:;                  frame-src 'none'; object-src 'none'; base-uri 'none';                  form-action 'none'",
            )
            .body(html.into_bytes())
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
        None => tauri::http::Response::builder()
            .status(404)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(b"Visual not found".to_vec())
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
    }
}

// ── Persistent browser dock ───────────────────────────────────────────────
//
// A sibling child webview, not an iframe. Remote sites refuse framing through
// `X-Frame-Options`/`frame-ancestors`, and the app's CSP is not theirs to
// relax, so the only honest way to show a real page is a native webview. It is
// created once and hidden when the user leaves the dock, which keeps history,
// scroll position, and logins across view changes.
//
// The dock has no Tauri capability entry: a remote origin must not reach app
// commands. The shell drives it from the local frontend only.

const BROWSER_DOCK_LABEL: &str = "browser-dock";

/// When the user takes control of the dock, agent DOM actions are refused
/// until they release it. This is deliberately process-wide and reset when
/// the panel is hidden: it is a control state, not a stored preference.
static BROWSER_DOCK_TAKEOVER: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn browser_dock_takeover_enabled() -> bool {
    BROWSER_DOCK_TAKEOVER.load(std::sync::atomic::Ordering::SeqCst)
}

fn set_browser_dock_takeover(taken: bool) -> bool {
    BROWSER_DOCK_TAKEOVER.store(taken, std::sync::atomic::Ordering::SeqCst);
    taken
}

#[derive(Default)]
struct BrowserDockMeta {
    title: String,
    visible: bool,
}

static BROWSER_DOCK_META: std::sync::OnceLock<std::sync::Mutex<BrowserDockMeta>> =
    std::sync::OnceLock::new();

fn browser_dock_meta() -> &'static std::sync::Mutex<BrowserDockMeta> {
    BROWSER_DOCK_META.get_or_init(|| std::sync::Mutex::new(BrowserDockMeta::default()))
}

/// A file the dock downloaded. The list is session metadata plus a directory
/// scan, so files survive a restart even though their URLs do not.
#[derive(Debug, Clone, serde::Serialize)]
struct BrowserDownload {
    id: String,
    name: String,
    path: String,
    url: String,
    state: String,
    size: Option<u64>,
    started_at: String,
    finished_at: Option<String>,
}

static BROWSER_DOWNLOADS: std::sync::OnceLock<std::sync::Mutex<Vec<BrowserDownload>>> =
    std::sync::OnceLock::new();

fn browser_downloads() -> &'static std::sync::Mutex<Vec<BrowserDownload>> {
    BROWSER_DOWNLOADS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn downloads_directory() -> std::path::PathBuf {
    ravenbot_core::raven_root().join("downloads")
}

fn unique_download_path(suggested: &std::path::Path) -> std::path::PathBuf {
    let directory = downloads_directory();
    let _ = std::fs::create_dir_all(&directory);
    let raw = suggested
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let mut name: String = raw
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\') {
                '_'
            } else {
                character
            }
        })
        .collect();
    if name.starts_with('.') {
        name.insert_str(0, "download-");
    }
    if name.chars().count() > 160 {
        name = name.chars().take(160).collect();
    }
    let mut candidate = directory.join(&name);
    let stem = candidate
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download")
        .to_string();
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let mut counter = 2u32;
    while candidate.exists() {
        candidate = directory.join(format!("{stem} ({counter}){extension}"));
        counter += 1;
    }
    candidate
}

fn record_download_started(url: &str, path: &std::path::Path) {
    let record = BrowserDownload {
        id: path.to_string_lossy().to_string(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("download")
            .to_string(),
        path: path.to_string_lossy().to_string(),
        url: url.to_string(),
        state: "downloading".to_string(),
        size: None,
        started_at: chrono::Utc::now().to_rfc3339(),
        finished_at: None,
    };
    let mut downloads = browser_downloads()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    downloads.retain(|download| download.path != record.path);
    downloads.insert(0, record);
    downloads.truncate(100);
}

/// Move a completed download into the app's downloads folder.
///
/// WebKitGTK does not always emit the `Requested` event that lets us set the
/// destination; it does emit `Finished` with the system path. Moving the file
/// there keeps downloads in one place on every platform.
fn relocate_download(path: &std::path::Path) -> std::path::PathBuf {
    let directory = downloads_directory();
    if path.starts_with(&directory) {
        return path.to_path_buf();
    }
    let target = unique_download_path(path);
    match std::fs::copy(path, &target) {
        Ok(_) => {
            let _ = std::fs::remove_file(path);
            tracing::info!(
                from = %path.display(),
                to = %target.display(),
                "browser download moved into the app downloads folder"
            );
            target
        }
        Err(error) => {
            tracing::warn!(
                from = %path.display(),
                error = %error,
                "browser download stayed in the system folder"
            );
            path.to_path_buf()
        }
    }
}

fn record_download_finished(
    app: &tauri::AppHandle,
    url: &str,
    path: Option<&std::path::Path>,
    success: bool,
) {
    let relocated = if success {
        path.map(relocate_download)
    } else {
        None
    };
    let path = relocated.as_deref().or(path);
    let Some(path) = path else {
        let _ = app.emit(
            "browser-dock:download",
            serde_json::json!({
                "url": url,
                "success": success,
                "state": if success { "done" } else { "failed" },
            }),
        );
        return;
    };
    let path_string = path.to_string_lossy().to_string();
    let size = std::fs::metadata(path).ok().map(|metadata| metadata.len());
    let mut downloads = browser_downloads()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let record = if let Some(record) = downloads.iter_mut().find(|item| item.path == path_string) {
        record.state = if success { "done" } else { "failed" }.to_string();
        record.size = size;
        record.finished_at = Some(chrono::Utc::now().to_rfc3339());
        record.clone()
    } else {
        let record = BrowserDownload {
            id: path_string.clone(),
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download")
                .to_string(),
            path: path_string,
            url: url.to_string(),
            state: if success { "done" } else { "failed" }.to_string(),
            size,
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        downloads.insert(0, record.clone());
        record
    };
    downloads.truncate(100);
    drop(downloads);
    let _ = app.emit(
        "browser-dock:download",
        serde_json::json!({ "download": record, "success": success }),
    );
}

#[derive(Debug, Clone, serde::Deserialize)]
struct BrowserDockBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn browser_dock_position(bounds: &BrowserDockBounds) -> tauri::LogicalPosition<f64> {
    tauri::LogicalPosition::new(bounds.x.max(0.0), bounds.y.max(0.0))
}

fn browser_dock_size(bounds: &BrowserDockBounds) -> tauri::LogicalSize<f64> {
    tauri::LogicalSize::new(
        bounds.width.clamp(180.0, 4096.0),
        bounds.height.clamp(120.0, 4096.0),
    )
}

// Linux: Tauri's `Window::add_child` packs a child webview into the window's
// vertical `GtkBox`, so a second webview splits the window instead of floating
// above it. The dock needs overlay behaviour, so the window's content is
// wrapped in a `GtkOverlay` once and the child is reparented into a `GtkFixed`
// overlay. The wrapper lives on the GTK main thread; commands reach it by
// dispatching their GTK work through `with_webview`.
#[cfg(target_os = "linux")]
thread_local! {
    static BROWSER_DOCK_OVERLAY: std::cell::RefCell<Option<gtk::Overlay>> =
        const { std::cell::RefCell::new(None) };
    /// The dock body only: right-aligned, below the chrome, full height.
    /// Sizing a container is what keeps WebKit from painting the whole window
    /// when the app is maximized or the measured column is wide.
    static BROWSER_DOCK_FIXED: std::cell::RefCell<Option<gtk::Fixed>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "linux")]
fn linux_dock_install_overlay(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview("main")
        .ok_or_else(|| "The main webview is not open.".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    main.with_webview(move |platform| {
        use gtk::prelude::*;
        let result = (|| -> Result<(), String> {
            if BROWSER_DOCK_OVERLAY.with(|slot| slot.borrow().is_some()) {
                return Ok(());
            }
            let webview = platform.inner();
            let toplevel = webview
                .toplevel()
                .ok_or_else(|| "The main webview has no toplevel.".to_string())?;
            let window = toplevel
                .downcast::<gtk::Window>()
                .map_err(|_| "The main toplevel is not a GtkWindow.".to_string())?;
            let child = window
                .child()
                .ok_or_else(|| "The main window has no content.".to_string())?;
            // Idempotence matters across app reloads: wrapping an existing
            // overlay again would detach the whole app.
            if child.type_().name() == "GtkOverlay" {
                if let Ok(overlay) = child.clone().downcast::<gtk::Overlay>() {
                    BROWSER_DOCK_OVERLAY.with(|slot| *slot.borrow_mut() = Some(overlay));
                }
                return Ok(());
            }
            window.remove(&child);
            let overlay = gtk::Overlay::new();
            overlay.set_child(Some(&child));
            window.add(&overlay);
            overlay.show_all();
            BROWSER_DOCK_OVERLAY.with(|slot| *slot.borrow_mut() = Some(overlay));
            tracing::debug!("browser dock GTK overlay installed");
            Ok(())
        })();
        let _ = tx.send(result);
    })
    .map_err(|error| format!("Could not prepare the dock overlay: {error}"))?;
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "Timed out preparing the dock overlay.".to_string())?
}

/// Put the child webview in the overlay and give it the measured rectangle.
/// The first call reparents it out of the window's box; later calls just move
/// the widget, which is what keeps a resize from restarting the page.
#[cfg(target_os = "linux")]
fn linux_dock_apply_bounds(
    view: &tauri::Webview,
    bounds: &BrowserDockBounds,
) -> Result<(), String> {
    let y = bounds.y.max(0.0) as i32;
    let width = bounds.width.clamp(180.0, 4096.0) as i32;
    let height = bounds.height.clamp(120.0, 4096.0) as i32;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    view.with_webview(move |platform| {
        use gtk::prelude::*;
        let result = (|| -> Result<(), String> {
            let widget = platform.inner();
            let overlay = BROWSER_DOCK_OVERLAY
                .with(|slot| slot.borrow().clone())
                .ok_or_else(|| "The dock overlay is not installed.".to_string())?;
            let fixed = BROWSER_DOCK_FIXED
                .with(|slot| slot.borrow().clone())
                .unwrap_or_else(|| {
                    let fixed = gtk::Fixed::new();
                    // The fixed has no window of its own; the WebKit child has
                    // one, confined to this fixed's allocation. This is what keeps
                    // the page from covering the app chrome at large widths.
                    fixed.set_has_window(false);
                    fixed.set_halign(gtk::Align::End);
                    fixed.set_valign(gtk::Align::Fill);
                    fixed.set_hexpand(false);
                    fixed.set_vexpand(true);
                    fixed.set_margin_start(0);
                    fixed.set_margin_end(0);
                    fixed.set_margin_bottom(0);
                    fixed.set_size_request(width, -1);
                    overlay.add_overlay(&fixed);
                    fixed.show();
                    BROWSER_DOCK_FIXED.with(|slot| *slot.borrow_mut() = Some(fixed.clone()));
                    fixed
                });

            fixed.set_margin_top(y);
            fixed.set_size_request(width, height);
            fixed.queue_resize();

            let parent = widget.parent();
            if let Some(box_) = parent.as_ref().and_then(|p| p.downcast_ref::<gtk::Box>()) {
                box_.remove(&widget);
            }
            if parent
                .as_ref()
                .and_then(|p| p.downcast_ref::<gtk::Fixed>())
                .is_none()
            {
                fixed.put(&widget, 0, 0);
            }
            widget.set_size_request(width, height);
            widget.queue_resize();
            let allocation = widget.allocation();
            tracing::debug!(
                alloc_x = allocation.x(),
                alloc_y = allocation.y(),
                alloc_width = allocation.width(),
                alloc_height = allocation.height(),
                "browser dock page allocation"
            );

            // Show only while the dock is logically open. A bounds update
            // that lands just after the user closes the dock must not
            // resurrect the page above the app.
            let visible = browser_dock_meta()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .visible;
            if visible {
                fixed.show();
                widget.show();
            }
            tracing::debug!(y, width, height, "browser dock GTK bounds applied");
            Ok(())
        })();
        let _ = tx.send(result);
    })
    .map_err(|error| format!("Could not position the dock webview: {error}"))?;
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "Timed out positioning the dock webview.".to_string())?
}

/// The dock only loads the web. `file:`, `javascript:`, and custom schemes are
/// rejected here, not merely hidden from the URL bar.
fn parse_browser_dock_url(raw: &str) -> Result<tauri::Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("A URL is required.".to_string());
    }
    let parsed = tauri::Url::parse(trimmed).map_err(|error| format!("Invalid URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(format!(
            "The browser dock only opens http(s) pages, not {scheme}:."
        )),
    }
}

fn browser_dock_status(app: &tauri::AppHandle) -> serde_json::Value {
    let webview = app.get_webview(BROWSER_DOCK_LABEL);
    let url = webview
        .as_ref()
        .and_then(|view| view.url().ok())
        .map(|url| url.to_string());
    let (title, visible) = {
        let meta = browser_dock_meta()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (meta.title.clone(), meta.visible)
    };
    serde_json::json!({
        "open": webview.is_some() && visible,
        "exists": webview.is_some(),
        "url": url,
        "title": title,
        "taken_over": browser_dock_takeover_enabled(),
    })
}

#[tauri::command]
async fn browser_dock_open(
    app: tauri::AppHandle,
    url: String,
    bounds: BrowserDockBounds,
) -> Result<serde_json::Value, String> {
    let parsed = parse_browser_dock_url(&url)?;
    let position = browser_dock_position(&bounds);
    let size = browser_dock_size(&bounds);
    // Mark visible before positioning: the GTK show path consults this flag,
    // and a close that lands during creation must win.
    browser_dock_meta()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .visible = true;

    #[cfg(target_os = "linux")]
    linux_dock_install_overlay(&app)?;

    if let Some(view) = app.get_webview(BROWSER_DOCK_LABEL) {
        // On Linux the child lives in a GtkFixed, so the runtime's own
        // position/size calls are not enough; move the widget directly.
        #[cfg(target_os = "linux")]
        linux_dock_apply_bounds(&view, &bounds)?;
        #[cfg(not(target_os = "linux"))]
        {
            view.set_position(position).map_err(|e| e.to_string())?;
            view.set_size(size).map_err(|e| e.to_string())?;
        }
        // Reopening the panel should restore the page, scroll position and
        // all. Only navigate when the caller actually asked for a new URL.
        let current = view.url().ok().map(|url| url.to_string());
        let same_page = current.as_deref() == Some(parsed.as_str());
        if !same_page {
            view.navigate(parsed.clone()).map_err(|e| e.to_string())?;
        }
        view.show().map_err(|e| e.to_string())?;
        browser_dock_meta()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .visible = true;
        if !same_page {
            let _ = app.emit(
                "browser-dock:page-load",
                serde_json::json!({"url": parsed.as_str(), "event": "started"}),
            );
        }
        return Ok(browser_dock_status(&app));
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| "The main window is not open.".to_string())?;

    let load_app = app.clone();
    let download_app = app.clone();
    let title_app = app.clone();
    let popup_app = app.clone();
    let builder = tauri::webview::WebviewBuilder::new(
        BROWSER_DOCK_LABEL,
        tauri::WebviewUrl::External(parsed.clone()),
    )
    .on_navigation(|url| matches!(url.scheme(), "http" | "https"))
    .on_page_load(move |_view, payload| {
        let event = match payload.event() {
            tauri::webview::PageLoadEvent::Started => "started",
            tauri::webview::PageLoadEvent::Finished => "finished",
        };
        tracing::debug!(url = %payload.url(), event, "browser dock page load");
        let _ = load_app.emit(
            "browser-dock:page-load",
            serde_json::json!({"url": payload.url().as_str(), "event": event}),
        );
    })
    .on_download(move |_view, event| {
        use tauri::webview::DownloadEvent;
        let app = &download_app;
        match event {
            DownloadEvent::Requested { url, destination } => {
                tracing::info!(url = %url, suggested = %destination.display(), "browser download requested");
                let path = unique_download_path(destination);
                tracing::info!(url = %url, chosen = %path.display(), "browser download destination chosen");
                *destination = path.clone();
                record_download_started(url.as_str(), &path);
                let _ = app.emit(
                    "browser-dock:download",
                    serde_json::json!({
                        "download": browser_downloads()
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .iter()
                            .find(|item| item.path == path.to_string_lossy())
                            .cloned(),
                        "success": true,
                    }),
                );
            }
            DownloadEvent::Finished { url, path, success } => {
                tracing::info!(url = %url, path = ?path, success, "browser download finished");
                record_download_finished(&app, url.as_str(), path.as_deref(), success);
            }
            _ => {}
        }
        true
    })
    .on_document_title_changed(move |_view, title| {
        browser_dock_meta()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .title = title.clone();
        tracing::debug!(title = %title, "browser dock title changed");
        let _ = title_app.emit("browser-dock:title", serde_json::json!({"title": title}));
    })
    .on_new_window(move |url, _features| {
        // A target=_blank link should open in the dock, not in a second
        // unmanaged window the app cannot control. The frontend owns layout,
        // so ask it to navigate rather than guessing the bounds here.
        if matches!(url.scheme(), "http" | "https") {
            let _ = popup_app.emit(
                "browser-dock:request",
                serde_json::json!({"url": url.as_str()}),
            );
        }
        tauri::webview::NewWindowResponse::Deny
    });

    #[cfg(target_os = "linux")]
    let original_size = window.inner_size().ok();

    let view = window
        .add_child(builder, position, size)
        .map_err(|error| format!("Could not create the browser dock: {error}"))?;

    #[cfg(target_os = "linux")]
    {
        // `add_child` briefly packed the webview into the window's box, which
        // can make the window manager resize the window. Reparent it into the
        // overlay and restore the window's measured size.
        linux_dock_apply_bounds(&view, &bounds)?;
        if let Some(original_size) = original_size {
            let _ = window.set_size(original_size);
        }
    }
    view.show().map_err(|e| e.to_string())?;
    {
        let mut meta = browser_dock_meta()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        meta.visible = true;
        meta.title.clear();
    }
    Ok(browser_dock_status(&app))
}

#[tauri::command]
fn browser_dock_set_bounds(app: tauri::AppHandle, bounds: BrowserDockBounds) -> Result<(), String> {
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    #[cfg(target_os = "linux")]
    {
        linux_dock_apply_bounds(&view, &bounds)
    }
    #[cfg(not(target_os = "linux"))]
    {
        view.set_position(browser_dock_position(&bounds))
            .map_err(|e| e.to_string())?;
        view.set_size(browser_dock_size(&bounds))
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn browser_dock_navigate(app: tauri::AppHandle, url: String) -> Result<serde_json::Value, String> {
    let parsed = parse_browser_dock_url(&url)?;
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    view.navigate(parsed.clone()).map_err(|e| e.to_string())?;
    view.show().map_err(|e| e.to_string())?;
    let _ = app.emit(
        "browser-dock:page-load",
        serde_json::json!({"url": parsed.as_str(), "event": "started"}),
    );
    Ok(browser_dock_status(&app))
}

#[tauri::command]
fn browser_dock_back(app: tauri::AppHandle) -> Result<(), String> {
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    view.eval("history.back()").map_err(|e| e.to_string())
}

#[tauri::command]
fn browser_dock_forward(app: tauri::AppHandle) -> Result<(), String> {
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    view.eval("history.forward()").map_err(|e| e.to_string())
}

#[tauri::command]
fn browser_dock_reload(app: tauri::AppHandle) -> Result<(), String> {
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    view.reload().map_err(|e| e.to_string())
}

/// Hide, do not destroy: the next open keeps history and session cookies.
#[tauri::command]
fn browser_dock_hide(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(view) = app.get_webview(BROWSER_DOCK_LABEL) {
        view.hide().map_err(|e| e.to_string())?;
    }
    set_browser_dock_takeover(false);
    browser_dock_meta()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .visible = false;
    let _ = app.emit("browser-dock:closed", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
fn browser_dock_destroy(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(view) = app.get_webview(BROWSER_DOCK_LABEL) {
        view.close().map_err(|e| e.to_string())?;
    }
    set_browser_dock_takeover(false);
    {
        let mut meta = browser_dock_meta()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        meta.visible = false;
        meta.title.clear();
    }
    Ok(())
}

#[tauri::command]
fn browser_dock_state(app: tauri::AppHandle) -> serde_json::Value {
    browser_dock_status(&app)
}

// ── Versioned local skills ────────────────────────────────────────────────
//
// The tools in the skills crate write the library; these commands expose the
// read/install surface to Settings. Both paths go through the same
// `LocalSkillLibrary`, so the UI cannot install a version the agent tools
// would refuse.

#[tauri::command]
fn local_skills_list() -> Result<serde_json::Value, String> {
    let library = ravenbot_skills::local::LocalSkillLibrary::default();
    let skills = library.list()?;
    Ok(serde_json::json!({ "skills": skills }))
}

#[tauri::command]
fn local_skills_read(id: String, version: Option<String>) -> Result<serde_json::Value, String> {
    let library = ravenbot_skills::local::LocalSkillLibrary::default();
    let view = library.read(&id, version.as_deref())?;
    serde_json::to_value(view).map_err(|error| error.to_string())
}

#[tauri::command]
fn local_skills_install(
    id: String,
    version: String,
    force: Option<bool>,
) -> Result<serde_json::Value, String> {
    let library = ravenbot_skills::local::LocalSkillLibrary::default();
    let view = library.install(&id, &version, force.unwrap_or(false))?;
    serde_json::to_value(view).map_err(|error| error.to_string())
}

#[tauri::command]
async fn local_skills_marketplace_search(
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<ravenbot_skills::awesome::CatalogSkillSummary>, String> {
    ravenbot_skills::awesome::search_catalog(
        query.as_deref().unwrap_or_default(),
        limit.unwrap_or(40),
    )
    .await
}

#[tauri::command]
async fn local_skills_marketplace_install(
    target: String,
    update: Option<bool>,
) -> Result<serde_json::Value, String> {
    let document = ravenbot_skills::awesome::fetch_catalog_skill(&target)
        .await?
        .ok_or_else(|| {
            format!(
                "'{target}' was not found in the live skill catalog. Use the exact owner/name from the catalog, or a direct GitHub or officialskills.sh URL."
            )
        })?;
    let id = ravenbot_skills::local::local_id_from_catalog(&document.id);
    let library = ravenbot_skills::local::LocalSkillLibrary::default();
    let view = library.import_document(ravenbot_skills::local::ImportSkillInput {
        id,
        name: None,
        description: None,
        content: document.content,
        source: document.source.clone(),
        update: update.unwrap_or(false),
    })?;
    Ok(serde_json::json!({
        "skill": view,
        "catalog_id": document.id,
        "source_url": document.url,
        "source": document.source,
    }))
}

/// Run JavaScript in the dock and return its JSON result. The child webview is
/// remote content, so this runs in the page origin; it cannot reach the app's
/// IPC bridge or storage. Linux exposes the WebKitGTK callback API; other
/// platforms report that DOM actions are not implemented rather than faking a
/// success.
#[cfg(target_os = "linux")]
fn browser_dock_eval_blocking(
    app: &tauri::AppHandle,
    script: &str,
) -> Result<serde_json::Value, String> {
    use javascriptcore::ValueExt;
    use webkit2gtk::WebViewExt;
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    let script = script.to_string();
    tracing::debug!(script_len = script.len(), "browser dock eval requested");
    view.with_webview(move |platform| {
        let webview = platform.inner();
        #[allow(deprecated)]
        webview.run_javascript(
            &script,
            None::<&webkit2gtk::gio::Cancellable>,
            move |result| {
                let value = match result {
                    Ok(result) => match result.js_value() {
                        Some(value) => Ok(value
                            .to_json(0)
                            .map(|json| json.to_string())
                            .unwrap_or_else(|| "null".to_string())),
                        None => Ok("null".to_string()),
                    },
                    Err(error) => Err(error.to_string()),
                };
                let _ = tx.send(value);
            },
        );
    })
    .map_err(|error| format!("Could not reach the browser dock: {error}"))?;
    let raw = rx
        .recv_timeout(std::time::Duration::from_secs(15))
        .map_err(|_| "The browser did not answer within 15 seconds.".to_string())??;
    tracing::debug!(result = %raw.chars().take(200).collect::<String>(), "browser dock eval finished");
    match serde_json::from_str(&raw) {
        Ok(value) => Ok(value),
        Err(_) => Ok(serde_json::Value::String(raw)),
    }
}

#[cfg(not(target_os = "linux"))]
fn browser_dock_eval_blocking(
    _app: &tauri::AppHandle,
    _script: &str,
) -> Result<serde_json::Value, String> {
    Err("Browser DOM actions are not available on this platform yet.".to_string())
}

#[tauri::command]
async fn browser_dock_eval(
    app: tauri::AppHandle,
    script: String,
) -> Result<serde_json::Value, String> {
    if script.len() > 256 * 1024 {
        return Err("The browser script is larger than the 256KB limit.".to_string());
    }
    if browser_dock_takeover_enabled() {
        return Err(
            "The user has taken over the browser dock. Agent actions are paused until they release control."
                .to_string(),
        );
    }
    // `with_webview` posts to the main thread and waits for it. A synchronous
    // Tauri command can itself be running on that thread, which deadlocks the
    // dispatch; the blocking task keeps the wait off the main thread.
    tauri::async_runtime::spawn_blocking(move || browser_dock_eval_blocking(&app, &script))
        .await
        .map_err(|error| format!("The browser evaluation task failed: {error}"))?
}

#[tauri::command]
fn browser_dock_takeover() -> bool {
    browser_dock_takeover_enabled()
}

/// Pause or resume agent control of the real desktop. Browser takeover is a
/// separate surface so a user can drive one without releasing the other.
#[tauri::command]
fn computer_takeover() -> bool {
    ravenbot_core::computer_control_paused()
}

#[tauri::command]
fn set_computer_takeover(paused: bool) -> bool {
    ravenbot_core::set_computer_control_paused(paused);
    ravenbot_core::computer_control_paused()
}

/// Persisted Wayland host-control opt-in. The per-agent `host_control` switch
/// still has to be enabled before any input runs.
#[tauri::command]
async fn set_wayland_control_allowed(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        "wayland_control_allowed",
        if enabled { "1" } else { "0" },
    )
    .await
    .map_err(|error| error.to_string())?;
    ravenbot_core::set_wayland_control_allowed(enabled);
    Ok(enabled)
}

#[tauri::command]
async fn list_desktop_windows() -> Result<Vec<ravenbot_vision::DesktopWindow>, String> {
    ravenbot_vision::list_windows()
}

#[tauri::command]
async fn focus_desktop_window(window_id: u64) -> Result<String, String> {
    ravenbot_vision::focus_window(window_id).await
}

#[tauri::command]
fn browser_dock_set_takeover(app: tauri::AppHandle, taken: bool) -> Result<bool, String> {
    let taken = set_browser_dock_takeover(taken);
    if taken {
        if let Some(view) = app.get_webview(BROWSER_DOCK_LABEL) {
            view.set_focus().map_err(|error| error.to_string())?;
        }
    }
    Ok(taken)
}

#[tauri::command]
fn browser_dock_focus(app: tauri::AppHandle) -> Result<(), String> {
    let view = app
        .get_webview(BROWSER_DOCK_LABEL)
        .ok_or_else(|| "The browser dock is not open.".to_string())?;
    view.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_browser_downloads() -> Vec<BrowserDownload> {
    let directory = downloads_directory();
    let mut records = browser_downloads()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let path_string = path.to_string_lossy().to_string();
            if records.iter().any(|record| record.path == path_string) {
                continue;
            }
            records.push(BrowserDownload {
                id: path_string.clone(),
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("download")
                    .to_string(),
                path: path_string,
                url: String::new(),
                state: "done".to_string(),
                size: entry.metadata().ok().map(|metadata| metadata.len()),
                started_at: entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .map(|modified| chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                finished_at: None,
            });
        }
    }
    records.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    records.truncate(100);
    records
}

#[tauri::command]
fn browser_downloads_dir() -> String {
    downloads_directory().to_string_lossy().to_string()
}

// Tauri commands (IPC handlers)

/// An inline attachment sent from the composer (paste/drop/pick). Images carry
/// a base64 payload the model can see; other files are preserved and surfaced
/// to the model by name/type rather than being silently dropped.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttachmentInput {
    pub name: String,
    pub mime: String,
    /// Base64-encoded file data (no data-URI prefix)
    pub data: String,
}

/// Decoded byte length of a base64 payload.
///
/// `String::len()` counts the encoded characters, which overstates a file by
/// roughly a third. The UI shows this number on every attachment card, so it
/// has to be the size of the file the user will download.
fn decoded_base64_len(data: &str) -> u64 {
    let padding = data.bytes().rev().take_while(|byte| *byte == b'=').count();
    ((data.len() as u64) * 3 / 4).saturating_sub(padding as u64)
}

fn build_attachments(attachments: Vec<AttachmentInput>) -> Vec<ravenbot_core::Attachment> {
    attachments
        .into_iter()
        .map(|a| {
            let is_image = a.mime.trim().to_lowercase().starts_with("image/");
            ravenbot_core::Attachment {
                id: Uuid::new_v4(),
                name: a.name,
                mime_type: a.mime,
                size: decoded_base64_len(&a.data),
                path: String::new(),
                data: Some(a.data),
                is_image,
            }
        })
        .collect()
}

/// Local-first default provider/model for newly created bots (Ollama unless
/// RAVENBOT_DEFAULT_PROVIDER / RAVENBOT_DEFAULT_MODEL override it).
/// Place the main window on a named or 1-based monitor for testing.
///
/// Hyprland/X11 monitor-relative dispatches are compositor-specific and can
/// move a Wayland window outside its output. Tauri's own monitor list is the
/// portable source of truth: `RAVENBOT_WINDOW_MONITOR` accepts a monitor name
/// (`DP-4`) or a 1-based index after sorting monitors by x position. The size
/// is clamped to the monitor so a test window cannot be larger than the screen.
fn place_main_window(app: &tauri::AppHandle) {
    let Ok(spec) = std::env::var("RAVENBOT_WINDOW_MONITOR") else {
        return;
    };
    let spec = spec.trim().to_string();
    if spec.is_empty() {
        return;
    }
    let Some(window) = app.get_window("main") else {
        return;
    };
    let Ok(mut monitors) = window.available_monitors() else {
        return;
    };
    monitors.sort_by_key(|monitor| monitor.position().x);
    tracing::info!(
        monitors = ?monitors
            .iter()
            .map(|monitor| (
                monitor.name().map(String::as_str).unwrap_or("?").to_string(),
                monitor.position().x,
                monitor.position().y,
                monitor.size().width,
                monitor.size().height,
            ))
            .collect::<Vec<_>>(),
        "available monitors for window placement"
    );
    let monitor = if let Ok(index) = spec.parse::<usize>() {
        monitors.get(index.saturating_sub(1))
    } else {
        monitors.iter().find(|monitor| {
            monitor
                .name()
                .map(|name| name.eq_ignore_ascii_case(&spec))
                .unwrap_or(false)
        })
    };
    let Some(monitor) = monitor else {
        tracing::warn!(monitor = %spec, "RAVENBOT_WINDOW_MONITOR did not match a monitor");
        return;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let position = monitor.position().to_logical::<f64>(scale);
    if std::env::var("RAVENBOT_WINDOW_FULLSCREEN").is_ok() {
        // A tiled compositor ignores set_position/set_size. Borderless
        // fullscreen is the placement it must honor, so the test window really
        // is on the requested monitor.
        // Tauri 2 exposes fullscreen as a boolean; the compositor chooses the
        // monitor. The window is opened on the focused output for this test.
        let _ = window.set_fullscreen(true);
        let _ = window.set_focus();
        tracing::info!(
            monitor = monitor.name().map(String::as_str).unwrap_or("?"),
            "Placed the main window fullscreen for testing"
        );
        return;
    }
    let width = 1280.0_f64.min(size.width);
    let height = 820.0_f64.min(size.height);
    let x = position.x + ((size.width - width) / 2.0).max(0.0);
    let y = position.y + ((size.height - height) / 2.0).max(0.0);
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    let _ = window.set_focus();
    tracing::info!(
        monitor = monitor.name().map(String::as_str).unwrap_or("?"),
        x,
        y,
        width,
        height,
        "Placed the main window for testing"
    );
}

fn default_bot_model() -> (String, String) {
    let provider = std::env::var("RAVENBOT_DEFAULT_PROVIDER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "ollama".to_string());
    let model = std::env::var("RAVENBOT_DEFAULT_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| match provider.as_str() {
            "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
            "anthropic" => "claude-3-5-sonnet-20241022".to_string(),
            "openai" => "gpt-4o".to_string(),
            _ => "llama3.1".to_string(),
        });
    (provider, model)
}

/// The model a provider starts on when the owner has not chosen one.
///
/// Kept next to the resolver rather than in the frontend so `create_bot`,
/// office provisioning and the settings panel all agree on what "default"
/// means.
fn default_model_for_provider(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "opencode-go" => "deepseek-v4.1-flash",
        "opencode" => "claude-sonnet-4-5",
        "commandcode" => "deepseek/deepseek-v4-flash",
        "anthropic" | "claude" => "claude-3-7-sonnet-20250219",
        "openai" => "gpt-4o",
        "openrouter" => "anthropic/claude-3.5-sonnet",
        "deepseek" => "deepseek-chat",
        "groq" => "llama-3.3-70b-versatile",
        "gemini" => "gemini-2.0-flash",
        "xai" | "grok" => "grok-2-latest",
        "mistral" => "mistral-large-latest",
        "together" => "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        "perplexity" => "llama-3.1-sonar-large-128k-online",
        "cohere" => "command-r-plus",
        "mimo" => "mimo-v2.5",
        "cline" => "claude-sonnet-4-5",
        "tokenrouter" => "deepseek/deepseek-v4.1-flash",
        _ => "llama3.1",
    }
}

/// Resolve the model new agents should start on, with the reason it won.
///
/// Order matters: an explicit default beats everything, then env for headless
/// installs, then **a provider that actually has a key**. The last step is the
/// one that was missing. A user who added an OpenCode Go key and never opened
/// the model picker used to get `ollama/llama3.1` for every new bot, and the
/// office failed with "no model provider available" while a perfectly good
/// key sat in the database.
async fn resolve_default_model_with_source(
    pool: &sqlx::SqlitePool,
) -> (String, String, &'static str) {
    let provider = ravenbot_db::queries::AppSettingsQueries::get(pool, "default_provider")
        .await
        .ok()
        .flatten();
    let model = ravenbot_db::queries::AppSettingsQueries::get(pool, "default_model")
        .await
        .ok()
        .flatten();
    if let (Some(p), Some(m)) = (provider, model) {
        if !p.trim().is_empty() && !m.trim().is_empty() {
            return (p, m, "settings");
        }
    }

    if let Ok(p) = std::env::var("RAVENBOT_DEFAULT_PROVIDER") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            let m = std::env::var("RAVENBOT_DEFAULT_MODEL")
                .ok()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| default_model_for_provider(&p).to_string());
            return (p, m, "env");
        }
    }

    // A bot that is already configured is the strongest signal of what works
    // in this installation. The most recently updated one wins, so setting up
    // an agent and then creating an office reuses the model that agent
    // demonstrably runs on instead of an unrelated key that happens to be newer.
    if let Ok(rows) = sqlx::query_as::<_, (String, String)>(
        "SELECT json_extract(config, '$.model_provider'), json_extract(config, '$.model_id')
         FROM bots
         WHERE TRIM(COALESCE(json_extract(config, '$.model_provider'), '')) <> ''
           AND TRIM(COALESCE(json_extract(config, '$.model_id'), '')) <> ''
         ORDER BY updated_at DESC
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    {
        for (p, m) in rows {
            if !p.trim().is_empty() && !m.trim().is_empty() {
                return (p, m, "bot");
            }
        }
    }

    // A configured key is stronger evidence of intent than a compiled-in
    // fallback. Most recently updated first, so the provider someone just
    // connected is the one their first office uses.
    let custom_models: std::collections::HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, default_model FROM custom_providers")
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, model)| (id.to_ascii_lowercase(), model))
            .collect();

    if let Ok(rows) = sqlx::query_as::<_, (String,)>(
        "SELECT provider FROM provider_keys
         WHERE TRIM(api_key) <> ''
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await
    {
        for (p,) in rows {
            let p = p.trim().to_ascii_lowercase();
            if p.is_empty() {
                continue;
            }
            let model = custom_models
                .get(&p)
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| default_model_for_provider(&p).to_string());
            return (p, model, "configured");
        }
    }

    let (p, m) = default_bot_model();
    (p, m, "default")
}

/// Resolve the model new agents should start on (provider, model).
async fn resolve_default_model(pool: &sqlx::SqlitePool) -> (String, String) {
    let (provider, model, _) = resolve_default_model_with_source(pool).await;
    (provider, model)
}

#[tauri::command]
async fn create_bot(
    state: State<'_, AppState>,
    name: String,
    description: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
) -> Result<Bot, String> {
    let mut bot = Bot::new(name, description);
    if let Some(url) = avatar_url {
        bot.avatar_url = Some(url);
    }
    if let Some(style) = avatar_style {
        bot.avatar_style = Some(style);
    }
    let (default_provider, default_model) = resolve_default_model(state.db.pool()).await;
    bot.config.model_provider = default_provider;
    bot.config.model_id = default_model;

    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;

    Ok(bot)
}

/// The app setting that remembers which agent designs offices.
const DEFAULT_AGENT_SETTING: &str = "default_agent_id";
const DEFAULT_AGENT_NAME: &str = "Chief of Staff";

/// The default agent's mandate.
///
/// This is the one agent the app asks for before it can design an office. It
/// is an architect, not a worker: it reads the live capability catalog and
/// writes the charter and roster, then gets out of the way.
const DEFAULT_AGENT_PROMPT: &str = "\
You are the Chief of Staff and Office Architect for this RAVENBOT installation.

You do not do the office's work. You design offices: turn a name and a partial mission into a goal, a binding policy, and the smallest team that can deliver it, using only the tools that exist on this machine.

**What you do**
- Infer the missing mission from the name and any context you were given.
- Write a policy with checkable rules: evidence, file locations, handoffs, and honesty about failure.
- Hire 3-6 agents, exactly one of them the lead. Give every role a mandate with ownership, the artifact it produces, how it proves the work, and what it hands on.
- Assign only skills, MCP connectors, and plugins from the catalog you are given. Never invent an id.

**What you produce**
A single JSON object with `goal`, `policy`, `roles`, and `question`. No prose around it. If one detail would materially change the work, put it in `question` and still return a usable team.

**How you verify**
Every role has a real artifact and a way to prove it. Every tool id appears in the catalog. The lead can delegate. A roster of titles with no evidence is a failed design.";

fn default_agent_skills() -> Vec<String> {
    [
        "delegate",
        "research",
        "file_read",
        "file_write",
        "memory_recall",
        "memory_save",
        "todo",
        "ask_user",
        "note_manager",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// The installed office architect, if one has been chosen.
///
/// Falls back to the first orchestrator so an existing fleet does not have to
/// re-run onboarding; the choice is persisted so the same agent keeps the role.
async fn find_default_agent(pool: &sqlx::SqlitePool) -> Option<Bot> {
    if let Ok(Some(id)) =
        ravenbot_db::queries::AppSettingsQueries::get(pool, DEFAULT_AGENT_SETTING).await
    {
        if let Ok(id) = Uuid::parse_str(id.trim()) {
            if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, id).await {
                return Some(bot);
            }
        }
    }
    if let Ok(bots) = ravenbot_db::queries::BotQueries::list(pool).await {
        if let Some(bot) = bots.into_iter().find(|b| b.is_orchestrator) {
            let _ = ravenbot_db::queries::AppSettingsQueries::set(
                pool,
                DEFAULT_AGENT_SETTING,
                &bot.id.to_string(),
            )
            .await;
            return Some(bot);
        }
    }
    None
}

#[tauri::command]
async fn get_default_agent(state: State<'_, AppState>) -> Result<Option<Bot>, String> {
    Ok(find_default_agent(state.db.pool()).await)
}

#[tauri::command]
async fn set_default_agent(state: State<'_, AppState>, bot_id: Uuid) -> Result<Bot, String> {
    let mut bot = ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Agent not found".to_string())?;
    if !bot.is_orchestrator {
        bot.is_orchestrator = true;
        ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
            .await
            .map_err(|e| e.to_string())?;
    }
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        DEFAULT_AGENT_SETTING,
        &bot.id.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(bot)
}

/// Create (or adopt) the agent that designs offices.
///
/// The onboarding flow asks for this once, after a provider is connected. It
/// is a real bot, not a hidden setting: the user can rename it, change its
/// model, inspect its prompt, or replace it later from Settings.
#[tauri::command]
async fn create_default_agent(
    state: State<'_, AppState>,
    name: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<Bot, String> {
    let name = name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(DEFAULT_AGENT_NAME)
        .to_string();
    let (provider, model) = match (
        provider.as_deref().map(str::trim).filter(|p| !p.is_empty()),
        model.as_deref().map(str::trim).filter(|m| !m.is_empty()),
    ) {
        (Some(p), Some(m)) => (p.to_string(), m.to_string()),
        _ => resolve_default_model(state.db.pool()).await,
    };

    // Never create an agent that cannot answer. Check both the live manager
    // (hydrated at startup) and the database, so opening onboarding before the
    // manager has finished hydrating does not produce a false "no key" error.
    let manager_has = {
        let manager = state.runtime.provider_manager().lock().await;
        manager.has_key(&provider)
    };
    let db_has = ravenbot_db::queries::ProviderKeyQueries::get(state.db.pool(), &provider)
        .await
        .ok()
        .flatten()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);
    let has_key = manager_has
        || db_has
        || provider.eq_ignore_ascii_case("ollama")
        || provider.eq_ignore_ascii_case("local");
    if !has_key {
        return Err(format!(
            "No API key is configured for {provider}. Connect it in Settings first."
        ));
    }

    let existing = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|b| b.name.eq_ignore_ascii_case(&name));

    let bot = if let Some(mut bot) = existing {
        bot.config.model_provider = provider;
        bot.config.model_id = model;
        bot.is_orchestrator = true;
        bot.approval_mode = ravenbot_core::ApprovalMode::Full;
        if bot.rank.as_deref().map(str::trim).unwrap_or("").is_empty() {
            bot.rank = Some("Chief of Staff".to_string());
        }
        if bot
            .specialty
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            bot.specialty = Some("Office design and orchestration".to_string());
        }
        if bot
            .config
            .custom_prompt
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .is_none()
        {
            bot.config.custom_prompt = Some(DEFAULT_AGENT_PROMPT.to_string());
        }
        if bot.skills.is_empty() {
            bot.skills = default_agent_skills();
        }
        ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
            .await
            .map_err(|e| e.to_string())?;
        bot
    } else {
        let mut bot = Bot::new(&name, "Office design and orchestration");
        bot.rank = Some("Chief of Staff".to_string());
        bot.specialty = Some("Office design and orchestration".to_string());
        bot.is_orchestrator = true;
        bot.approval_mode = ravenbot_core::ApprovalMode::Full;
        bot.avatar_color = "#8b5cf6".to_string();
        bot.config.model_provider = provider;
        bot.config.model_id = model;
        bot.config.custom_prompt = Some(DEFAULT_AGENT_PROMPT.to_string());
        bot.skills = default_agent_skills();
        ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
            .await
            .map_err(|e| e.to_string())?;
        bot
    };

    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        DEFAULT_AGENT_SETTING,
        &bot.id.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(bot)
}

#[tauri::command]
async fn list_bots(state: State<'_, AppState>) -> Result<Vec<Bot>, String> {
    ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<Option<Bot>, String> {
    ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// List recent runs for diagnostics, newest first.
#[tauri::command]
async fn list_runs(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<serde_json::Value, String> {
    let limit = limit.unwrap_or(50).min(200) as i64;
    let rows: Vec<(String, String, String, String, i64, f64, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, bot_id, thread_id, state, tokens_consumed, cost_estimate, created_at, updated_at, completed_at, outcome FROM runs ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, bot_id, thread_id, state, tokens, cost, created, updated, completed, outcome) in rows {
        out.push(serde_json::json!({
            "id": id,
            "bot_id": bot_id,
            "thread_id": thread_id,
            "state": state,
            "tokens_consumed": tokens,
            "cost_estimate": cost,
            "created_at": created,
            "updated_at": updated,
            "completed_at": completed,
            "outcome": outcome,
        }));
    }
    Ok(serde_json::Value::Array(out))
}

/// Fleet usage over a window. `days = 0` means all recorded runs.
#[tauri::command]
async fn get_usage_summary(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<serde_json::Value, String> {
    let days = days.unwrap_or(30).min(3650);
    let rows = ravenbot_db::queries::UsageQueries::summary(state.db.pool(), days)
        .await
        .map_err(|e| e.to_string())?;

    let mut agents: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(bot_id, runs, tokens, cost, failed, completed)| {
            serde_json::json!({
                "bot_id": bot_id,
                "runs": runs,
                "tokens": tokens,
                "cost": cost,
                "failed": failed,
                "completed": completed,
            })
        })
        .collect();
    agents.sort_by(|left, right| {
        right["cost"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&left["cost"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_runs: i64 = agents.iter().filter_map(|agent| agent["runs"].as_i64()).sum();
    let total_tokens: i64 = agents.iter().filter_map(|agent| agent["tokens"].as_i64()).sum();
    let total_cost: f64 = agents.iter().filter_map(|agent| agent["cost"].as_f64()).sum();
    let failed_runs: i64 = agents.iter().filter_map(|agent| agent["failed"].as_i64()).sum();
    let completed_runs: i64 = agents
        .iter()
        .filter_map(|agent| agent["completed"].as_i64())
        .sum();
    Ok(serde_json::json!({
        "days": days,
        "total_runs": total_runs,
        "total_tokens": total_tokens,
        "total_cost": total_cost,
        "failed_runs": failed_runs,
        "completed_runs": completed_runs,
        "agents": agents,
    }))
}

#[tauri::command]
async fn update_bot(state: State<'_, AppState>, bot: Bot) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::delete(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Contact state for the fleet list: pinned / hidden / last read.
#[tauri::command]
async fn list_bot_contacts(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let rows = ravenbot_db::queries::BotContactQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|(bot_id, pinned, hidden, last_read_at)| {
                serde_json::json!({
                    "bot_id": bot_id,
                    "pinned": pinned,
                    "hidden": hidden,
                    "last_read_at": last_read_at,
                })
            })
            .collect(),
    ))
}

#[tauri::command]
async fn set_bot_pinned(
    state: State<'_, AppState>,
    bot_id: Uuid,
    pinned: bool,
) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::set_pinned(state.db.pool(), bot_id, pinned)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_bot_hidden(
    state: State<'_, AppState>,
    bot_id: Uuid,
    hidden: bool,
) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::set_hidden(state.db.pool(), bot_id, hidden)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn mark_bot_read(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::mark_read(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Persist manual sidebar order: each bot id gets sort_order = its position.
#[tauri::command]
async fn reorder_bots(state: State<'_, AppState>, ordered_ids: Vec<Uuid>) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::reorder(state.db.pool(), &ordered_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Unread assistant-message count per bot (drives sidebar badges).
#[tauri::command]
async fn get_unread_counts(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let rows = ravenbot_db::queries::BotContactQueries::unread_counts(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut map = serde_json::Map::new();
    for (bot_id, count) in rows {
        if count > 0 {
            map.insert(bot_id.to_string(), serde_json::json!(count));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Clone an agent (identity, model, skills, permissions) as an independent bot.
#[tauri::command]
async fn duplicate_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<Bot, String> {
    let source = ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Bot not found".to_string())?;
    let mut bot = source.clone();
    let now = chrono::Utc::now();
    bot.id = Uuid::new_v4();
    bot.name = format!("{} (copy)", source.name);
    bot.description = source.description.clone();
    bot.is_orchestrator = false;
    bot.delegate_to = Vec::new();
    bot.status = ravenbot_core::BotStatus::Idle;
    bot.created_at = now;
    bot.updated_at = now;
    bot.last_active_at = None;
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;
    let _ = ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool()).await;
    Ok(bot)
}

#[tauri::command]
async fn create_thread(
    state: State<'_, AppState>,
    bot_id: Uuid,
    title: String,
    ephemeral: Option<bool>,
    channel_id: Option<Uuid>,
) -> Result<Thread, String> {
    let mut thread = if ephemeral.unwrap_or(false) {
        Thread::new_ephemeral(bot_id, title)
    } else {
        Thread::new(bot_id, title)
    };
    thread.channel_id = channel_id;
    ravenbot_db::queries::ThreadQueries::create(state.db.pool(), &thread)
        .await
        .map_err(|e| e.to_string())?;
    Ok(thread)
}

#[tauri::command]
async fn list_threads(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<Thread>, String> {
    ravenbot_db::queries::ThreadQueries::list_by_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_messages(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<Message>, String> {
    ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_message_reaction(
    state: State<'_, AppState>,
    message_id: Uuid,
    emoji: String,
) -> Result<Vec<String>, String> {
    // A reaction is a short emoji sequence, not free text. The limit is a
    // grapheme cluster like a family or a flag, never a sentence.
    let emoji = emoji.trim();
    if emoji.is_empty() {
        return Err("A reaction needs an emoji.".to_string());
    }
    if emoji.chars().count() > 16 {
        return Err("That reaction is too long.".to_string());
    }
    ravenbot_db::queries::ReactionQueries::toggle(state.db.pool(), message_id, emoji)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_thread(
    state: State<'_, AppState>,
    thread_id: Uuid,
    title: String,
) -> Result<(), String> {
    ravenbot_db::queries::ThreadQueries::rename(state.db.pool(), thread_id, &title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_thread(state: State<'_, AppState>, thread_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ThreadQueries::delete(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_messages(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    let results = ravenbot_db::queries::SearchQueries::messages_with_bot(
        state.db.pool(),
        &query,
        limit.unwrap_or(20),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|(message, thread_title, bot_id)| {
            let snippet = match &message.content {
                ravenbot_core::MessageContent::Text { text, .. } => {
                    let lower = text.to_lowercase();
                    let ql = query.to_lowercase();
                    let pos = lower.find(&ql).unwrap_or(0);
                    let start = pos.saturating_sub(60);
                    let snippet: String = text.chars().skip(start).take(160).collect();
                    snippet
                }
                _ => String::new(),
            };
            serde_json::json!({
                "message_id": message.id.to_string(),
                "thread_id": message.thread_id.to_string(),
                "bot_id": bot_id,
                "thread_title": thread_title,
                "role": match message.role {
                    ravenbot_core::MessageRole::User => "user",
                    ravenbot_core::MessageRole::Assistant => "assistant",
                    ravenbot_core::MessageRole::System => "system",
                    ravenbot_core::MessageRole::Tool => "tool",
                },
                "snippet": snippet,
                "created_at": message.created_at.to_rfc3339(),
            })
        })
        .collect())
}

/// Build a stream emitter that forwards runtime StreamEvents to the UI
/// via the `agent-stream` Tauri event channel.
fn make_stream_emitter(app: tauri::AppHandle) -> ravenbot_runtime::StreamEmitter {
    use ravenbot_runtime::StreamEvent;
    Arc::new(move |event: StreamEvent| {
        if let Ok(payload) = serde_json::to_value(&event) {
            let _ = app.emit("agent-stream", payload);
        }
    })
}

/// Execute a routine instruction: create a thread, insert the instruction
/// as the user turn, and run the bot with streaming wired for that thread.
pub async fn execute_routine_instruction(
    db: &Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    app: &tauri::AppHandle,
    routine: &ravenbot_core::Routine,
) -> Result<(), String> {
    let thread = Thread::new(routine.bot_id, format!("Routine: {}", routine.name));
    ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|e| e.to_string())?;

    let user_msg = Message::user(thread.id, &routine.instruction);
    ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    let mut run = Run::new(routine.bot_id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    runtime.set_thread_emitter(thread.id, Some(make_stream_emitter(app.clone())));
    // Routines run unattended: auto-allow approval gates for THIS run (audited)
    // rather than parking for 10 minutes and denying.
    runtime.allow_approvals_for_run(run.id, true);
    let result = runtime.execute_run(&mut run).await;
    runtime.allow_approvals_for_run(run.id, false);
    runtime.set_thread_emitter(thread.id, None);
    result.map_err(|e| e.to_string())?;

    let _ = app.emit(
        "routine-executed",
        serde_json::json!({
            "routine_id": routine.id.to_string(),
            "thread_id": thread.id.to_string(),
            "bot_id": routine.bot_id.to_string(),
            "ok": true
        }),
    );

    Ok(())
}

// ---- Sync (signed bundle export/import) commands ----

#[tauri::command]
async fn export_bot_bundle(
    state: State<'_, AppState>,
    bot_id: Uuid,
    include_memory: Option<bool>,
) -> Result<ravenbot_core::BotBundle, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager
        .export_bot(bot_id, include_memory.unwrap_or(true))
        .await
}

/// Edit a user message and resend: deletes this turn and everything after it,
/// inserts the edited user turn, and executes a fresh run. This is the
/// "edit message" flow (Grok-style).
#[tauri::command]
async fn edit_and_resend(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
    message_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<Message, String> {
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let idx = messages
        .iter()
        .position(|m| m.id == message_id)
        .ok_or_else(|| "Message not found".to_string())?;

    if !matches!(messages[idx].role, ravenbot_core::MessageRole::User) {
        return Err("Only user messages can be edited".to_string());
    }
    let original_reply = messages[idx].reply_to_id;

    // Delete this turn and everything after it
    for msg in &messages[idx..] {
        ravenbot_db::queries::MessageQueries::delete(state.db.pool(), msg.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Insert the edited user turn (with any new inline image attachments)
    let mut user_msg = Message::user(thread_id, &content);
    if let Some(atts) = attachments {
        user_msg.attachments = build_attachments(atts);
    }
    user_msg.reply_to_id = original_reply;
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    let result = execute_run_with_stream(&state, &app, &mut run).await;

    if let Err(err) = result {
        let err_str = err.to_string();
        tracing::warn!("Edit-and-resend run error: {}", err_str);
        let error_msg = Message::assistant(
            thread_id,
            format!(
                "⚠️ **Model Error:** {}\n\n{}{}",
                err_str,
                error_hint(&err_str),
                "\n\nYou can also run me fully offline via a local Ollama model."
            ),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
        );
        return Ok(error_msg);
    }

    let _ = app.emit(
        "agent-stream",
        serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
    );

    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;
    messages
        .last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

#[tauri::command]
async fn import_bot_bundle(
    state: State<'_, AppState>,
    bundle_json: String,
) -> Result<Uuid, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager.import_from_json(&bundle_json).await
}

#[tauri::command]
async fn import_bot_bundle_from_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<Uuid, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager.import_from_file(&path).await
}

// ---- Routines (scheduler) commands ----

#[tauri::command]
async fn create_routine(
    state: State<'_, AppState>,
    bot_id: Uuid,
    name: String,
    schedule: String,
    _description: String,
    instruction: String,
) -> Result<ravenbot_core::Routine, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.create(bot_id, &name, &schedule, &instruction).await
}

#[tauri::command]
async fn get_routine(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<Option<ravenbot_core::Routine>, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.get(routine_id).await
}

#[tauri::command]
async fn list_routines(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<ravenbot_core::Routine>, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.list_for_bot(bot_id).await
}

#[tauri::command]
async fn update_routine(
    state: State<'_, AppState>,
    routine: ravenbot_core::Routine,
) -> Result<(), String> {
    // Validate the schedule before persisting
    ravenbot_scheduler::cron::CronParser::parse(&routine.schedule)?;
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.update(&routine).await
}

#[tauri::command]
async fn delete_routine(state: State<'_, AppState>, routine_id: Uuid) -> Result<(), String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.delete(routine_id).await
}

#[tauri::command]
async fn get_scheduler_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let running = state.scheduler.is_running().await;
    Ok(serde_json::json!({
        "running": running,
        "check_interval_secs": 60,
        "max_concurrent": 5
    }))
}

#[tauri::command]
async fn run_routine_now(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    routine_id: Uuid,
) -> Result<(), String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    let routine = manager
        .get(routine_id)
        .await?
        .ok_or_else(|| "Routine not found".to_string())?;
    execute_routine_instruction(&state.db, &state.runtime, &app, &routine).await
}

// ---- Webhook triggers ----

/// Enable (or rotate) a routine's inbound webhook. Returns the URL + secret
/// **once** so the caller can copy them; the secret is never re-exposed.
#[tauri::command]
async fn enable_routine_webhook(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<serde_json::Value, String> {
    // 32 hex chars from a v4 UUID pair — enough entropy for a bearer secret.
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    ravenbot_db::queries::WebhookQueries::set(state.db.pool(), routine_id, Some(&secret), true)
        .await
        .map_err(|e| e.to_string())?;

    let base = state
        .webhook_base_url
        .read()
        .map(|u| u.clone())
        .unwrap_or_else(|_| "http://127.0.0.1:8800".to_string());
    Ok(serde_json::json!({
        "url": format!("{}/hooks/{}", base, secret),
        "secret": secret,
        "header": format!("Bearer {}", secret),
    }))
}

#[tauri::command]
async fn disable_routine_webhook(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<(), String> {
    ravenbot_db::queries::WebhookQueries::set(state.db.pool(), routine_id, None, false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_routine_webhook_status(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<serde_json::Value, String> {
    let enabled = ravenbot_db::queries::WebhookQueries::secret(state.db.pool(), routine_id)
        .await
        .map_err(|e| e.to_string())?
        .is_some();
    let base = state
        .webhook_base_url
        .read()
        .map(|u| u.clone())
        .unwrap_or_else(|_| "http://127.0.0.1:8800".to_string());
    Ok(serde_json::json!({ "enabled": enabled, "base_url": base }))
}

// ---- Team packages (install a team from Markdown) ----

/// Parse a team Markdown file and return a review summary, without creating
/// anything. `source_url` is accepted so the UI can show where it came from.
#[tauri::command]
async fn preview_team(markdown: String) -> Result<serde_json::Value, String> {
    let team = TeamPackage::parse(&markdown).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "name": team.name,
        "description": team.description,
        "summary": team.summary(),
        "bots": team.bots.iter().map(|b| serde_json::json!({
            "name": b.name,
            "title": b.title,
            "description": b.description,
            "rank": b.rank,
            "model": b.model,
        })).collect::<Vec<_>>(),
        "office": team.office.as_ref().map(|o| serde_json::json!({
            "name": o.name,
            "template": o.template,
            "goal": o.goal,
        })),
        "routines": team.routines.iter().map(|r| serde_json::json!({
            "name": r.name,
            "bot": r.bot,
            "schedule": r.schedule,
        })).collect::<Vec<_>>(),
    }))
}

/// Fetch a team Markdown file from a URL (marketplace install) and preview it.
#[tauri::command]
async fn fetch_and_preview_team(url: String) -> Result<serde_json::Value, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    let body = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    preview_team(body).await
}

/// Create the team: bots, an optional office, and paused routines.
#[tauri::command]
async fn import_team(
    state: State<'_, AppState>,
    markdown: String,
) -> Result<serde_json::Value, String> {
    import_team_into(state.db.pool(), &markdown).await
}

/// Core team importer (testable without Tauri state): bots, optional office,
/// paused routines.
async fn import_team_into(
    pool: &sqlx::SqlitePool,
    markdown: &str,
) -> Result<serde_json::Value, String> {
    let team = TeamPackage::parse(markdown).map_err(|e| e.to_string())?;

    // 1. Create bots, remembering name → id for routines.
    let mut name_to_id: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
    let mut created_bots = Vec::new();
    for spec in &team.bots {
        let mut bot = Bot::new(
            spec.name.clone(),
            spec.description.clone().unwrap_or_default(),
        );
        if let Some(title) = &spec.title {
            bot.specialty = Some(title.clone());
        }
        if let Some(rank) = &spec.rank {
            bot.rank = Some(rank.clone());
        }
        if let Some(prompt) = &spec.prompt {
            bot.config.custom_prompt = Some(prompt.clone());
        }
        if let Some(model) = &spec.model {
            // Accept "provider/model" or a bare provider.
            if let Some((provider, id)) = model.split_once('/') {
                bot.config.model_provider = provider.to_string();
                bot.config.model_id = id.to_string();
            } else {
                bot.config.model_provider = model.clone();
            }
        }
        if let Some(style) = &spec.avatar_style {
            bot.avatar_style = Some(style.clone());
        }
        ravenbot_db::queries::BotQueries::insert(pool, &bot)
            .await
            .map_err(|e| e.to_string())?;
        name_to_id.insert(spec.name.to_lowercase(), bot.id);
        created_bots.push(serde_json::json!({ "name": bot.name, "id": bot.id.to_string() }));
    }

    // 2. Optional office binding every bot.
    let mut office_id: Option<Uuid> = None;
    if let Some(office) = &team.office {
        let template = office
            .template
            .clone()
            .unwrap_or_else(|| "custom".to_string());
        let mut room = ChatRoom::new(
            office.name.clone(),
            office.description.clone().unwrap_or_default(),
            template,
        );
        room.goal = office.goal.clone();
        room.policy = office.policy.clone();
        ravenbot_db::queries::ChatRoomQueries::create(pool, &room)
            .await
            .map_err(|e| e.to_string())?;
        for bot_id in name_to_id.values() {
            let member = ChatRoomMember {
                chatroom_id: room.id,
                bot_id: *bot_id,
                rank: team
                    .bots
                    .iter()
                    .find(|b| name_to_id.get(&b.name.to_lowercase()) == Some(bot_id))
                    .and_then(|b| b.rank.clone())
                    .unwrap_or_else(|| "specialist".to_string()),
                specialty: team
                    .bots
                    .iter()
                    .find(|b| name_to_id.get(&b.name.to_lowercase()) == Some(bot_id))
                    .and_then(|b| b.title.clone())
                    .unwrap_or_default(),
                joined_at: chrono::Utc::now(),
            };
            ravenbot_db::queries::ChatRoomQueries::add_member(pool, &member)
                .await
                .map_err(|e| e.to_string())?;
        }
        office_id = Some(room.id);
    }

    // 3. Routines — created **disabled** so a freshly imported team never
    //    starts running work unattended (OpenMausBot parity).
    let manager = ravenbot_scheduler::routine::RoutineManager::new(pool.clone());
    let mut created_routines = Vec::new();
    for spec in &team.routines {
        let Some(bot_id) = name_to_id.get(&spec.bot.to_lowercase()).copied() else {
            continue;
        };
        let schedule = spec
            .schedule
            .clone()
            .unwrap_or_else(|| "0 9 * * *".to_string());
        if let Ok(mut routine) = manager
            .create(bot_id, &spec.name, &schedule, &spec.instruction)
            .await
        {
            routine.is_enabled = false;
            let _ = manager.update(&routine).await;
            created_routines
                .push(serde_json::json!({ "name": routine.name, "id": routine.id.to_string() }));
        }
    }

    Ok(serde_json::json!({
        "name": team.name,
        "bots": created_bots,
        "office_id": office_id.map(|u| u.to_string()),
        "routines": created_routines,
        "playbook": team.playbook,
    }))
}

// ---- Channels (named contexts) ----

#[tauri::command]
async fn list_channels(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let channels = ravenbot_db::queries::ChannelQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for c in channels {
        let bots = ravenbot_db::queries::ChannelQueries::bots(state.db.pool(), c.id)
            .await
            .unwrap_or_default();
        out.push(serde_json::json!({
            "id": c.id.to_string(),
            "name": c.name,
            "description": c.description,
            "instructions": c.instructions,
            "working_folder": c.working_folder,
            "color": c.color,
            "position": c.position,
            "bot_ids": bots.iter().map(|b| b.to_string()).collect::<Vec<_>>(),
        }));
    }
    Ok(out)
}

#[tauri::command]
async fn create_channel(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    working_folder: Option<String>,
    color: Option<String>,
    bot_ids: Option<Vec<Uuid>>,
) -> Result<String, String> {
    let mut channel = Channel::new(name);
    channel.description = description.unwrap_or_default();
    channel.instructions = instructions.unwrap_or_default();
    channel.working_folder = working_folder;
    channel.color = color;
    ravenbot_db::queries::ChannelQueries::create(state.db.pool(), &channel)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(bots) = bot_ids {
        ravenbot_db::queries::ChannelQueries::set_bots(state.db.pool(), channel.id, &bots)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(channel.id.to_string())
}

#[tauri::command]
async fn update_channel(state: State<'_, AppState>, channel: Channel) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::update(state.db.pool(), &channel)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_channel(state: State<'_, AppState>, channel_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::delete(state.db.pool(), channel_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_channel_bots(
    state: State<'_, AppState>,
    channel_id: Uuid,
    bot_ids: Vec<Uuid>,
) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::set_bots(state.db.pool(), channel_id, &bot_ids)
        .await
        .map_err(|e| e.to_string())
}

// ── Team channel timeline and task ownership ─────────────────────────────
//
// The channel row already owns the roster and instructions. These commands add
// the thing a team actually returns to: a shared timeline, and tasks whose
// owner is a single database row rather than a convention.

#[tauri::command]
async fn list_channel_messages(
    state: State<'_, AppState>,
    channel_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<ravenbot_core::ChannelMessage>, String> {
    ravenbot_db::queries::ChannelMessageQueries::list_recent(
        state.db.pool(),
        channel_id,
        limit.unwrap_or(200),
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn post_channel_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    channel_id: Uuid,
    content: String,
    author_name: Option<String>,
) -> Result<ravenbot_core::ChannelMessage, String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("A channel message cannot be empty.".to_string());
    }
    let message = ravenbot_core::ChannelMessage::human(
        channel_id,
        author_name.unwrap_or_else(|| "You".to_string()),
        content,
    );
    ravenbot_db::queries::ChannelMessageQueries::insert(state.db.pool(), &message)
        .await
        .map_err(|error| error.to_string())?;
    let _ = app.emit(
        "channel-event",
        serde_json::json!({
            "kind": "message",
            "channel_id": channel_id.to_string(),
            "message": message,
        }),
    );
    Ok(message)
}

#[tauri::command]
async fn list_channel_tasks(
    state: State<'_, AppState>,
    channel_id: Uuid,
    include_archived: Option<bool>,
) -> Result<Vec<ravenbot_core::ChannelTask>, String> {
    ravenbot_db::queries::ChannelTaskQueries::list(
        state.db.pool(),
        channel_id,
        include_archived.unwrap_or(true),
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_channel_task(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    channel_id: Uuid,
    title: String,
    description: String,
    owner_bot_id: Uuid,
) -> Result<ravenbot_core::ChannelTask, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("A task title is required.".to_string());
    }
    let roster = ravenbot_db::queries::ChannelQueries::bots(state.db.pool(), channel_id)
        .await
        .map_err(|error| error.to_string())?;
    if !roster.contains(&owner_bot_id) {
        return Err("The task owner must be a member of this channel.".to_string());
    }
    let task = ravenbot_core::ChannelTask::new(channel_id, title, description, owner_bot_id, "You");
    ravenbot_db::queries::ChannelTaskQueries::create(state.db.pool(), &task)
        .await
        .map_err(|error| error.to_string())?;
    ravenbot_db::queries::ChannelTaskQueries::insert_event(
        state.db.pool(),
        &ravenbot_core::ChannelTaskEvent {
            id: Uuid::new_v4(),
            task_id: task.id,
            kind: "created".to_string(),
            actor: "You".to_string(),
            from_owner: None,
            to_owner: Some(owner_bot_id),
            note: None,
            created_at: chrono::Utc::now(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let _ = app.emit(
        "channel-event",
        serde_json::json!({
            "kind": "task_created",
            "channel_id": channel_id.to_string(),
            "task": task,
        }),
    );
    Ok(task)
}

/// Start a task on its owner's thread. The claim happens before any work, so
/// two clicks (or two surfaces) cannot start two runs for one task.
async fn start_channel_task_inner(
    app: &tauri::AppHandle,
    db: &Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    task_id: Uuid,
) -> Result<serde_json::Value, String> {
    let task = ravenbot_db::queries::ChannelTaskQueries::get(db.pool(), task_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Channel task not found.".to_string())?;
    if task.state == ravenbot_core::ChannelTaskState::Archived {
        return Err("Archived tasks must be restored before they can run.".to_string());
    }
    let channel = ravenbot_db::queries::ChannelQueries::get(db.pool(), task.channel_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Channel not found.".to_string())?;
    let bot = ravenbot_db::queries::BotQueries::get(db.pool(), task.owner_bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Task owner not found.".to_string())?;

    if !ravenbot_db::queries::ChannelTaskQueries::claim(db.pool(), task_id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err(format!(
            "The task is already {} and cannot be started again.",
            task.state.as_str()
        ));
    }
    let setup = async {
        let mut thread =
            Thread::new(task.owner_bot_id, format!("Channel task: {}", task.title));
        thread.channel_id = Some(task.channel_id);
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .map_err(|error| error.to_string())?;
        ravenbot_db::queries::ChannelTaskQueries::set_thread(db.pool(), task.id, thread.id)
            .await
            .map_err(|error| error.to_string())?;

        let prompt = format!(
            "You are working in the team channel \"{}\".\n\nChannel instructions:\n{}\n\nTask: {}\n\n{}\n\nDo the work, then report the result concisely. This answer is posted to the shared channel.",
            channel.name,
            if channel.instructions.trim().is_empty() {
                "(none)"
            } else {
                channel.instructions.trim()
            },
            task.title,
            task.description
        );
        let user_msg = Message::user(thread.id, prompt);
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
            .await
            .map_err(|error| error.to_string())?;
        let run = Run::new(task.owner_bot_id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((thread, run))
    }
    .await;
    let (thread, mut run) = match setup {
        Ok(value) => value,
        Err(error) => {
            let _ = ravenbot_db::queries::ChannelTaskQueries::finish(
                db.pool(),
                task_id,
                false,
                Some(&error),
            )
            .await;
            return Err(error);
        }
    };

    let started = ravenbot_core::ChannelMessage::system(
        task.channel_id,
        format!("{} started “{}”.", bot.name, task.title),
    );
    let _ = ravenbot_db::queries::ChannelMessageQueries::insert(db.pool(), &started).await;
    let _ = app.emit(
        "channel-event",
        serde_json::json!({
            "kind": "task_started",
            "channel_id": task.channel_id.to_string(),
            "task_id": task.id.to_string(),
            "thread_id": thread.id.to_string(),
            "message": started,
        }),
    );

    let spawn_app = app.clone();
    let spawn_runtime = runtime.clone();
    let task_id = task.id;
    let channel_id = task.channel_id;
    let owner_bot_id = task.owner_bot_id;
    let owner_name = bot.name.clone();
    let query_db = db.clone();
    tauri::async_runtime::spawn(async move {
        spawn_runtime.set_thread_emitter(thread.id, Some(make_stream_emitter(spawn_app.clone())));
        spawn_runtime.allow_approvals_for_run(run.id, true);
        let result = spawn_runtime.execute_run(&mut run).await;
        spawn_runtime.allow_approvals_for_run(run.id, false);
        spawn_runtime.set_thread_emitter(thread.id, None);

        // The user may have stopped the task while the model was still
        // generating. In that case the row is no longer running and the
        // answer must not be posted over the Stop.
        let still_running = ravenbot_db::queries::ChannelTaskQueries::get(query_db.pool(), task_id)
            .await
            .ok()
            .flatten()
            .map(|task| task.state == ravenbot_core::ChannelTaskState::Running)
            .unwrap_or(false);
        if !still_running {
            let _ = spawn_app.emit(
                "channel-event",
                serde_json::json!({
                    "kind": "task_stopped",
                    "channel_id": channel_id.to_string(),
                    "task_id": task_id.to_string(),
                }),
            );
            return;
        }

        let answer =
            match ravenbot_db::queries::MessageQueries::list_by_thread(query_db.pool(), thread.id)
                .await
            {
                Ok(messages) => messages
                    .iter()
                    .rev()
                    .find(|message| matches!(message.role, ravenbot_core::MessageRole::Assistant))
                    .and_then(|message| match &message.content {
                        ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
        let (ok, outcome) = match &result {
            Ok(()) if !answer.trim().is_empty() => (true, answer),
            Ok(()) => (
                false,
                "The agent finished without writing an answer.".to_string(),
            ),
            Err(error) => (false, format!("The run failed: {error}")),
        };
        let _ = ravenbot_db::queries::ChannelTaskQueries::finish(
            query_db.pool(),
            task_id,
            ok,
            Some(&outcome),
        )
        .await;
        let message = ravenbot_core::ChannelMessage::agent(
            channel_id,
            owner_bot_id,
            owner_name,
            outcome.clone(),
        );
        let _ =
            ravenbot_db::queries::ChannelMessageQueries::insert(query_db.pool(), &message).await;
        let _ = ravenbot_db::queries::ChannelTaskQueries::insert_event(
            query_db.pool(),
            &ravenbot_core::ChannelTaskEvent {
                id: Uuid::new_v4(),
                task_id,
                kind: if ok {
                    "finished".to_string()
                } else {
                    "failed".to_string()
                },
                actor: "RAVENBOT".to_string(),
                from_owner: Some(owner_bot_id),
                to_owner: Some(owner_bot_id),
                note: Some(outcome.clone()),
                created_at: chrono::Utc::now(),
            },
        )
        .await;
        let _ = spawn_app.emit(
            "channel-event",
            serde_json::json!({
                "kind": if ok { "task_finished" } else { "task_failed" },
                "channel_id": channel_id.to_string(),
                "task_id": task_id.to_string(),
                "thread_id": thread.id.to_string(),
                "message": message,
                "ok": ok,
            }),
        );
    });

    Ok(serde_json::json!({
        "task_id": task.id.to_string(),
        "thread_id": thread.id.to_string(),
        "bot_id": task.owner_bot_id.to_string(),
    }))
}

#[tauri::command]
async fn start_channel_task(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    task_id: Uuid,
) -> Result<serde_json::Value, String> {
    start_channel_task_inner(&app, &state.db, &state.runtime, task_id).await
}

#[tauri::command]
async fn channel_task_action(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    task_id: Uuid,
    action: String,
    owner_bot_id: Option<Uuid>,
    note: Option<String>,
) -> Result<ravenbot_core::ChannelTask, String> {
    if action == "start" {
        start_channel_task_inner(&app, &state.db, &state.runtime, task_id).await?;
        return ravenbot_db::queries::ChannelTaskQueries::get(state.db.pool(), task_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Channel task not found.".to_string());
    }
    // Stop means stop the run, not only the row. The run's cancellation flag
    // is what prevents a background task from posting a result after the user
    // has already pressed Stop.
    if action == "stop" || action == "pause" {
        if let Ok(Some(task)) =
            ravenbot_db::queries::ChannelTaskQueries::get(state.db.pool(), task_id).await
        {
            if let Some(thread_id) = task.source_thread_id {
                if let Ok(Some(run_id)) = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM runs WHERE thread_id = ? AND state IN ('planning','acting','observing','reflecting','waiting_on_user') ORDER BY created_at DESC LIMIT 1",
                )
                .bind(thread_id.to_string())
                .fetch_optional(state.db.pool())
                .await
                {
                    if let Ok(run_id) = Uuid::parse_str(&run_id) {
                        state.runtime.request_cancel(run_id);
                    }
                }
            }
        }
    }
    let task = ravenbot_db::queries::ChannelTaskQueries::apply(
        state.db.pool(),
        task_id,
        &action,
        "You",
        owner_bot_id,
        note,
    )
    .await?;
    let _ = app.emit(
        "channel-event",
        serde_json::json!({
            "kind": "task_changed",
            "channel_id": task.channel_id.to_string(),
            "task": task,
            "action": action,
        }),
    );
    Ok(task)
}

#[tauri::command]
async fn list_channel_task_events(
    state: State<'_, AppState>,
    task_id: Uuid,
) -> Result<Vec<ravenbot_core::ChannelTaskEvent>, String> {
    ravenbot_db::queries::ChannelTaskQueries::events(state.db.pool(), task_id)
        .await
        .map_err(|error| error.to_string())
}

/// Resolve which bot should answer in a channel under `lead` rules.
async fn channel_lead_bot(
    pool: &sqlx::SqlitePool,
    channel_id: Uuid,
    rules: &serde_json::Value,
) -> Option<Uuid> {
    if let Some(id) = rules
        .get("lead_bot_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        return Some(id);
    }
    let bots = ravenbot_db::queries::ChannelQueries::bots(pool, channel_id)
        .await
        .ok()?;
    let mut first: Option<Uuid> = None;
    for id in bots {
        if first.is_none() {
            first = Some(id);
        }
        if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, id).await {
            if bot.is_orchestrator {
                return Some(id);
            }
            let rank = bot.rank.unwrap_or_default().to_lowercase();
            if rank.contains("lead") || rank.contains("chief") || rank.contains("manager") {
                return Some(id);
            }
        }
    }
    first
}

/// Whether the message @mentions this bot by name (manual responder mode).
async fn channel_bot_mentioned(pool: &sqlx::SqlitePool, bot_id: Uuid, content: &str) -> bool {
    if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, bot_id).await {
        let needle = format!("@{}", bot.name.to_lowercase());
        return content.to_lowercase().contains(&needle);
    }
    false
}

/// Is this message part of that conversation?
///
/// A reply target arrives from the renderer, so it is untrusted input: a quote
/// must not be able to point at a message in another thread and leak its text
/// into a conversation that never contained it.
async fn message_in_thread(
    pool: &sqlx::SqlitePool,
    message_id: Uuid,
    thread_id: Uuid,
) -> Result<bool, String> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE id = ? AND thread_id = ?",
    )
    .bind(message_id.to_string())
    .bind(thread_id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
    reply_to_id: Option<Uuid>,
) -> Result<Message, String> {
    // Save user message (with any inline image attachments)
    let mut user_msg = Message::user(thread_id, &content);
    if let Some(atts) = attachments {
        user_msg.attachments = build_attachments(atts);
    }
    if let Some(reply_id) = reply_to_id {
        if !message_in_thread(state.db.pool(), reply_id, thread_id).await? {
            return Err("The quoted message is not part of this conversation.".to_string());
        }
        user_msg.reply_to_id = Some(reply_id);
    }
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    // Get thread to find bot
    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    // ── Channel responder gate ────────────────────────────────────────────
    // A thread filed under a channel obeys the channel's responder rules:
    // `all` (default) lets this bot answer, `lead` only lets the channel lead
    // answer, `manual` requires an @mention of this bot.
    let channel_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<String>>("SELECT channel_id FROM threads WHERE id = ?")
            .bind(thread_id.to_string())
            .fetch_optional(state.db.pool())
            .await
            .ok()
            .flatten()
            .flatten()
            .and_then(|s| Uuid::parse_str(&s).ok());
    if let Some(cid) = channel_id {
        if let Ok(Some(channel)) =
            ravenbot_db::queries::ChannelQueries::get(state.db.pool(), cid).await
        {
            if let Some(rules) = channel.responder_rules.as_ref() {
                let mode = rules.get("mode").and_then(|v| v.as_str()).unwrap_or("all");
                let allowed = match mode {
                    "lead" => channel_lead_bot(state.db.pool(), cid, rules)
                        .await
                        .map(|lead| lead == thread.bot_id)
                        .unwrap_or(true),
                    "manual" => {
                        channel_bot_mentioned(state.db.pool(), thread.bot_id, &content).await
                    }
                    _ => true,
                };
                if !allowed {
                    let note = Message::assistant(
                        thread_id,
                        format!(
                            "_Channel “{}” is in **{}** responder mode, so this agent stayed silent._",
                            channel.name, mode
                        ),
                    );
                    let _ =
                        ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &note).await;
                    let _ = app.emit(
                        "agent-stream",
                        serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
                    );
                    return Ok(note);
                }
            }
        }
    }

    // Create and execute run
    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    // Execute the run with live streaming to the UI
    let result = execute_run_with_stream(&state, &app, &mut run).await;

    // A resumable pause is a successful, incomplete exit: keep the run's
    // checkpoint alive for `resume_run` and don't emit `done`.
    if result.is_ok() && run.state == RunState::Paused {
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({
                "kind": "paused",
                "thread_id": thread_id.to_string(),
                "run_id": run.id.to_string(),
                "bot_id": run.bot_id.to_string(),
            }),
        );
        let messages =
            ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
                .await
                .map_err(|e| e.to_string())?;
        return messages
            .last()
            .cloned()
            .ok_or_else(|| "Run paused".to_string());
    }

    if let Err(err) = result {
        let err_str = err.to_string();
        tracing::warn!("Run execution error: {}", err_str);

        let error_msg = Message::assistant(
            thread_id,
            format!(
                "⚠️ **Model Error:** {}\n\n{}{}",
                err_str,
                error_hint(&err_str),
                "\n\nYou can also run me fully offline via a local Ollama model."
            ),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
        );
        return Ok(error_msg);
    }

    let _ = app.emit(
        "agent-stream",
        serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
    );

    // Return the last message (assistant response)
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    messages
        .last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

/// Execute a run with streaming wired to the UI for the given thread.
async fn execute_run_with_stream(
    state: &State<'_, AppState>,
    app: &tauri::AppHandle,
    run: &mut Run,
) -> Result<(), String> {
    // Let the UI track the active run so its Pause/Resume control can target
    // this run specifically (resumable checkpoint), not the global kill switch.
    let _ = app.emit(
        "agent-stream",
        serde_json::json!({
            "kind": "run_started",
            "thread_id": run.thread_id.to_string(),
            "run_id": run.id.to_string(),
        }),
    );
    state
        .runtime
        .set_thread_emitter(run.thread_id, Some(make_stream_emitter(app.clone())));
    let result = state.runtime.execute_run(run).await;
    state.runtime.set_thread_emitter(run.thread_id, None);
    result.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub key_placeholder: String,
    pub key_url: String,
    pub key_env: String,
    pub supports_discovery: bool,
    pub keyless: bool,
    pub default_model: String,
    pub fallback_models: Vec<CatalogModel>,
}

/// Fetch complete model provider catalog
#[tauri::command]
async fn get_model_catalog() -> Result<Vec<CatalogProvider>, String> {
    let m = |id: &str, name: &str| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: None,
        supports_vision: None,
    };
    let mv = |id: &str, name: &str, vision: bool| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: None,
        supports_vision: Some(vision),
    };
    let mf = |id: &str, name: &str| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: Some(true),
        supports_vision: None,
    };

    Ok(vec![
        // 1. Command Code
        CatalogProvider {
            id: "commandcode".to_string(),
            name: "Command Code".to_string(),
            icon: "⚡".to_string(),
            description: "OpenAI & Anthropic proxy, high-rate limits, free models".to_string(),
            key_placeholder: "cc_... or rc_...".to_string(),
            key_url: "https://commandcode.ai".to_string(),
            key_env: "COMMANDCODE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-6".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-6", "Claude Sonnet 4 (CC)", true),
                m("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash (CC)"),
                mf("longcat-2.0:free", "LongCat 2.0 (Free)"),
            ],
        },
        // 2. OpenCode
        CatalogProvider {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            icon: "💻".to_string(),
            description: "OpenCode Zen coding agent & multi-model LLM gateway".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://opencode.ai/auth".to_string(),
            key_env: "OPENCODE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-5".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-5", "Claude Sonnet 4.5", true),
                mv("gpt-5", "GPT-5", true),
                m("deepseek-v4-pro", "DeepSeek V4 Pro"),
                m("gemini-3-flash", "Gemini 3 Flash"),
            ],
        },
        // 3. OpenCode Go
        //
        // The model list is NOT hardcoded: `fetch_provider_models` reads the
        // live id list and enriches it from models.dev. The one entry here is a
        // bootstrap so a model picker is never empty before discovery runs.
        CatalogProvider {
            id: "opencode-go".to_string(),
            name: "OpenCode Go".to_string(),
            icon: "🟢".to_string(),
            description: "Go subscription (models auto-synced from OpenCode; Claude routed via Anthropic)".to_string(),
            key_placeholder: "oc_sk_...".to_string(),
            key_url: "https://opencode.ai/console".to_string(),
            key_env: "OPENCODE_GO_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "deepseek-v4.1-flash".to_string(),
            fallback_models: vec![m("deepseek-v4.1-flash", "DeepSeek V4.1 Flash")],
        },
        // 3. Anthropic Claude
        CatalogProvider {
            id: "anthropic".to_string(),
            name: "Anthropic Claude".to_string(),
            icon: "🧠".to_string(),
            description: "Claude 3.7 Sonnet (Thinking), Claude 3.5 Sonnet, Claude 3 Opus".to_string(),
            key_placeholder: "sk-ant-api03-...".to_string(),
            key_url: "https://console.anthropic.com".to_string(),
            key_env: "ANTHROPIC_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-3-7-sonnet-20250219".to_string(),
            fallback_models: vec![
                mv("claude-3-7-sonnet-20250219", "Claude 3.7 Sonnet (Hybrid Thinking)", true),
                mv("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", true),
                mv("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", true),
                mv("claude-3-opus-20240229", "Claude 3 Opus", true),
            ],
        },
        // 4. Cline
        CatalogProvider {
            id: "cline".to_string(),
            name: "Cline".to_string(),
            icon: "🤖".to_string(),
            description: "Autonomous coding agent gateway & multi-model routing".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://cline.bot".to_string(),
            key_env: "CLINE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-5".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-5", "Claude Sonnet 4.5", true),
                mv("openai/gpt-4o", "GPT-4o", true),
                m("deepseek/deepseek-chat", "DeepSeek Chat"),
            ],
        },
        // 5. OpenRouter (9 router)
        CatalogProvider {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            icon: "🌐".to_string(),
            description: "300+ models, unified API, pay-as-you-go aggregator".to_string(),
            key_placeholder: "sk-or-v1-...".to_string(),
            key_url: "https://openrouter.ai".to_string(),
            key_env: "OPENROUTER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "anthropic/claude-3.5-sonnet".to_string(),
            fallback_models: vec![
                mv("anthropic/claude-3.5-sonnet", "Claude 3.5 Sonnet", true),
                mv("openai/gpt-4o", "GPT-4o", true),
                m("deepseek/deepseek-chat", "DeepSeek V3"),
                m("deepseek/deepseek-r1", "DeepSeek R1"),
                m("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B"),
            ],
        },
        // 6. OpenAI
        CatalogProvider {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            icon: "🟢".to_string(),
            description: "GPT-4o, GPT-4o mini, o1 reasoning & o3-mini".to_string(),
            key_placeholder: "sk-proj-...".to_string(),
            key_url: "https://platform.openai.com".to_string(),
            key_env: "OPENAI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "gpt-4o".to_string(),
            fallback_models: vec![
                mv("gpt-4o", "GPT-4o", true),
                mv("gpt-4o-mini", "GPT-4o Mini", true),
                m("o1", "o1 Reasoning"),
                m("o3-mini", "o3-mini"),
                mv("gpt-4-turbo", "GPT-4 Turbo", true),
            ],
        },
        // 7. Google Gemini
        CatalogProvider {
            id: "gemini".to_string(),
            name: "Google Gemini".to_string(),
            icon: "✨".to_string(),
            description: "Gemini 2.0 Flash, Gemini 2.5 Pro with 1M-2M context".to_string(),
            key_placeholder: "AIzaSy...".to_string(),
            key_url: "https://aistudio.google.com".to_string(),
            key_env: "GEMINI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "gemini-2.0-flash".to_string(),
            fallback_models: vec![
                mv("gemini-2.0-flash", "Gemini 2.0 Flash", true),
                mv("gemini-2.5-pro", "Gemini 2.5 Pro", true),
                mv("gemini-1.5-pro", "Gemini 1.5 Pro", true),
                mv("gemini-1.5-flash", "Gemini 1.5 Flash", true),
            ],
        },
        // 8. DeepSeek
        CatalogProvider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            icon: "🐋".to_string(),
            description: "DeepSeek V3 671B MoE and DeepSeek R1 reasoning models".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://platform.deepseek.com".to_string(),
            key_env: "DEEPSEEK_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "deepseek-chat".to_string(),
            fallback_models: vec![
                m("deepseek-chat", "DeepSeek V3"),
                m("deepseek-reasoner", "DeepSeek R1 Reasoning"),
            ],
        },
        // 9. Groq
        CatalogProvider {
            id: "groq".to_string(),
            name: "Groq LPU".to_string(),
            icon: "🚀".to_string(),
            description: "Ultra-fast low-latency inference on custom LPUs".to_string(),
            key_placeholder: "gsk_...".to_string(),
            key_url: "https://console.groq.com".to_string(),
            key_env: "GROQ_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "llama-3.3-70b-versatile".to_string(),
            fallback_models: vec![
                m("llama-3.3-70b-versatile", "Llama 3.3 70B Versatile"),
                m("deepseek-r1-distill-llama-70b", "DeepSeek R1 Distill Llama 70B"),
                m("mixtral-8x7b-32768", "Mixtral 8x7B"),
            ],
        },
        // 10. xAI Grok
        CatalogProvider {
            id: "xai".to_string(),
            name: "xAI Grok".to_string(),
            icon: "🌌".to_string(),
            description: "Grok 2, Grok 2 Vision & Grok real-time frontier models".to_string(),
            key_placeholder: "xai-...".to_string(),
            key_url: "https://console.x.ai".to_string(),
            key_env: "XAI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "grok-2-latest".to_string(),
            fallback_models: vec![
                m("grok-2-latest", "Grok 2"),
                mv("grok-2-vision-1212", "Grok 2 Vision", true),
                m("grok-beta", "Grok Beta"),
            ],
        },
        // 11. Mistral AI
        CatalogProvider {
            id: "mistral".to_string(),
            name: "Mistral AI".to_string(),
            icon: "🌪️".to_string(),
            description: "Mistral Large, Mistral Small, Codestral & Pixtral".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://console.mistral.ai".to_string(),
            key_env: "MISTRAL_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "mistral-large-latest".to_string(),
            fallback_models: vec![
                m("mistral-large-latest", "Mistral Large"),
                m("mistral-small-latest", "Mistral Small"),
                m("codestral-latest", "Codestral"),
            ],
        },
        // 12. Together AI
        CatalogProvider {
            id: "together".to_string(),
            name: "Together AI".to_string(),
            icon: "🤝".to_string(),
            description: "Fast inference engine for leading open source LLMs".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://api.together.xyz".to_string(),
            key_env: "TOGETHER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
            fallback_models: vec![
                m("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo"),
                m("deepseek-ai/DeepSeek-R1", "DeepSeek R1 (Together)"),
                m("Qwen/Qwen2.5-72B-Instruct-Turbo", "Qwen 2.5 72B Turbo"),
            ],
        },
        // 13. Perplexity AI
        CatalogProvider {
            id: "perplexity".to_string(),
            name: "Perplexity AI".to_string(),
            icon: "🔍".to_string(),
            description: "Sonar search models with live internet citations & grounding".to_string(),
            key_placeholder: "pplx-...".to_string(),
            key_url: "https://perplexity.ai".to_string(),
            key_env: "PERPLEXITY_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "llama-3.1-sonar-large-128k-online".to_string(),
            fallback_models: vec![
                m("llama-3.1-sonar-large-128k-online", "Sonar Large 128K"),
                m("llama-3.1-sonar-small-128k-online", "Sonar Small 128K"),
                m("sonar-reasoning", "Sonar Reasoning"),
            ],
        },
        // 14. Cohere
        CatalogProvider {
            id: "cohere".to_string(),
            name: "Cohere".to_string(),
            icon: "🌲".to_string(),
            description: "Command R+ & Command R enterprise multilingual & tool models".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://cohere.com".to_string(),
            key_env: "COHERE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "command-r-plus".to_string(),
            fallback_models: vec![
                m("command-r-plus", "Command R+"),
                m("command-r", "Command R"),
            ],
        },
        // 15. Xiaomi MiMo
        CatalogProvider {
            id: "mimo".to_string(),
            name: "Xiaomi MiMo".to_string(),
            icon: "📱".to_string(),
            description: "High-efficiency multilingual reasoning models & edge AI".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://api.mimo.mi.com".to_string(),
            key_env: "MIMO_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "mimo-v2.5".to_string(),
            fallback_models: vec![
                m("mimo-v2.5", "MiMo V2.5"),
                m("mimo-v2.5-pro", "MiMo V2.5 Pro"),
            ],
        },
        // 16. Ollama
        CatalogProvider {
            id: "ollama".to_string(),
            name: "Ollama (Local)".to_string(),
            icon: "🟢".to_string(),
            description: "100% sovereign offline inference".to_string(),
            key_placeholder: "http://localhost:11434".to_string(),
            key_url: "https://ollama.com".to_string(),
            key_env: "OLLAMA_HOST".to_string(),
            supports_discovery: true,
            keyless: true,
            default_model: "llama3.1".to_string(),
            fallback_models: vec![
                m("llama3.1", "Llama 3.1"),
                m("llama3.1:8b", "Llama 3.1 8B"),
                m("mistral", "Mistral 7B"),
                m("deepseek-r1:8b", "DeepSeek R1 8B"),
            ],
        },
        // 17. TokenRouter
        CatalogProvider {
            id: "tokenrouter".to_string(),
            name: "TokenRouter".to_string(),
            icon: "🧭".to_string(),
            description: "Unified multi-model hub — OpenAI/Claude/Gemini-compatible with global routing & failover".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://www.tokenrouter.com/console/token".to_string(),
            key_env: "TOKENROUTER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "deepseek/deepseek-v4.1-flash".to_string(),
            fallback_models: vec![
                m("deepseek/deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
                m("z-ai/glm-5.3", "GLM 5.3"),
                m("qwen/qwen3.8-max", "Qwen 3.8 Max"),
                m("moonshotai/kimi-k2.7-code", "Kimi K2.7 Code"),
                mv("openai/gpt-5-mini", "GPT-5 Mini", true),
                m("x-ai/grok-4.6", "Grok 4.6"),
                mf("z-ai/glm-5.3-free", "GLM 5.3 (Free)"),
            ],
        },
    ])
}

/// Fetch live models for a provider using ModelDiscovery
#[tauri::command]
async fn fetch_provider_models(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<serde_json::Value>, String> {
    let (key, custom) = {
        let manager = state.runtime.provider_manager().lock().await;
        (
            manager.get_api_key(&provider).map(|s| s.to_string()),
            manager
                .custom_spec(&provider)
                .map(|s| (s.base_url.clone(), s.kind.clone())),
        )
    };
    let models = match custom {
        Some((base_url, kind)) => {
            state
                .model_discovery
                .fetch_models_for(&provider, key.as_deref(), &base_url, &kind)
                .await
        }
        None => {
            state
                .model_discovery
                .fetch_models(&provider, key.as_deref(), true)
                .await
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(&models)
        .map_err(|e| e.to_string())?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

/// Fetch all provider models in parallel
#[tauri::command]
async fn fetch_all_provider_models(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, Vec<ravenbot_models::DiscoveredModel>>, String> {
    let api_keys = {
        let manager = state.runtime.provider_manager().lock().await;
        manager.get_api_keys().clone()
    };
    let mut models = state.model_discovery.fetch_all_providers(&api_keys).await;

    // User-defined providers get discovered through their saved base URL/kind
    let customs = {
        let manager = state.runtime.provider_manager().lock().await;
        manager
            .custom_provider_ids()
            .into_iter()
            .filter_map(|id| {
                let spec = manager.custom_spec(&id)?;
                Some((id, spec.base_url.clone(), spec.kind.clone()))
            })
            .collect::<Vec<_>>()
    };
    for (id, base_url, kind) in customs {
        let key = api_keys.get(&id).cloned();
        if let Ok(list) = state
            .model_discovery
            .fetch_models_for(&id, key.as_deref(), &base_url, &kind)
            .await
        {
            if !list.is_empty() {
                models.insert(id, list);
            }
        }
    }
    Ok(models)
}

/// Set and persist an API key for a provider
#[tauri::command]
async fn set_provider_api_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    let p = provider.trim().to_lowercase();
    let trimmed_key = api_key.trim();

    // Save to database
    ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), &p, trimmed_key)
        .await
        .map_err(|e| e.to_string())?;

    // Also persist alias if claude/anthropic or grok/xai
    if p == "claude" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(
            state.db.pool(),
            "anthropic",
            trimmed_key,
        )
        .await;
    } else if p == "anthropic" {
        let _ =
            ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "claude", trimmed_key)
                .await;
    } else if p == "grok" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "xai", trimmed_key)
            .await;
    } else if p == "xai" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "grok", trimmed_key)
            .await;
    }

    // Update in runtime memory
    let mut manager = state.runtime.provider_manager().lock().await;
    if trimmed_key.is_empty() {
        manager.remove_api_key(&p);
        if p == "claude" {
            manager.remove_api_key("anthropic");
        }
        if p == "anthropic" {
            manager.remove_api_key("claude");
        }
        if p == "grok" {
            manager.remove_api_key("xai");
        }
        if p == "xai" {
            manager.remove_api_key("grok");
        }
    } else {
        manager.set_api_key(&p, trimmed_key.to_string());
        if p == "claude" {
            manager.set_api_key("anthropic", trimmed_key.to_string());
        }
        if p == "anthropic" {
            manager.set_api_key("claude", trimmed_key.to_string());
        }
        if p == "grok" {
            manager.set_api_key("xai", trimmed_key.to_string());
        }
        if p == "xai" {
            manager.set_api_key("grok", trimmed_key.to_string());
        }
    }

    tracing::info!("Provider API key updated for: {}", p);
    Ok(())
}

/// Backwards compatibility setter
#[tauri::command]
async fn set_api_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    set_provider_api_key(state, provider, api_key).await
}

/// Backwards compatibility check
#[tauri::command]
async fn check_api_key(state: State<'_, AppState>, provider: String) -> Result<bool, String> {
    let manager = state.runtime.provider_manager().lock().await;
    Ok(manager.has_key(&provider))
}

/// Get list of configured provider IDs
#[tauri::command]
async fn get_configured_providers(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db_keys = ravenbot_db::queries::ProviderKeyQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut configured: std::collections::HashSet<String> = db_keys.into_keys().collect();

    let mem_keys = {
        let manager = state.runtime.provider_manager().lock().await;
        manager.configured_providers()
    };
    for k in mem_keys {
        configured.insert(k);
    }

    // Ensure aliases stay in sync
    if configured.contains("anthropic") {
        configured.insert("claude".to_string());
    }
    if configured.contains("claude") {
        configured.insert("anthropic".to_string());
    }
    if configured.contains("xai") {
        configured.insert("grok".to_string());
    }
    if configured.contains("grok") {
        configured.insert("xai".to_string());
    }

    let mut result: Vec<String> = configured.into_iter().collect();
    result.sort();
    Ok(result)
}

/// Get Ollama base URL
#[tauri::command]
async fn get_ollama_url(state: State<'_, AppState>) -> Result<String, String> {
    if let Ok(Some(url)) =
        ravenbot_db::queries::AppSettingsQueries::get(state.db.pool(), "ollama_url").await
    {
        if !url.trim().is_empty() {
            return Ok(url);
        }
    }
    let manager = state.runtime.provider_manager().lock().await;
    if let Some(u) = manager.get_base_url("ollama") {
        return Ok(u.to_string());
    }
    if let Ok(u) = std::env::var("OLLAMA_HOST") {
        if !u.trim().is_empty() {
            return Ok(u.trim().to_string());
        }
    }
    if let Ok(u) = std::env::var("OLLAMA_URL") {
        if !u.trim().is_empty() {
            return Ok(u.trim().to_string());
        }
    }
    Ok("http://localhost:11434".to_string())
}

/// Set and persist Ollama base URL
#[tauri::command]
async fn set_ollama_url(state: State<'_, AppState>, url: String) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/').to_string();
    ravenbot_db::queries::AppSettingsQueries::set(state.db.pool(), "ollama_url", &u)
        .await
        .map_err(|e| e.to_string())?;
    let mut manager = state.runtime.provider_manager().lock().await;
    manager.set_base_url("ollama", u.clone());
    Ok(u)
}

// ── Custom providers (P8) ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CustomProviderInput {
    pub id: String,
    pub display_name: String,
    /// "openai" | "anthropic" | "ollama"
    pub kind: String,
    pub base_url: String,
    /// Write-only. None = leave the stored key untouched; Some("") = clear it.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_true")]
    pub supports_tools: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional JSON object of extra HTTP headers (e.g. {"x-opencode-session":"..."}).
    #[serde(default)]
    pub headers_json: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, serde::Serialize)]
pub struct CustomProviderInfo {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub base_url: String,
    pub default_model: String,
    pub supports_tools: bool,
    pub enabled: bool,
    pub has_key: bool,
    #[serde(default)]
    pub headers_json: Option<String>,
}

/// Custom ids must be lowercase slugs and must never shadow a built-in.
fn validate_custom_provider_id(id: &str) -> Result<(), String> {
    let chars: Vec<char> = id.chars().collect();
    if chars.is_empty() || chars.len() > 32 {
        return Err("Provider id must be 1-32 characters".to_string());
    }
    if !chars[0].is_ascii_lowercase() {
        return Err("Provider id must start with a lowercase letter".to_string());
    }
    for c in &chars[1..] {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-') {
            return Err("Provider id may only contain a-z, 0-9, '-' and '_'".to_string());
        }
    }
    if ravenbot_models::manager::is_reserved_provider_name(id) {
        return Err(format!(
            "'{id}' is a built-in provider name and cannot be used as a custom id"
        ));
    }
    Ok(())
}

fn validate_http_base_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/').to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    Ok(trimmed)
}

/// List user-defined providers (never includes API keys)
#[tauri::command]
async fn list_custom_providers(
    state: State<'_, AppState>,
) -> Result<Vec<CustomProviderInfo>, String> {
    let rows = ravenbot_db::queries::CustomProviderQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let manager = state.runtime.provider_manager().lock().await;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                display_name,
                kind,
                base_url,
                default_model,
                supports_tools,
                enabled,
                headers_json,
            )| {
                CustomProviderInfo {
                    has_key: manager.has_key(&id),
                    id,
                    display_name,
                    kind,
                    base_url,
                    default_model,
                    supports_tools,
                    enabled,
                    headers_json,
                }
            },
        )
        .collect())
}

/// Create or update a user-defined provider and hydrate it into the manager
#[tauri::command]
async fn upsert_custom_provider(
    state: State<'_, AppState>,
    provider: CustomProviderInput,
) -> Result<(), String> {
    let id = provider.id.trim().to_lowercase();
    validate_custom_provider_id(&id)?;
    let kind = provider.kind.trim().to_lowercase();
    if !matches!(kind.as_str(), "openai" | "anthropic" | "ollama") {
        return Err(format!("Unknown provider kind: {kind}"));
    }
    let base_url = validate_http_base_url(&provider.base_url)?;
    let display_name = provider.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("Display name is required".to_string());
    }
    let default_model = provider.default_model.trim().to_string();

    ravenbot_db::queries::CustomProviderQueries::upsert(
        state.db.pool(),
        &id,
        &display_name,
        &kind,
        &base_url,
        &default_model,
        provider.supports_tools,
        provider.enabled,
        provider.headers_json.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(key) = provider.api_key.as_deref() {
        ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), &id, key)
            .await
            .map_err(|e| e.to_string())?;
        let mut manager = state.runtime.provider_manager().lock().await;
        let trimmed = key.trim();
        if trimmed.is_empty() {
            manager.remove_api_key(&id);
        } else {
            manager.set_api_key(&id, trimmed.to_string());
        }
    }

    {
        let mut manager = state.runtime.provider_manager().lock().await;
        if provider.enabled {
            let headers = provider
                .headers_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            manager.register_custom(ravenbot_models::manager::CustomProviderSpec {
                id: id.clone(),
                display_name,
                kind,
                base_url,
                default_model,
                supports_tools: provider.supports_tools,
                headers,
            });
        } else {
            manager.remove_custom(&id);
        }
    }
    state.model_discovery.clear_cache(Some(&id)).await;
    tracing::info!("Custom provider upserted: {}", id);
    Ok(())
}

/// Delete a user-defined provider. Returns the number of bots that were using
/// it; refuses unless `force` when bots still reference it.
#[tauri::command]
async fn delete_custom_provider(
    state: State<'_, AppState>,
    id: String,
    force: Option<bool>,
) -> Result<usize, String> {
    let id = id.trim().to_lowercase();
    let in_use = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|b| b.config.model_provider == id)
        .count();
    if in_use > 0 && !force.unwrap_or(false) {
        return Err(format!(
            "Provider is used by {in_use} bot(s). Confirm to delete anyway."
        ));
    }
    ravenbot_db::queries::CustomProviderQueries::delete(state.db.pool(), &id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = ravenbot_db::queries::ProviderKeyQueries::delete(state.db.pool(), &id).await;
    {
        let mut manager = state.runtime.provider_manager().lock().await;
        manager.remove_custom(&id);
        manager.remove_api_key(&id);
    }
    state.model_discovery.clear_cache(Some(&id)).await;
    Ok(in_use)
}

/// One-token completion against the saved definition + key
#[tauri::command]
async fn test_custom_provider(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let id = id.trim().to_lowercase();
    let provider = {
        let manager = state.runtime.provider_manager().lock().await;
        manager
            .create_provider_from_str(&id)
            .map_err(|e| e.to_string())?
    };
    let messages = vec![ravenbot_models::Message::text(
        "user",
        "Reply with the single word: ok",
    )];
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        provider.complete(&messages, &[], 0.0, 16),
    )
    .await
    .map_err(|_| "Request timed out after 30s".to_string())?
    .map_err(|e| e.to_string())?;
    Ok(response.content.unwrap_or_default())
}

/// Set (or clear, with an empty string) a built-in provider's base URL override
#[tauri::command]
async fn set_provider_base_url(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
) -> Result<(), String> {
    let p = provider.trim().to_lowercase();
    if p.is_empty() {
        return Err("Provider is required".to_string());
    }
    let trimmed = if base_url.trim().is_empty() {
        String::new()
    } else {
        validate_http_base_url(&base_url)?
    };
    ravenbot_db::queries::ProviderBaseUrlQueries::set(state.db.pool(), &p, &trimmed)
        .await
        .map_err(|e| e.to_string())?;
    {
        let mut manager = state.runtime.provider_manager().lock().await;
        manager.set_base_url(&p, trimmed);
    }
    state.model_discovery.clear_cache(Some(&p)).await;
    Ok(())
}

/// Current base URL override for a provider ("" = compiled-in default)
#[tauri::command]
async fn get_provider_base_url(
    state: State<'_, AppState>,
    provider: String,
) -> Result<String, String> {
    let p = provider.trim().to_lowercase();
    if let Some(u) = ravenbot_db::queries::ProviderBaseUrlQueries::get(state.db.pool(), &p)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(u);
    }
    let manager = state.runtime.provider_manager().lock().await;
    Ok(manager.get_base_url(&p).unwrap_or_default().to_string())
}

/// Probe Ollama endpoint for installed models
#[tauri::command]
async fn probe_ollama(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = get_ollama_url(state.clone()).await?;
    let models = state
        .model_discovery
        .fetch_models("ollama", None, true)
        .await
        .map_err(|e| format!("Failed to connect to Ollama at {}: {}", url, e))?;

    Ok(serde_json::json!({
        "base_url": url,
        "count": models.len(),
        "models": models
    }))
}

/// Get global default model
#[tauri::command]
async fn get_default_model(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (provider, model, source) = resolve_default_model_with_source(state.db.pool()).await;
    Ok(serde_json::json!({ "provider": provider, "model": model, "source": source }))
}

/// Set global default model
#[tauri::command]
async fn set_default_model(
    state: State<'_, AppState>,
    provider: String,
    model: String,
) -> Result<(), String> {
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        "default_provider",
        provider.trim(),
    )
    .await
    .map_err(|e| e.to_string())?;
    ravenbot_db::queries::AppSettingsQueries::set(state.db.pool(), "default_model", model.trim())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Set approval mode for a bot
#[tauri::command]
async fn set_approval_mode(
    state: State<'_, AppState>,
    bot_id: Uuid,
    mode: String,
) -> Result<String, String> {
    let mode_val = mode.trim().to_lowercase();
    sqlx::query("UPDATE bots SET approval_mode = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&mode_val)
        .bind(bot_id.to_string())
        .execute(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(mode_val)
}

/// Get approval mode for a bot
#[tauri::command]
async fn get_approval_mode(state: State<'_, AppState>, bot_id: Uuid) -> Result<String, String> {
    let row = sqlx::query_as::<_, (Option<String>,)>("SELECT approval_mode FROM bots WHERE id = ?")
        .bind(bot_id.to_string())
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;

    Ok(row.and_then(|r| r.0).unwrap_or_else(|| "ask".to_string()))
}

/// Pending approval requests for a thread (inline Allow/Deny cards).
#[tauri::command]
async fn list_pending_approvals(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<ApprovalRequest>, String> {
    ravenbot_db::queries::ApprovalQueries::list_pending_for_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// Pending approvals anywhere under one bot.
///
/// A run does not stay on the thread the user has open: a delegated child runs
/// on its own thread and parks its approvals there. Scoping to the open thread
/// — which is all the caller could ask for — meant that after a reload nothing
/// could find those approvals, so the run waited for a decision no screen could
/// show or make. From outside it looked like the app had stopped working
/// mid-run, with no error and no spinner to explain it.
#[tauri::command]
async fn list_pending_approvals_for_bot(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<ApprovalRequest>, String> {
    ravenbot_db::queries::ApprovalQueries::list_pending_for_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Decide a parked approval (allow/deny). Returns true if it was still pending.
#[tauri::command]
async fn decide_approval(
    state: State<'_, AppState>,
    approval_id: Uuid,
    allowed: bool,
    note: Option<String>,
) -> Result<bool, String> {
    let decided = ravenbot_db::queries::ApprovalQueries::decide(
        state.db.pool(),
        approval_id,
        allowed,
        note.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(decided)
}

/// A bot's live todo list (the runtime-native `todo` tool). Surfaced on the
/// office board so each teammate's self-tracked checklist is visible.
#[tauri::command]
async fn list_bot_todos(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<serde_json::Value>, String> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id, task, done FROM bot_todos WHERE bot_id = ? ORDER BY created_at ASC LIMIT 60",
    )
    .bind(bot_id.to_string())
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, task, done)| serde_json::json!({ "id": id, "task": task, "done": done != 0 }))
        .collect())
}

/// Persist the office board snapshot for a room thread so the kanban/DAG
/// survives room re-opens and app restarts. Stored as one JSON value in
/// `app_settings` keyed by thread (rooms are few; upsert keeps one row each).
#[tauri::command]
async fn save_office_board(
    state: State<'_, AppState>,
    thread_id: Uuid,
    goal: String,
    nodes: serde_json::Value,
) -> Result<(), String> {
    if !nodes.is_array() {
        return Err("nodes must be an array".to_string());
    }
    let payload = serde_json::json!({ "goal": goal, "nodes": nodes });
    let encoded = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    if encoded.len() > 64 * 1024 {
        return Err("board snapshot too large".to_string());
    }
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        &format!("office_board:{}", thread_id),
        &encoded,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Last persisted office board for a room thread, if any. The payload gains
/// `liveNodeThreads`: node threads whose latest run is still non-terminal,
/// so the UI can tell a genuinely-working card from one orphaned by a restart.
#[tauri::command]
async fn get_office_board(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    let raw = ravenbot_db::queries::AppSettingsQueries::get(
        state.db.pool(),
        &format!("office_board:{}", thread_id),
    )
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(None) };
    let mut payload: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let mut node_threads: Vec<String> = Vec::new();
    if let Some(nodes) = payload.get("nodes").and_then(|v| v.as_array()) {
        for n in nodes {
            if let Some(s) = n.get("nodeThreadId").and_then(|v| v.as_str()) {
                if Uuid::parse_str(s).is_ok() {
                    node_threads.push(s.to_string());
                }
            }
        }
    }
    let live: Vec<String> = if node_threads.is_empty() {
        Vec::new()
    } else {
        let sql = format!(
            "SELECT DISTINCT thread_id FROM runs WHERE thread_id IN ({}) \
             AND state NOT IN ('completed','failed','cancelled')",
            vec!["?"; node_threads.len()].join(",")
        );
        let mut q = sqlx::query_as::<_, (String,)>(&sql);
        for id in &node_threads {
            q = q.bind(id);
        }
        q.fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(t,)| t)
            .collect()
    };
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("liveNodeThreads".to_string(), serde_json::json!(live));
    }
    Ok(Some(payload))
}

/// Pending `ask_user` questions for a thread (inline answer cards).
#[tauri::command]
async fn list_pending_questions(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<QuestionRequest>, String> {
    ravenbot_db::queries::QuestionQueries::list_pending_for_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// Pending questions anywhere under one bot — the same argument as
/// `list_pending_approvals_for_bot`: a parked `ask_user` on a child thread is
/// invisible to a thread-scoped lookup, and an invisible question never gets
/// answered, so the run never resumes.
#[tauri::command]
async fn list_pending_questions_for_bot(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<QuestionRequest>, String> {
    ravenbot_db::queries::QuestionQueries::list_pending_for_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Answer a parked question. Returns true if it was still pending.
#[tauri::command]
async fn answer_question(
    state: State<'_, AppState>,
    question_id: Uuid,
    answer: String,
) -> Result<bool, String> {
    ravenbot_db::queries::QuestionQueries::answer(state.db.pool(), question_id, &answer)
        .await
        .map_err(|e| e.to_string())
}

/// Cooperatively cancel a running run (stops at the next step boundary).
#[tauri::command]
async fn cancel_run(state: State<'_, AppState>, run_id: Uuid) -> Result<(), String> {
    state.runtime.request_cancel(run_id);
    Ok(())
}

/// Request a resumable pause at the run's next tool-round boundary.
#[tauri::command]
async fn pause_run(state: State<'_, AppState>, run_id: Uuid) -> Result<(), String> {
    state.runtime.request_pause(run_id);
    Ok(())
}

/// Resume a paused run from its checkpoint, streaming as it re-enters the loop.
#[tauri::command]
async fn resume_run(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    run_id: Uuid,
) -> Result<Message, String> {
    let mut run = ravenbot_db::queries::RunQueries::get(state.db.pool(), run_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Run not found".to_string())?;
    if run.state != RunState::Paused {
        return Err("Run is not paused".to_string());
    }
    execute_run_with_stream(&state, &app, &mut run).await?;
    let _ = app.emit(
        "agent-stream",
        serde_json::json!({ "kind": "done", "thread_id": run.thread_id.to_string() }),
    );
    let messages =
        ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), run.thread_id)
            .await
            .map_err(|e| e.to_string())?;
    messages
        .last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

/// A captured screen frame for the desktop panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    /// Desktop coordinates of the capture's top-left pixel.
    pub origin_x: i32,
    pub origin_y: i32,
    pub data_url: String,
    pub timestamp: String,
}

/// Capture the current screen (for the desktop panel / live preview).
#[tauri::command]
async fn capture_screen() -> Result<ScreenCapture, String> {
    let capture = ravenbot_vision::ScreenshotCapture::new();
    let shot = capture.capture().await.map_err(|e| e.to_string())?;
    Ok(ScreenCapture {
        width: shot.width,
        height: shot.height,
        origin_x: shot.origin_x,
        origin_y: shot.origin_y,
        data_url: shot.to_data_url(),
        timestamp: shot.timestamp.to_rfc3339(),
    })
}

/// Capture one window by its xcap id, for the Computer panel's window list.
#[tauri::command]
async fn capture_desktop_window(window_id: u64) -> Result<ScreenCapture, String> {
    let capture = ravenbot_vision::ScreenshotCapture::new();
    let shot = capture
        .capture_window(window_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ScreenCapture {
        width: shot.width,
        height: shot.height,
        origin_x: shot.origin_x,
        origin_y: shot.origin_y,
        data_url: shot.to_data_url(),
        timestamp: shot.timestamp.to_rfc3339(),
    })
}

/// Whether real desktop input injection is available, and via which tools.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InputBackendInfo {
    pub available: bool,
    pub summary: String,
}

#[tauri::command]
async fn get_input_backend() -> Result<InputBackendInfo, String> {
    let injector = ravenbot_vision::InputInjector::detect();
    Ok(InputBackendInfo {
        available: injector.is_available(),
        summary: injector.backend_summary(),
    })
}

/// Transcribe recorded audio (base64) to text using the real STT engine
/// (OpenAI Whisper API, or local faster-whisper). Honest errors when neither
/// is configured — this is what powers microphone input on Linux + macOS,
/// where the browser Web Speech API is not available.
#[tauri::command]
async fn transcribe_audio(
    audio_base64: String,
    format: Option<String>,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64.trim())
        .map_err(|e| format!("Invalid base64 audio: {}", e))?;
    let fmt = format
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .unwrap_or("webm");
    let result = ravenbot_vision::AudioTranscriber::new()
        .transcribe(&bytes, fmt)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "text": result.text,
        "language": result.language,
        "confidence": result.confidence,
        "duration_secs": result.duration_secs,
    }))
}

/// Synthesize text to speech and return base64 WAV for the UI to play. Uses
/// the bot's configured TTS engine (HF Inference API or the local model).
#[tauri::command]
async fn synthesize_speech(
    state: State<'_, AppState>,
    text: String,
    voice: Option<String>,
    rate: Option<f32>,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    if text.trim().is_empty() {
        return Err("Nothing to speak".to_string());
    }
    let options = ravenbot_vision::audio::tts::VoiceOptions {
        voice: voice
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("default")
            .to_string(),
        rate: rate.filter(|r| *r > 0.0).unwrap_or(1.0),
        ..Default::default()
    };
    // Prefer the owner's saved OpenAI key (multi-voice engine) over the env.
    let openai_key = ravenbot_db::queries::ProviderKeyQueries::get(state.db.pool(), "openai")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .filter(|k| !k.trim().is_empty());
    let audio = ravenbot_vision::TextToSpeech::new()
        .with_voice(options)
        .with_openai_key(openai_key)
        .synthesize(&text)
        .await
        .map_err(|e| e.to_string())?;
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio);
    Ok(serde_json::json!({
        "audio_b64": audio_b64,
        "format": "wav",
    }))
}

/// List the text-to-speech voices available with the current configuration.
/// Only real engine voices are returned — `multi_voice` is false when the
/// active engine has a single voice.
#[tauri::command]
async fn list_tts_voices(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let has_openai = ravenbot_db::queries::ProviderKeyQueries::get(state.db.pool(), "openai")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);
    let has_hf = ["HUGGINGFACE_INFERENCE_TOKEN", "HF_TOKEN"].iter().any(|k| {
        std::env::var(k)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    });

    let mut voices = vec![serde_json::json!({
        "id": "default",
        "name": "Engine default",
        "provider": "auto"
    })];
    if has_openai {
        for v in ravenbot_vision::audio::tts::OPENAI_VOICES {
            let mut name = v.to_string();
            if let Some(first) = name.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            voices.push(serde_json::json!({ "id": v, "name": name, "provider": "openai" }));
        }
    }

    Ok(serde_json::json!({
        "engine": if has_openai { "openai" } else if has_hf { "huggingface" } else { "local" },
        "multi_voice": has_openai,
        "voices": voices,
    }))
}

/// Report what this machine can do for the Computer panel: screen preview,
/// host control (with the platform safety policy), and Docker desktops.
#[tauri::command]
async fn computer_capabilities() -> Result<serde_json::Value, String> {
    let injector = ravenbot_vision::InputInjector::detect();
    let session = if cfg!(target_os = "linux") {
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_string())
    } else if cfg!(target_os = "macos") {
        "aqua".to_string()
    } else {
        std::env::consts::OS.to_string()
    };
    let wayland = cfg!(target_os = "linux")
        && (session.eq_ignore_ascii_case("wayland")
            || std::env::var("WAYLAND_DISPLAY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false));
    let allow_wayland = ravenbot_core::wayland_control_allowed()
        || std::env::var("RAVENBOT_ALLOW_WAYLAND_CONTROL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
    let host_control_policy = if wayland {
        if allow_wayland {
            "opt_in_required"
        } else {
            "blocked"
        }
    } else {
        "opt_in_required"
    };

    let docker_version = ravenbot_sandbox::desktop::docker_version().await;
    let image = ravenbot_sandbox::desktop::desktop_image();
    let image_present = if docker_version.is_some() {
        ravenbot_sandbox::desktop::image_present(&image).await
    } else {
        false
    };

    Ok(serde_json::json!({
        "os": std::env::consts::OS,
        "session": session,
        "preview": true,
        "host_control": injector.is_available(),
        "host_control_backend": injector.backend_summary(),
        "host_control_policy": host_control_policy,
        "docker": {
            "available": docker_version.is_some(),
            "version": docker_version,
            "image": image,
            "image_present": image_present,
        },
    }))
}

/// Status of a bot's isolated Docker desktop (None when never created).
#[tauri::command]
async fn bot_desktop_status(bot_id: Uuid) -> Result<serde_json::Value, String> {
    let session = ravenbot_sandbox::desktop::status(&bot_id).await?;
    Ok(serde_json::to_value(session).unwrap_or(serde_json::Value::Null))
}

/// Start (or reuse) a bot's isolated Docker desktop.
#[tauri::command]
async fn start_bot_desktop(bot_id: Uuid) -> Result<serde_json::Value, String> {
    let session = ravenbot_sandbox::desktop::start(&bot_id, 2.0, 2048).await?;
    Ok(serde_json::to_value(session).unwrap_or(serde_json::Value::Null))
}

/// Stop and remove a bot's isolated desktop (workspace volume is kept).
#[tauri::command]
async fn stop_bot_desktop(bot_id: Uuid) -> Result<(), String> {
    ravenbot_sandbox::desktop::stop(&bot_id).await
}

/// Report the effective command-isolation backend for a sandbox tier.
#[tauri::command]
async fn get_sandbox_report(
    tier: Option<String>,
) -> Result<ravenbot_sandbox::SandboxReport, String> {
    let tier = match tier
        .as_deref()
        .map(str::trim)
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("host") => SandboxTier::Host,
        Some("docker") => SandboxTier::Docker,
        _ => SandboxTier::OsLevel,
    };
    Ok(ravenbot_sandbox::SandboxRunner::from_tier(tier).report())
}

/// List what is inside a workspace, so a person can see the agents' work.
///
/// The confinement on this side is on the way *in*: `resolve_path` plus
/// `confined()`. Nothing in the app showed the result of it, so a user set a
/// folder, watched an agent run, and had no way to find out whether it produced
/// anything. This is a read of directories the user already pointed us at, and
/// it is bounded — depth, entry count, no symlink following, hidden files
/// opt-in — and says so when a bound bites rather than returning a short tree
/// that reads as complete.
#[tauri::command]
async fn browse_workspace(
    path: String,
    show_hidden: Option<bool>,
) -> Result<ravenbot_runtime::tree::Tree, String> {
    let root = ravenbot_core::expand_home(path.trim());
    if root.as_os_str().is_empty() {
        return Err("No folder given".to_string());
    }
    // Cheap and synchronous, but a big tree is real I/O, so it must not sit on
    // the async runtime's core threads.
    tauri::async_runtime::spawn_blocking(move || {
        ravenbot_runtime::tree::list(&root, show_hidden.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())
}

/// One project: a folder set an agent or an office works inside.
///
/// This is what makes the codebase reachable from the app rather than only from
/// a settings screen. Until now a workspace was a string you typed into bot
/// settings and a browser you had to remember to open; there was no list of
/// them anywhere, which is why "expand the codebase" was not a thing you could
/// do.
#[derive(serde::Serialize)]
struct Project {
    id: String,
    name: String,
    /// `office` or `agent` — decides the icon and whether the tree is offered
    /// as one workspace or many.
    kind: String,
    /// The folder paths, in the order they were configured.
    paths: Vec<String>,
    /// How many files changed by this project's agents, if known. Populated by
    /// the caller from `file_change_totals`, not here: this function is about
    /// what exists on disk, and mixing in what happened would make it
    /// uncacheable for no benefit.
    #[serde(skip)]
    _totals: (),
}

/// Every configured workspace, offices first.
///
/// Offices lead because an office's folder is a *shared* room the whole roster
/// writes into, while an agent's folder is its own desk — and the shared room is
/// the one a user opens to see what the team did.
#[tauri::command]
async fn list_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    let mut projects: Vec<Project> = Vec::new();

    let rooms = ravenbot_db::queries::ChatRoomQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    for room in rooms {
        let paths: Vec<String> = room
            .project_folders
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if paths.is_empty() {
            continue;
        }
        projects.push(Project {
            id: room.id.to_string(),
            name: room.name.clone(),
            kind: "office".to_string(),
            paths,
            _totals: (),
        });
    }

    let bots = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    for bot in bots {
        let mut paths: Vec<String> = Vec::new();
        if let Some(folder) = bot.config.working_folder.as_ref() {
            let folder = folder.trim();
            if !folder.is_empty() {
                paths.push(folder.to_string());
            }
        }
        if paths.is_empty() {
            // An agent with no folder still has its default workspace — it is
            // where a write lands when nothing was configured, so hiding it
            // would make the most common first-run state look like a
            // project-less app.
            let default = default_workspace_for_impl(&bot.name);
            if !default.is_empty() {
                paths.push(default);
            }
        }
        if paths.is_empty() {
            continue;
        }
        projects.push(Project {
            id: bot.id.to_string(),
            name: bot.name.clone(),
            kind: "agent".to_string(),
            paths,
            _totals: (),
        });
    }
    Ok(projects)
}

/// Recent writes across every agent, newest first.
///
/// One call for the whole board rather than one per agent: the view needs the
/// same rows for three things — the totals strip, the badges in the tree, and
/// the list for whichever agent is selected — and fetching them per agent would
/// be N round trips whose results have to be reconciled into one answer about
/// one file. The all-time totals still come from `file_change_totals`, because
/// a bounded list cannot sum to an all-time number and pretending otherwise
/// would show a total that shrinks as the list grows.
///
/// Deliberately does **not** carry `diff`. Each one may be a quarter of a
/// megabyte and this list is fetched whole for every render of the board; a
/// list of five hundred writes would put 128MB through IPC to populate badges
/// that are two digits wide. The lines are fetched one at a time, when someone
/// actually expands a row — see [`file_change_diff`].
#[tauri::command]
async fn list_recent_file_changes(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ravenbot_core::FileChange>, String> {
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            i64,
            i64,
            String,
        ),
    >(
        "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted, created_at
         FROM file_changes
         ORDER BY created_at DESC, rowid DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            use std::str::FromStr;
            Some(ravenbot_core::FileChange {
                id: uuid::Uuid::from_str(&r.0).ok()?,
                bot_id: uuid::Uuid::from_str(&r.1).ok()?,
                run_id: r.2.as_deref().and_then(|s| uuid::Uuid::from_str(s).ok()),
                thread_id: r.3.as_deref().and_then(|s| uuid::Uuid::from_str(s).ok()),
                path: r.4,
                skill: r.5,
                lines_added: r.6,
                lines_deleted: r.7,
                diff: None,
                created_at: r.8,
            })
        })
        .collect())
}

/// The lines behind one ledger row, for expanding it.
///
/// Separate from the list for the reason a viewer separates a preview from a
/// file: the list is always wanted in full and never read in full, the diff is
/// the opposite. Returns `None` rather than an empty string when there is
/// nothing to expand — a row whose counts could not be turned into lines
/// (binary content, a file past the diff cap, a row from before the column
/// existed) says so by not expanding, instead of opening onto a blank pane
/// that reads as a bug.
#[tauri::command]
async fn file_change_diff(state: State<'_, AppState>, id: Uuid) -> Result<Option<String>, String> {
    ravenbot_db::queries::FileChangeQueries::diff_for(state.db.pool(), id)
        .await
        .map_err(|e| e.to_string())
}

/// Per-agent change totals for the projects view: files, writes, +lines, -lines.
///
/// Returned keyed by bot id so the view can attach them to whichever project
/// list it already holds, instead of the two being fetched in an order the
/// caller has to care about.
#[tauri::command]
async fn file_change_totals(
    state: State<'_, AppState>,
) -> Result<Vec<(String, ravenbot_core::FileChangeTotals)>, String> {
    let rows = ravenbot_db::queries::FileChangeQueries::totals(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, t)| (id.to_string(), t))
        .collect())
}

/// Recent writes for one agent, newest first.
#[tauri::command]
async fn list_file_changes(
    state: State<'_, AppState>,
    bot_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<ravenbot_core::FileChange>, String> {
    ravenbot_db::queries::FileChangeQueries::recent_for_bot(
        state.db.pool(),
        bot_id,
        limit.unwrap_or(200).clamp(1, 1000),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Read one file from a workspace, for the viewer.
///
/// Separate from `browse_workspace` because of its preview cap: that call
/// refuses to inline anything over 256KB, which is right for a *list* (a
/// megabyte of text per row is a megabyte the user is not looking at) and wrong
/// for a viewer, where the whole point is the file. So the tree says which files
/// exist and this reads the one that was clicked.
///
/// The read is confined to the same configured roots the editor writes into.
/// The viewer is part of the project surface, not a general file browser: a
/// symlink in a workspace that points at `~/.ssh/id_rsa` is still outside it.
///
/// Capped, because a viewer that will happily load a 400MB log is a viewer that
/// freezes the window once.
#[tauri::command]
async fn read_workspace_file(
    state: State<'_, AppState>,
    path: String,
    max_bytes: Option<u64>,
) -> Result<serde_json::Value, String> {
    let target = ravenbot_core::expand_home(path.trim());
    // 4MB default: larger than the tree's preview cap by enough that "too big to
    // list" and "too big to open" are not the same file, small enough that
    // rendering it cannot stall the window.
    let cap = max_bytes
        .unwrap_or(4 * 1024 * 1024)
        .clamp(1024, 64 * 1024 * 1024);
    let roots = configured_workspace_roots(&state).await;
    tauri::async_runtime::spawn_blocking(move || read_workspace_file_impl(&target, cap, &roots))
        .await
        .map_err(|e| e.to_string())?
}

fn read_workspace_file_impl(
    requested: &std::path::Path,
    cap: u64,
    roots: &[std::path::PathBuf],
) -> Result<serde_json::Value, String> {
    // Resolve first, so a symlink inside the office cannot be used to read a
    // file outside it.
    let real = std::fs::canonicalize(requested).map_err(|e| format!("File not found: {e}"))?;
    if !roots.iter().any(|root| real.starts_with(root)) {
        return Err(
            "Refused: that file is outside every configured project workspace.".to_string(),
        );
    }

    let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return Err("That is a folder, not a file".to_string());
    }
    if !meta.is_file() {
        return Err("That is not a regular file".to_string());
    }
    let size = meta.len();
    let bytes = std::fs::read(&real).map_err(|e| e.to_string())?;
    // Hash the full file, before truncation: the editor uses this version
    // to refuse a save when an agent changed the file underneath it.
    let sha256 = file_version(&bytes);
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis());
    let truncated = bytes.len() as u64 > cap;
    let slice = if truncated {
        &bytes[..cap as usize]
    } else {
        &bytes[..]
    };
    // Cut on a char boundary: slicing a UTF-8 buffer at an arbitrary byte
    // would panic, and a panic here surfaces as a silent failed invoke.
    let mut text = match std::str::from_utf8(slice) {
        Ok(t) => t.to_string(),
        Err(e) => {
            let up_to = e.valid_up_to();
            String::from_utf8_lossy(&slice[..up_to]).into_owned()
        }
    };
    if truncated {
        text.push_str("\n\n… truncated …");
    }
    Ok(serde_json::json!({
        "text": text,
        "sizeBytes": size,
        "truncated": truncated,
        "sha256": sha256,
        "modifiedMs": modified_ms,
    }))
}

fn file_version(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every folder the Projects view can write into: office project folders and
/// each agent's working folder (or its default workspace).
async fn configured_workspace_roots(state: &AppState) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<String> = Vec::new();
    if let Ok(rooms) = ravenbot_db::queries::ChatRoomQueries::list(state.db.pool()).await {
        for room in rooms {
            roots.extend(room.project_folders);
        }
    }
    if let Ok(bots) = ravenbot_db::queries::BotQueries::list(state.db.pool()).await {
        for bot in bots {
            if let Some(folder) = bot
                .config
                .working_folder
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
            {
                roots.push(folder.to_string());
            } else {
                let default = default_workspace_for_impl(&bot.name);
                if !default.trim().is_empty() {
                    roots.push(default);
                }
            }
        }
    }
    let mut out: Vec<std::path::PathBuf> = roots
        .into_iter()
        .filter_map(|root| {
            let expanded = ravenbot_core::expand_home(root.trim());
            std::fs::canonicalize(expanded).ok()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Save one text file from the Projects editor.
///
/// This is the user's own editor, not an agent tool. It is still deliberately
/// bounded:
///  - the path must resolve inside a configured office or agent workspace
///  - an existing binary file cannot be overwritten as text
///  - the file cannot be larger than the editor's 4MB read cap
///  - `expected_sha256` makes a concurrent agent write a visible conflict
///    instead of silently losing whichever side saved last
///  - the write is atomic (same-directory temp file + rename), so a crash
///    cannot leave a half-written source file
/// Media types the in-app viewer can render. Everything else stays in the
/// "binary file" state with an open-in-file-manager action.
fn media_mime_for(path: &std::path::Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "svg" => Some("image/svg+xml"),
        "mp4" | "m4v" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "ogv" => Some("video/ogg"),
        "mov" => Some("video/quicktime"),
        "mkv" => Some("video/x-matroska"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "m4a" => Some("audio/mp4"),
        "flac" => Some("audio/flac"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

/// Read a workspace media file for in-app preview.
///
/// This is deliberately a separate command from `read_workspace_file`: the text
/// reader decodes bytes as UTF-8 and truncates, which is wrong for an MP4. The
/// same confinement rule applies — canonicalize first, then prove the real path
/// is inside a configured root, so a symlink cannot escape the office.
#[tauri::command]
async fn read_workspace_media(
    state: State<'_, AppState>,
    path: String,
) -> Result<serde_json::Value, String> {
    use base64::Engine;
    const MAX_MEDIA_BYTES: u64 = 32 * 1024 * 1024;
    let target = ravenbot_core::expand_home(path.trim());
    let roots = configured_workspace_roots(&state).await;
    tauri::async_runtime::spawn_blocking(move || {
        let real = std::fs::canonicalize(&target).map_err(|e| format!("File not found: {e}"))?;
        if !roots.iter().any(|root| real.starts_with(root)) {
            return Err(
                "Refused: that file is outside every configured project workspace.".to_string(),
            );
        }
        let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("That is not a regular file".to_string());
        }
        if meta.len() > MAX_MEDIA_BYTES {
            return Err(format!(
                "This media file is larger than the {}MB preview limit.",
                MAX_MEDIA_BYTES / (1024 * 1024)
            ));
        }
        let mime = media_mime_for(&real)
            .ok_or_else(|| "That file is not an image, video, audio, or PDF.".to_string())?;
        let bytes = std::fs::read(&real).map_err(|e| e.to_string())?;
        let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(serde_json::json!({
            "mime": mime,
            "base64": base64,
            "sizeBytes": bytes.len(),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn write_workspace_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
    expected_sha256: Option<String>,
    force: Option<bool>,
) -> Result<serde_json::Value, String> {
    let target = ravenbot_core::expand_home(path.trim());
    // Resolving the configured roots touches the database, so it happens on the
    // async side; the file work itself must not run on a runtime core thread.
    let roots = configured_workspace_roots(&state).await;
    tauri::async_runtime::spawn_blocking(move || {
        write_workspace_file_impl(
            &target,
            &content,
            expected_sha256.as_deref(),
            force.unwrap_or(false),
            &roots,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The filesystem half of `write_workspace_file`, split out so the safety rules
/// can be tested without a Tauri app or a database.
fn write_workspace_file_impl(
    target: &std::path::Path,
    content: &str,
    expected_sha256: Option<&str>,
    force: bool,
    roots: &[std::path::PathBuf],
) -> Result<serde_json::Value, String> {
    const MAX_EDIT_BYTES: usize = 4 * 1024 * 1024;
    if content.len() > MAX_EDIT_BYTES {
        return Err("This file is larger than the 4MB editor limit.".to_string());
    }
    if target.as_os_str().is_empty() {
        return Err("No file given".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "That path has no parent folder".to_string())?;
    let parent_real =
        std::fs::canonicalize(parent).map_err(|e| format!("Folder not found: {e}"))?;
    let file_name = target
        .file_name()
        .ok_or_else(|| "That path has no file name".to_string())?;

    // Resolve an existing target (including symlinks) so a link pointing out of
    // the workspace cannot be used to write somewhere else. `symlink_metadata`
    // is deliberate: `exists()` is false for a dangling symlink, and writing
    // through one would create a file outside the workspace.
    let target_real = if std::fs::symlink_metadata(target).is_ok() {
        std::fs::canonicalize(target).map_err(|e| format!("Cannot resolve file: {e}"))?
    } else {
        parent_real.join(file_name)
    };
    if target_real.is_dir() {
        return Err("That is a folder, not a file".to_string());
    }
    if let Ok(meta) = std::fs::metadata(&target_real) {
        if !meta.is_file() {
            // A FIFO or device node inside a workspace would block a read (or
            // turn a save into something other than a text write).
            return Err("Refused: that is not a regular file.".to_string());
        }
    }

    if !roots.iter().any(|root| target_real.starts_with(root)) {
        return Err(
            "Refused: that file is outside every configured project workspace.".to_string(),
        );
    }

    // The editor cannot open a file larger than this in the first place, so a
    // file that grew past the limit while the user was typing is not something
    // we can reload or merge. Refuse without reading gigabytes into memory.
    if std::fs::metadata(&target_real)
        .map(|meta| meta.len() > MAX_EDIT_BYTES as u64)
        .unwrap_or(false)
    {
        return Err(
            "Refused: the file on disk is now larger than the 4MB editor limit.".to_string(),
        );
    }
    let current = std::fs::read(&target_real).ok();
    if let Some(bytes) = &current {
        if std::str::from_utf8(bytes).is_err() {
            return Err("Refused: this is a binary file.".to_string());
        }
    }
    let current_hash = current.as_ref().map(|b| file_version(b));

    if !force {
        match (expected_sha256, current_hash.as_deref()) {
            (Some(expected), Some(actual)) if expected != actual => {
                let text = current
                    .as_ref()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_default();
                return Ok(serde_json::json!({
                    "status": "conflict",
                    "sha256": actual,
                    "text": text,
                    "sizeBytes": current.as_ref().map(|b| b.len()).unwrap_or(0),
                    "message": "The file changed on disk since you opened it.",
                }));
            }
            (Some(_), None) => {
                return Ok(serde_json::json!({
                    "status": "conflict",
                    "sha256": null,
                    "text": "",
                    "sizeBytes": 0,
                    "message": "The file was deleted on disk since you opened it.",
                }));
            }
            _ => {}
        }
    }

    let old_permissions = std::fs::metadata(&target_real)
        .ok()
        .map(|m| m.permissions());
    let write_parent = target_real.parent().unwrap_or(&parent_real);
    let temp = write_parent.join(format!(
        ".{}.ravenbot-{}.tmp",
        file_name.to_string_lossy(),
        Uuid::new_v4()
    ));
    std::fs::write(&temp, content.as_bytes()).map_err(|e| format!("Could not write file: {e}"))?;
    if let Some(permissions) = old_permissions {
        let _ = std::fs::set_permissions(&temp, permissions);
    }
    #[cfg(windows)]
    if target_real.exists() {
        let _ = std::fs::remove_file(&target_real);
    }
    if let Err(e) = std::fs::rename(&temp, &target_real) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Could not save file: {e}"));
    }
    let meta = std::fs::metadata(&target_real).map_err(|e| e.to_string())?;
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis());
    tracing::info!(
        path = %target_real.display(),
        bytes = content.len(),
        "Workspace file saved by user"
    );
    Ok(serde_json::json!({
        "status": "saved",
        "sha256": file_version(content.as_bytes()),
        "sizeBytes": meta.len(),
        "modifiedMs": modified_ms,
    }))
}

/// A one-line description of a workspace, for a header.
#[tauri::command]
async fn summarize_workspace(path: String) -> Result<String, String> {
    let root = ravenbot_core::expand_home(path.trim());
    Ok(ravenbot_runtime::tree::summarize(&root))
}

/// The folder an agent works in when nothing else is set.
///
/// Without this, "inherits the office workspace" is not a thing the user can
/// look at — the default case has no path to show them, so the one workspace
/// that is created automatically is the one workspace they can never inspect.
#[tauri::command]
async fn default_workspace_for(name: String) -> Result<String, String> {
    Ok(default_workspace_for_impl(&name))
}

/// Pure path arithmetic: the folder is *derived* from the agent's name rather
/// than created, so there is nothing here that can fail.
fn default_workspace_for_impl(name: &str) -> String {
    ravenbot_core::office_workspace(name)
        .to_string_lossy()
        .to_string()
}

/// Reveal a workspace in the system file manager.
///
/// The browser is for looking; this is for the case where the user wants the
/// folder in their own tools, which is the natural thing to want once they can
/// see that the agents are working in it.
#[tauri::command]
async fn open_workspace_in_file_manager(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let root = ravenbot_core::expand_home(path.trim());
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&root)
        .map_err(|e| format!("Could not open the file manager: {e}"))
}

/// Auth → configure key; rate limit → wait; network → check connectivity;
/// everything else → how to fix the config.
fn error_hint(err: &str) -> &'static str {
    let e = err.to_lowercase();
    if e.contains("modelprotocolunsupported") || e.contains("model does not support this protocol")
    {
        "Fix: this model is served over a protocol RAVENBOT does not speak yet (OpenAI Responses or Google native). Pick a Chat Completions model or a Claude model from the same provider."
    } else if e.contains("does not match pattern") || e.contains("function.name") {
        "Fix: a tool name contained characters the provider rejects. This is a RAVENBOT bug; update to the build with tool-name sanitization and retry."
    } else if e.contains("endpoint is unavailable") || e.contains("503") {
        "Fix: the provider's endpoint is temporarily unavailable. Retry the run; if it persists, switch this agent to a different model or provider."
    } else if e.contains("free usage limit") || e.contains("subscribe to opencode go") {
        "Fix: the OpenCode Go free allowance on this key is exhausted. Subscribe or top up at https://opencode.ai/console, or switch the agent to another provider/model in **Settings → API Keys**."
    } else if e.contains("free tier") && e.contains("opencode") {
        "Fix: that free tier only works inside the provider's own client. Pick a model from a provider you have a key for in **Settings (⌘,) → API Keys**."
    } else if e.contains("missing session") || e.contains("missingsessionid") {
        "Fix: this model is only served inside its own client, not over the API. Pick a different model or provider."
    } else if e.contains("api key")
        || e.contains("unauthorized")
        || e.contains("401")
        || e.contains("403")
    {
        "Fix: configure the API key for this provider in **Settings (⌘,) → API Keys**."
    } else if e.contains("rate limited") || e.contains("429") {
        "Fix: the provider is rate-limiting you. RAVENBOT paces and retries requests automatically; wait for the cooldown, or switch this office to a provider with more headroom."
    } else if e.contains("internal server error")
        || e.contains("500")
        || e.contains("502")
        || e.contains("503")
    {
        "Fix: the provider returned a server error (their side). Try again, or pick a different model/provider for this agent."
    } else if e.contains("timed out") || e.contains("connection") || e.contains("sending request") {
        "Fix: check your network connection, or switch the bot to a local model in its settings."
    } else if e.contains("insufficient credits")
        || e.contains("insufficient funds")
        || e.contains("credits to make this request")
        || e.contains("payment required")
        || e.contains("402")
    {
        "Fix: this provider account has no credit left. Top it up at the provider's dashboard (for OpenCode Go, enable Extra Usage or add balance at https://opencode.ai/console), switch the agent to a provider with remaining credit in **Settings (⌘,) → API Keys**, or run it on a local model."
    } else if e.contains("unknown provider") {
        "Fix: pick a valid model provider in the bot settings (openrouter, anthropic, openai, opencode-go, ollama)."
    } else if e.contains("no local model") || e.contains("local inference") {
        "Fix: set a local model path in **Settings (⌘,) → Local Models**, or pick a hosted provider for this bot."
    } else {
        "Fix: review the bot's provider/model settings, then resend."
    }
}

/// Re-run the last user message: removes the trailing assistant message
/// (if any) and executes a fresh run without duplicating the user turn.
#[tauri::command]
async fn regenerate_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
) -> Result<Message, String> {
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let _last_user = messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, ravenbot_core::MessageRole::User))
        .cloned()
        .ok_or_else(|| "No user message to regenerate from".to_string())?;

    // Remove the trailing assistant message(s) so the run produces a fresh one
    for msg in messages.iter().rev() {
        if matches!(msg.role, ravenbot_core::MessageRole::Assistant) {
            ravenbot_db::queries::MessageQueries::delete(state.db.pool(), msg.id)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            break;
        }
    }

    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    if let Err(err) = execute_run_with_stream(&state, &app, &mut run).await {
        tracing::warn!("Regenerate run error: {}", err);
        let error_msg = Message::assistant(
            thread_id,
            format!(
                "⚠️ **Model Error:** {}\n\n{}{}",
                err,
                error_hint(&err),
                "\n\nYou can also run me fully offline via a local Ollama model."
            ),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
        );
        return Ok(error_msg);
    }

    let _ = app.emit(
        "agent-stream",
        serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
    );

    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;
    messages
        .last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

#[tauri::command]
async fn execute_graph(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    orchestrator_bot_id: Uuid,
    goal: String,
    tasks: Vec<GraphTask>,
    chatroom_id: Option<Uuid>,
) -> Result<GraphResult, String> {
    // When the plan belongs to a room, resolve its roster/thread so the run is
    // posted as a genuine, attributed office conversation. A standalone graph
    // (no chatroom) keeps the old headless behaviour.
    let roster = match chatroom_id {
        Some(cid) => Some(load_office_roster(state.db.pool(), cid).await?),
        None => None,
    };
    let thread_id = match (&roster, chatroom_id) {
        (Some(r), Some(cid)) => {
            Some(ensure_chatroom_thread(state.db.pool(), cid, r.bots[0].1.id).await?)
        }
        _ => None,
    };

    // The lead is the explicitly requested orchestrator when it is a member of
    // this room, otherwise the room's resolved lead.
    let (lead_id, lead_name) = match &roster {
        Some(r) => {
            if orchestrator_bot_id != Uuid::nil()
                && r.bots.iter().any(|(_, b)| b.id == orchestrator_bot_id)
            {
                (orchestrator_bot_id, roster_name(r, orchestrator_bot_id))
            } else {
                (r.lead_id, r.lead_name.clone())
            }
        }
        None => (orchestrator_bot_id, "Team Lead".to_string()),
    };

    // Record the dispatched plan as the client's directive, and let the lead
    // brief the team in the room.
    if let (Some(r), Some(tid)) = (&roster, thread_id) {
        let directive = Message::user(tid, format!("🎯 **Plan Mode** — goal: {}", goal.trim()));
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &directive).await;
        let items: Vec<(String, String, Vec<usize>)> = tasks
            .iter()
            .map(|t| {
                (
                    roster_name(r, t.bot_id),
                    briefing_label(&t.instruction),
                    t.depends_on.clone(),
                )
            })
            .collect();
        if !items.is_empty() {
            let _ = post_bot_message(
                state.db.pool(),
                tid,
                lead_id,
                lead_name.clone(),
                briefing_markdown(&items),
            )
            .await;
        }
    }

    // Create the task graph
    let mut graph = TaskGraph::new(&goal);
    let mut node_ids: Vec<Uuid> = Vec::new();
    for task in &tasks {
        let node_id = graph.add_node(task.bot_id, &task.instruction);
        node_ids.push(node_id);
        for &dep_idx in &task.depends_on {
            if dep_idx < node_ids.len() {
                graph.add_edge(node_ids[dep_idx], node_id);
            }
        }
    }

    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    // Board persistence for Plan mode. The normal office path owns this in
    // office_turn; without it here, a plan-mode board disappears when the view
    // is closed because only the frontend used to write it.
    let (board_tx, mut board_rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
    if let Some(tid) = thread_id {
        let board_db = state.db.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = board_rx.recv().await {
                apply_office_board_event(&board_db, tid, &event).await;
            }
        });
    }
    // Seed the room's live planner board before execution starts.
    if let Some(tid) = thread_id {
        let nodes: Vec<serde_json::Value> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                serde_json::json!({
                    "node_id": node_ids[i].to_string(),
                    "bot_id": task.bot_id.to_string(),
                    "label": briefing_label(&task.instruction),
                    "depends_on": task
                        .depends_on
                        .iter()
                        .filter(|d| **d < node_ids.len() && **d < i)
                        .map(|d| node_ids[*d].to_string())
                        .collect::<Vec<String>>(),
                })
            })
            .collect();
        let payload = serde_json::json!({
            "kind": "plan_ready",
            "thread_id": tid.to_string(),
            "goal": truncate_chars(goal.trim(), 200),
            "nodes": nodes,
        });
        let _ = board_tx.send(payload.clone());
        let _ = app.emit("agent-stream", payload);
    }
    // Project folders for every node in this office (same rule as send_to_chatroom).
    let office_dirs: Vec<String> = match chatroom_id {
        Some(cid) => match ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), cid).await {
            Ok(Some(r)) if !r.project_folders.is_empty() => r.project_folders.clone(),
            Ok(Some(r)) => vec![ravenbot_runtime::default_project_dir(&r.name)
                .to_string_lossy()
                .to_string()],
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let node_event_app = app.clone();
    let node_event_board = board_tx.clone();
    let emitter_app = app.clone();
    let executor =
        ravenbot_runtime::executor::GraphExecutor::new(state.runtime.clone(), state.db.clone())
            .with_working_dirs(office_dirs)
            .with_thread_emitter(Arc::new(move |_thread_id| {
                Some(make_stream_emitter(emitter_app.clone()))
            }))
            .with_node_events(Arc::new(move |v| {
                let _ = node_event_board.send(v.clone());
                let _ = node_event_app.emit("agent-stream", v);
            }));

    let exec_result = executor.execute(graph.clone()).await;

    let blackboard = exec_result.map_err(|e| e.to_string())?;
    let graph_snapshot = graph.lock().await.clone();
    let checklist = graph_snapshot.to_checklist();

    // Post each node's real output + the lead's synthesis into the room.
    let summary = if let (Some(r), Some(tid)) = (&roster, thread_id) {
        let mut outputs: Vec<(String, String)> = Vec::new();
        for nid in graph_snapshot.ordered_node_ids() {
            let Some(node) = graph_snapshot.nodes.get(&nid) else {
                continue;
            };
            let Some(output) = &node.output else { continue };
            let name = roster_name(r, node.bot_id);
            // Pure conversation: the bubble carries only the agent's reply;
            // author identity lives in the row gutter, task text on the board.
            let body = output.trim().to_string();
            let _ = post_bot_message(state.db.pool(), tid, node.bot_id, name.clone(), body).await;
            outputs.push((name, output.clone()));
        }

        // Surface failed and skipped nodes so a provider error or a blocked
        // dependency is visible and actionable.
        let failures: Vec<String> = checklist
            .iter()
            .filter(|c| {
                matches!(
                    c.status,
                    ravenbot_core::ChecklistStatus::Failed
                        | ravenbot_core::ChecklistStatus::Skipped
                )
            })
            .map(|c| {
                let who = c
                    .bot_id
                    .map(|id| roster_name(r, id))
                    .unwrap_or_else(|| "Teammate".to_string());
                let detail = match c.status {
                    ravenbot_core::ChecklistStatus::Skipped => {
                        "skipped because a dependency did not complete".to_string()
                    }
                    _ => c
                        .result
                        .clone()
                        .unwrap_or_else(|| "unknown error".to_string()),
                };
                format!("• **{}** — {}", who, detail)
            })
            .collect();
        if !failures.is_empty() {
            let hint = error_hint(failures.first().map(|s| s.as_str()).unwrap_or(""));
            let _ = post_bot_message(
                state.db.pool(),
                tid,
                lead_id,
                lead_name.clone(),
                format!(
                    "⚠️ Some teammates did not complete:\n{}\n\n{}",
                    failures.join("\n"),
                    hint
                ),
            )
            .await;
        }

        let synthesis_results = if failures.is_empty() {
            outputs.clone()
        } else {
            let mut results = outputs.clone();
            results.push((
                "Incomplete or failed tasks".to_string(),
                failures.join("\n"),
            ));
            results
        };
        let final_text = if outputs.is_empty() {
            checklist
                .iter()
                .map(|c| {
                    format!(
                        "{} {} → {}",
                        match c.status {
                            ravenbot_core::ChecklistStatus::Completed => "✓",
                            ravenbot_core::ChecklistStatus::Failed => "✗",
                            _ => "○",
                        },
                        c.label,
                        c.result.clone().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            state
                .runtime
                .synthesize_office(lead_id, &goal, &synthesis_results)
                .await
        };
        let _ = post_bot_message(
            state.db.pool(),
            tid,
            lead_id,
            lead_name.clone(),
            final_text.clone(),
        )
        .await;
        Some(final_text)
    } else {
        None
    };

    // Close the run only after the room holds the full persisted conversation
    // (matches send_to_chatroom; the UI holds live lanes until this arrives).
    if let Some(tid) = thread_id {
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({ "kind": "done", "thread_id": tid.to_string() }),
        );
    }

    Ok(GraphResult {
        goal,
        thread_id,
        checklist,
        blackboard_data: blackboard.data,
        summary,
    })
}

/// Ask the room's lead to draft a task plan for a goal, so Plan mode can be
/// seeded by the CEO rather than only hand-authored. Returns the tasks
/// (assignee bot ids) plus an optional clarifying question.
#[tauri::command]
async fn draft_office_plan(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    goal: String,
) -> Result<serde_json::Value, String> {
    if goal.trim().is_empty() {
        return Err("Describe the goal first.".to_string());
    }
    let roster = load_office_roster(state.db.pool(), chatroom_id).await?;
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    let plan = state
        .runtime
        .plan_office(
            roster.lead_id,
            room.as_ref().and_then(|r| r.goal.as_deref()),
            room.as_ref().and_then(|r| r.policy.as_deref()),
            &roster.members,
            &goal,
        )
        .await;
    let Some(plan) = plan else {
        return Ok(serde_json::json!({
            "tasks": [],
            "question": null,
            "lead_id": roster.lead_id,
            "lead_name": roster.lead_name,
        }));
    };
    let tasks: Vec<serde_json::Value> = plan
        .tasks
        .iter()
        .map(|t| {
            let member = ravenbot_runtime::orchestrator::resolve_member(&roster.members, &t.bot);
            let bot_id = member.map(|m| m.bot_id).unwrap_or(roster.lead_id);
            serde_json::json!({
                "bot_id": bot_id,
                "bot_name": member.map(|m| m.name.clone()).unwrap_or_else(|| t.bot.clone()),
                "instruction": t.instruction,
                "depends_on": t.depends_on,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "tasks": tasks,
        "question": plan.question,
        "lead_id": roster.lead_id,
        "lead_name": roster.lead_name,
    }))
}

#[tauri::command]
async fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state
        .runtime
        .trigger_kill_switch("User triggered pause all")
        .await;
    tracing::info!("All bots paused via kill switch");
    Ok(())
}

#[tauri::command]
async fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.release_kill_switch().await;
    tracing::info!("All bots resumed");
    Ok(())
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<StatusInfo, String> {
    let bots = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;

    let kill_switch_active = state.runtime.is_paused().await;

    Ok(StatusInfo {
        active_bots: bots.len() as u32,
        running_tasks: 0,
        session_tokens: 0,
        session_cost: 0.0,
        kill_switch_active,
    })
}

#[tauri::command]
async fn trigger_kill_switch(state: State<'_, AppState>, reason: String) -> Result<(), String> {
    state.runtime.trigger_kill_switch(reason).await;
    Ok(())
}

#[tauri::command]
async fn release_kill_switch(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.release_kill_switch().await;
    Ok(())
}

#[tauri::command]
async fn get_kill_switch_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let status = state.runtime.kill_switch().status().await;
    Ok(serde_json::json!({
        "state": format!("{:?}", status.state),
        "reason": status.reason,
        "triggered_at": status.triggered_at.map(|dt| dt.to_rfc3339()),
    }))
}

#[tauri::command]
async fn list_all_skills(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let reg = state.runtime.skill_registry();
    Ok(reg
        .list()
        .iter()
        .map(|s| {
            serde_json::json!({
                // The real wire shape, not `{:?}`. The settings screen reads these to
                // show which capabilities an equipped skill needs, and it has to be able
                // to name the variant; a Debug string would have to be taken apart again
                // in the UI to learn that `FileSystem { paths: ["/"] }` means the file
                // system at all.
                "id": s.id(),
                "name": s.name(),
                "description": s.description(),
                "permissions": s.required_permissions()
            })
        })
        .collect())
}

// ——— Plugins: real OpenAPI specs, parsed into executable skills ———

const MAX_OPENAPI_BYTES: usize = 20 * 1024 * 1024;

async fn fetch_openapi_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Use an http:// or https:// URL for the OpenAPI spec.".to_string());
    }
    let response = client
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "application/json, application/yaml, text/yaml, */*",
        )
        .send()
        .await
        .map_err(|e| format!("Could not fetch {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Spec server returned {} for {url}.",
            response.status().as_u16()
        ));
    }
    if let Some(length) = response.content_length() {
        if length > MAX_OPENAPI_BYTES as u64 {
            return Err(format!(
                "Spec is larger than {} MB.",
                MAX_OPENAPI_BYTES / (1024 * 1024)
            ));
        }
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Could not read {url}: {e}"))?;
    if bytes.len() > MAX_OPENAPI_BYTES {
        return Err(format!(
            "Spec is larger than {} MB.",
            MAX_OPENAPI_BYTES / (1024 * 1024)
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "Spec is not valid UTF-8.".to_string())
}

/// Fetch an OpenAPI document, following an `ai-plugin.json` manifest when the
/// URL points at one. Returns the resolved spec URL, raw document, and display
/// metadata.
async fn load_openapi_document(
    source_url: &str,
) -> Result<(String, String, ravenbot_plugins::openapi::SpecMetadata), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let raw = fetch_openapi_text(&client, source_url).await?;

    // An AI-plugin manifest has `api.url`, not `paths`. Resolve it relative to
    // the manifest so both absolute and relative API URLs work.
    if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&raw) {
        if manifest.get("paths").is_none() {
            if let Some(api_url) = manifest
                .pointer("/api/url")
                .and_then(|value| value.as_str())
            {
                let base = reqwest::Url::parse(source_url)
                    .map_err(|e| format!("Invalid manifest URL: {e}"))?;
                let resolved = base
                    .join(api_url)
                    .map_err(|e| format!("Invalid api.url in manifest: {e}"))?;
                let spec = fetch_openapi_text(&client, resolved.as_str()).await?;
                let mut metadata = ravenbot_plugins::openapi::spec_metadata(&spec)?;
                if metadata.title == "OpenAPI App" {
                    if let Some(name) = manifest
                        .get("name_for_human")
                        .and_then(|value| value.as_str())
                    {
                        metadata.title = name.to_string();
                    }
                }
                if metadata.description.is_empty() {
                    if let Some(description) = manifest
                        .get("description_for_human")
                        .and_then(|value| value.as_str())
                    {
                        metadata.description = description.to_string();
                    }
                }
                if metadata.logo.is_none() {
                    metadata.logo = manifest
                        .get("logo_url")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                }
                return Ok((resolved.to_string(), spec, metadata));
            }
        }
    }

    let metadata = ravenbot_plugins::openapi::spec_metadata(&raw)?;
    Ok((source_url.to_string(), raw, metadata))
}

#[tauri::command]
async fn sync_plugins(
    state: State<'_, AppState>,
) -> Result<ravenbot_plugins::store::SyncReport, String> {
    use ravenbot_plugins::openapi::{parse_openapi_spec, spec_metadata, store_operations};
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    let removed = store.remove_invalid_plugins().await?;
    let mut verified = 0usize;
    let mut failed = Vec::new();
    for (plugin_id, spec) in store.all_specs().await? {
        match parse_openapi_spec(&spec) {
            Ok(ops) if !ops.is_empty() => {
                store_operations(state.db.pool(), &plugin_id, &ops).await?;
                if let Ok(metadata) = spec_metadata(&spec) {
                    // A manifest can carry the human name while its API
                    // document has no `info.title`. Do not overwrite the
                    // imported name with the generic fallback.
                    if metadata.title != "OpenAPI App" {
                        let _ = store
                            .set_metadata(
                                &plugin_id,
                                &metadata.title,
                                &metadata.description,
                                metadata.logo.as_deref(),
                                metadata.requires_auth,
                                metadata.credential_header.as_deref(),
                            )
                            .await;
                    }
                }
                verified += 1;
            }
            Ok(_) => failed.push(ravenbot_plugins::store::SyncFailure {
                plugin_id,
                error: "The spec contains no operations.".to_string(),
            }),
            Err(error) => failed.push(ravenbot_plugins::store::SyncFailure {
                plugin_id,
                error,
            }),
        }
    }
    Ok(ravenbot_plugins::store::SyncReport {
        verified,
        removed,
        failed,
    })
}

#[tauri::command]
async fn list_plugins(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<ravenbot_plugins::store::PluginSummary>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_summaries(query.as_deref()).await
}

#[tauri::command]
async fn get_plugin_details(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store
        .details(&plugin_id)
        .await?
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not installed."))
}

#[tauri::command]
async fn list_bot_plugins(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<String>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_bot_plugins(bot_id).await
}

#[tauri::command]
async fn toggle_bot_plugin(
    state: State<'_, AppState>,
    bot_id: Uuid,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.set_bot_plugin(bot_id, &plugin_id, enabled).await
}

/// Enable/disable a plugin for EVERY bot (MCP-style global scope, P8).
#[tauri::command]
async fn toggle_plugin_global(
    state: State<'_, AppState>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.set_plugin_global(&plugin_id, enabled).await
}

#[tauri::command]
async fn list_global_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<(String, String, String, String)>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_global_plugins().await
}

#[tauri::command]
async fn set_plugin_credential(
    state: State<'_, AppState>,
    plugin_id: String,
    header_name: String,
    header_value: String,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    if store.summary(&plugin_id).await?.is_none() {
        return Err(format!("Plugin '{plugin_id}' is not installed."));
    }
    store
        .set_credential(&plugin_id, &header_name, &header_value)
        .await?;
    store
        .details(&plugin_id)
        .await?
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not installed."))
}

#[tauri::command]
async fn set_plugin_base_url(
    state: State<'_, AppState>,
    plugin_id: String,
    base_url: Option<String>,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    if store.summary(&plugin_id).await?.is_none() {
        return Err(format!("Plugin '{plugin_id}' is not installed."));
    }
    store.set_base_url(&plugin_id, base_url.as_deref()).await?;
    store
        .details(&plugin_id)
        .await?
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not installed."))
}

#[tauri::command]
async fn clear_plugin_credential(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.clear_credential(&plugin_id).await?;
    store
        .details(&plugin_id)
        .await?
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not installed."))
}

async fn import_openapi_document(
    db: &ravenbot_db::Database,
    source_url: &str,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    import_openapi_document_with_auth(db, source_url, None, None).await
}

async fn import_openapi_document_with_auth(
    db: &ravenbot_db::Database,
    source_url: &str,
    requires_auth_override: Option<bool>,
    credential_header_override: Option<&str>,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    use ravenbot_plugins::openapi::{parse_openapi_spec, slugify_title, store_operations};
    let source_url = source_url.trim().to_string();
    let (resolved_url, spec, metadata) = load_openapi_document(&source_url).await?;
    let requires_auth = requires_auth_override.unwrap_or(metadata.requires_auth);
    let credential_header = credential_header_override
        .map(str::to_string)
        .or(metadata.credential_header);
    let ops = parse_openapi_spec(&spec)
        .map_err(|error| format!("That document is not a usable OpenAPI spec: {error}"))?;
    if ops.is_empty() {
        return Err("The spec contains no operations to import.".to_string());
    }
    let store = ravenbot_plugins::store::PluginStore::new(db.pool().clone());
    store.ensure_tables().await?;
    let base_id = slugify_title(&metadata.title);
    let mut plugin_id = base_id.clone();
    if let Some(existing) = store.summary(&plugin_id).await? {
        // Compare the resolved spec, not the manifest URL: re-importing the
        // same ai-plugin.json must update the existing app, not clone it.
        if existing.source_url.as_deref() != Some(resolved_url.as_str()) {
            plugin_id = format!(
                "{}-{}",
                base_id,
                &uuid::Uuid::new_v4().simple().to_string()[..6]
            );
        }
    }
    store
        .upsert_plugin_full(
            &plugin_id,
            &metadata.title,
            &metadata.description,
            metadata.logo.as_deref(),
            Some(&resolved_url),
            &spec,
            requires_auth,
            credential_header.as_deref(),
        )
        .await?;
    store_operations(db.pool(), &plugin_id, &ops).await?;
    store
        .details(&plugin_id)
        .await?
        .ok_or_else(|| "The imported plugin could not be read back.".to_string())
}

#[tauri::command]
async fn import_openapi_plugin(
    state: State<'_, AppState>,
    manifest_url: String,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    import_openapi_document(&state.db, &manifest_url).await
}

/// Search the installable OpenAPI directory.
///
/// The first call may download the directory (about 9 MB). Later calls read
/// the cached index, and the picker can request a refresh explicitly.
#[tauri::command]
async fn search_plugin_catalog(
    state: State<'_, AppState>,
    query: Option<String>,
    category: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    refresh: Option<bool>,
) -> Result<ravenbot_plugins::catalog::CatalogPage, String> {
    state
        .plugin_catalog
        .page(
            query.as_deref(),
            category.as_deref(),
            limit
                .map(|value| value as usize)
                .unwrap_or(ravenbot_plugins::catalog::DEFAULT_PAGE_SIZE),
            offset.unwrap_or(0) as usize,
            refresh.unwrap_or(false),
        )
        .await
}

/// Pick a spec URL for a catalog install, accepting only directory URLs as a
/// fallback. The renderer never gets to point the catalog path at an
/// arbitrary host; manual imports use `import_openapi_plugin` instead.
fn resolve_catalog_install_url(
    catalog_id: &str,
    resolved: Option<String>,
    fallback: Option<String>,
) -> Result<String, String> {
    if let Some(url) = resolved.filter(|url| !url.trim().is_empty()) {
        return Ok(url);
    }
    let fallback = fallback.unwrap_or_default();
    if catalog_id.starts_with("apisguru:")
        && fallback.starts_with("https://api.apis.guru/v2/specs/")
    {
        return Ok(fallback);
    }
    Err(format!(
        "Unknown catalog app '{catalog_id}'. Refresh the catalog and try again."
    ))
}

#[tauri::command]
async fn install_catalog_plugin(
    state: State<'_, AppState>,
    catalog_id: String,
    spec_url: Option<String>,
) -> Result<ravenbot_plugins::store::PluginDetails, String> {
    let target = state.plugin_catalog.spec_url_for(&catalog_id).await?;
    let requires_auth = target.as_ref().and_then(|target| target.requires_auth);
    let credential_header = target
        .as_ref()
        .and_then(|target| target.credential_header_hint.clone());
    let resolved = target.map(|target| target.spec_url);
    let spec_url = resolve_catalog_install_url(&catalog_id, resolved, spec_url)?;
    import_openapi_document_with_auth(
        &state.db,
        &spec_url,
        requires_auth,
        credential_header.as_deref(),
    )
    .await
}

// ——— MCP — 60+ servers as native tools, when necessary the bot will use them ———
#[tauri::command]
async fn list_mcp_servers(
    state: State<'_, AppState>,
    category: Option<String>,
) -> Result<Vec<ravenbot_mcp::McpServerSummary>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.list_server_summaries(category.as_deref()).await
}
#[tauri::command]
async fn toggle_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
    enabled: bool,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.set_server_enabled(&server_id, enabled).await
}
#[tauri::command]
async fn toggle_bot_mcp_server(
    state: State<'_, AppState>,
    bot_id: Uuid,
    server_id: String,
    enabled: bool,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.set_bot_server(bot_id, &server_id, enabled).await
}
#[tauri::command]
async fn list_bot_mcp_servers(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<String>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.list_bot_servers(bot_id).await
}
#[tauri::command]
async fn save_custom_mcp_server(
    state: State<'_, AppState>,
    server: ravenbot_mcp::McpServerConfig,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.save_custom_server(server).await
}
#[tauri::command]
async fn delete_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.delete_server(&server_id).await
}
#[tauri::command]
async fn get_mcp_server_env(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<std::collections::HashMap<String, String>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.get_server_env(&server_id).await
}
#[tauri::command]
async fn save_mcp_server_env(
    state: State<'_, AppState>,
    server_id: String,
    env: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.save_server_env(&server_id, env).await
}
#[tauri::command]
async fn test_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<ravenbot_mcp::McpTestResult, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.test_server(&server_id).await
}
#[tauri::command]
async fn batch_assign_bot_mcp(
    state: State<'_, AppState>,
    server_id: String,
    bot_ids: Vec<Uuid>,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.batch_assign_bot_servers(&server_id, bot_ids).await
}
#[tauri::command]
async fn batch_set_bot_mcp(
    state: State<'_, AppState>,
    bot_id: Uuid,
    server_ids: Vec<String>,
) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.batch_set_bot_servers(bot_id, server_ids).await
}

// ——— Budgets — real enforcement + tracking ———

#[tauri::command]
async fn get_bot_budget(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<serde_json::Value, String> {
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    let budget = budgets
        .get_budget(bot_id)
        .await
        .map_err(|e| e.to_string())?;
    let (tokens_used, cost_used) = budgets.get_usage(bot_id).await.map_err(|e| e.to_string())?;
    let check = budgets
        .check_budget(bot_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "budget": budget.map(|b| serde_json::json!({
            "limit": match &b.limit {
                ravenbot_core::BudgetLimit::Unlimited => serde_json::json!({"kind": "unlimited"}),
                ravenbot_core::BudgetLimit::Tokens { max } => serde_json::json!({"kind": "tokens", "max": max}),
                ravenbot_core::BudgetLimit::Cost { max } => serde_json::json!({"kind": "cost", "max": max}),
            },
            "period": match b.period {
                ravenbot_core::BudgetPeriod::Hourly => "hourly",
                ravenbot_core::BudgetPeriod::Daily => "daily",
                ravenbot_core::BudgetPeriod::Weekly => "weekly",
                ravenbot_core::BudgetPeriod::Monthly => "monthly",
                ravenbot_core::BudgetPeriod::Total => "total",
            },
        })),
        "tokens_used": tokens_used,
        "cost_used": cost_used,
        "percentage_used": check.percentage_used,
        "allowed": check.allowed,
        "should_warn": check.should_warn,
    }))
}

#[tauri::command]
async fn set_bot_budget(
    state: State<'_, AppState>,
    bot_id: Uuid,
    kind: String,
    max: f64,
    period: String,
) -> Result<(), String> {
    let limit = match kind.as_str() {
        "tokens" => ravenbot_core::BudgetLimit::Tokens { max: max as u64 },
        "cost" => ravenbot_core::BudgetLimit::Cost { max },
        _ => ravenbot_core::BudgetLimit::Unlimited,
    };
    let budget_period = match period.as_str() {
        "hourly" => ravenbot_core::BudgetPeriod::Hourly,
        "daily" => ravenbot_core::BudgetPeriod::Daily,
        "weekly" => ravenbot_core::BudgetPeriod::Weekly,
        "monthly" => ravenbot_core::BudgetPeriod::Monthly,
        _ => ravenbot_core::BudgetPeriod::Total,
    };
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    budgets
        .set_budget(&ravenbot_core::Budget::new(bot_id, limit, budget_period))
        .await
}

#[tauri::command]
async fn reset_bot_budget(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    budgets.reset_usage(bot_id).await
}

/// Lifetime usage for a bot (summed over all its runs) — so the telemetry
/// pill survives app restarts instead of resetting with the session.
#[tauri::command]
async fn get_session_usage(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<serde_json::Value, String> {
    let row: Option<(i64, f64)> = sqlx::query_as(
        "SELECT COALESCE(SUM(r.tokens_consumed), 0), COALESCE(SUM(r.cost_estimate), 0.0)
         FROM runs r
         JOIN threads t ON t.id = r.thread_id
         WHERE t.bot_id = ?",
    )
    .bind(bot_id.to_string())
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| e.to_string())?
    .or(Some((0, 0.0)));

    let (tokens, cost) = row.unwrap_or((0, 0.0));
    Ok(serde_json::json!({ "tokens": tokens, "cost": cost }))
}

// ——— ChatRoom / Office Team ———

#[tauri::command]
async fn create_chatroom(
    state: State<'_, AppState>,
    name: String,
    description: String,
    office_template: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
    // An existing folder to work in. Omit to get a fresh room created under
    // `~/RAVENBOT/projects/`.
    project_folder: Option<String>,
    goal: Option<String>,
    policy: Option<String>,
) -> Result<ChatRoom, String> {
    let mut room = ChatRoom::new(name, description, office_template.clone());
    if let Some(url) = avatar_url {
        room.avatar_url = Some(url);
    }
    if let Some(style) = avatar_style {
        room.avatar_style = Some(style);
    }
    room.goal = goal.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    // An office with no policy is an office where every agent invents its own
    // standards, so a new one starts from the template's. The user can edit or
    // clear it afterwards; `write_policy` keeps POLICY.md in step either way.
    room.policy = policy
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .or_else(|| Some(office_org::template_policy(&office_template).to_string()));

    // An office always gets a room, at creation rather than lazily at first
    // send. `project_folder` lets a user point the office at a directory they
    // already have; otherwise we mint `~/RAVENBOT/projects/<slug>/` and seed
    // the charter files the agents read on their first turn.
    match project_folder
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(chosen) => {
            // The user's folder is the office. Do NOT also mint a default
            // room — that would leave a stray empty directory and give the
            // agents a charter pointing somewhere they cannot write.
            let path = ravenbot_core::expand_home(chosen);
            ravenbot_core::ensure_dir(&path);
            ravenbot_runtime::workspace::seed_at(&path, &room, &[]);
            room.project_folders = vec![path.to_string_lossy().to_string()];
        }
        None => {
            let ws = ravenbot_runtime::workspace::seed(&room, &[]);
            room.project_folders = ravenbot_runtime::workspace::project_folders(&ws);
        }
    }

    ravenbot_db::queries::ChatRoomQueries::create(state.db.pool(), &room)
        .await
        .map_err(|e| e.to_string())?;
    Ok(room)
}

/// Re-seed an office's charter files from the current room + roster.
///
/// Called whenever the roster, goal, or policy changes, so `OFFICE.md` in the
/// workspace always matches what the app displays. Best-effort: a workspace
/// that cannot be written must not fail the edit that triggered it.
async fn refresh_office_workspace(state: &AppState, chatroom_id: Uuid) {
    let Ok(Some(room)) =
        ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id).await
    else {
        return;
    };
    let members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .unwrap_or_default();

    let roster: Vec<ravenbot_runtime::workspace::RosterEntry> = {
        let mut out = Vec::with_capacity(members.len());
        for m in members {
            let Ok(Some(bot)) =
                ravenbot_db::queries::BotQueries::get(state.db.pool(), m.bot_id).await
            else {
                continue;
            };
            let mut entry = ravenbot_runtime::workspace::RosterEntry::new(
                bot.name.clone(),
                if m.rank.trim().is_empty() {
                    bot.rank.clone().unwrap_or_default()
                } else {
                    m.rank.clone()
                },
                if m.specialty.trim().is_empty() {
                    bot.specialty.clone().unwrap_or_default()
                } else {
                    m.specialty.clone()
                },
                bot.is_orchestrator,
            );
            // The roster records how each member works, not only their title,
            // so a teammate reading the folder sees the mandate.
            if let Some(p) = bot
                .config
                .custom_prompt
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                entry = entry.with_mandate(p);
            }
            out.push(entry);
        }
        out
    };

    // Honour a user-chosen folder: seed there rather than in the default room.
    if let Some(first) = room.project_folders.first().map(String::as_str) {
        ravenbot_runtime::workspace::seed_at(&ravenbot_core::expand_home(first), &room, &roster);
    } else {
        ravenbot_runtime::workspace::seed(&room, &roster);
    }
}

#[tauri::command]
async fn list_chatrooms(state: State<'_, AppState>) -> Result<Vec<ChatRoom>, String> {
    ravenbot_db::queries::ChatRoomQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_chatroom(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Option<ChatRoom>, String> {
    ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_member_to_chatroom(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    bot_id: Uuid,
    rank: String,
    specialty: String,
) -> Result<(), String> {
    let member = ChatRoomMember {
        chatroom_id,
        bot_id,
        rank,
        specialty,
        joined_at: chrono::Utc::now(),
    };
    ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member)
        .await
        .map_err(|e| e.to_string())?;
    // Any agent added to an office is now part of autonomous office work.
    if let Ok(Some(mut bot)) = ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id).await
    {
        if !matches!(bot.approval_mode, ravenbot_core::ApprovalMode::Full) {
            bot.approval_mode = ravenbot_core::ApprovalMode::Full;
            ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn list_chatroom_members(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Vec<ChatRoomMember>, String> {
    ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_chatroom(state: State<'_, AppState>, room: ChatRoom) -> Result<(), String> {
    // Keep the policy document in the workspace in step with the field.
    ravenbot_runtime::workspace::write_policy(&room);
    let result = ravenbot_db::queries::ChatRoomQueries::update(state.db.pool(), &room)
        .await
        .map_err(|e| e.to_string());
    if result.is_ok() {
        refresh_office_workspace(&state, room.id).await;
    }
    result
}

#[tauri::command]
async fn delete_chatroom(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::delete(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_chatroom_member(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    bot_id: Uuid,
) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::remove_member(state.db.pool(), chatroom_id, bot_id)
        .await
        .map_err(|e| e.to_string())?;
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn update_chatroom_member(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    bot_id: Uuid,
    rank: String,
    specialty: String,
) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::update_member(
        state.db.pool(),
        chatroom_id,
        bot_id,
        &rank,
        &specialty,
    )
    .await
    .map_err(|e| e.to_string())?;
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn create_bot_for_office(
    state: State<'_, AppState>,
    name: String,
    description: String,
    rank: String,
    specialty: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
    chatroom_id: Option<Uuid>,
) -> Result<Bot, String> {
    let mut bot = Bot::new(name, description);
    if let Some(url) = avatar_url {
        bot.avatar_url = Some(url);
    }
    if let Some(style) = avatar_style {
        bot.avatar_style = Some(style);
    }
    let (default_provider, default_model) = resolve_default_model(state.db.pool()).await;
    bot.config.model_provider = default_provider;
    bot.config.model_id = default_model;
    bot.rank = Some(rank.clone());
    bot.specialty = Some(specialty.clone());
    // This bot is being created directly into an office.
    bot.approval_mode = ravenbot_core::ApprovalMode::Full;
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(cid) = chatroom_id {
        let member = ChatRoomMember {
            chatroom_id: cid,
            bot_id: bot.id,
            rank,
            specialty,
            joined_at: chrono::Utc::now(),
        };
        ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(bot)
}

/// The built-in role blueprint for an office template (CEO + specialists).
#[tauri::command]
async fn default_office_org(office_template: String) -> Result<Vec<office_org::RoleSpec>, String> {
    Ok(office_org::default_org(&office_template))
}

/// Result of staffing an office: which agents were freshly created vs which
/// existing fleet agents were reused (so the same agent is never duplicated).
#[derive(serde::Serialize)]
struct ProvisionOutcome {
    created: Vec<Bot>,
    reused: Vec<Bot>,
}

/// Find an existing fleet agent that should fill a role instead of creating a
/// duplicate. Preference order: exact name → exact rank+specialty → specialty.
fn find_reusable_agent(
    bots: &[Bot],
    used: &std::collections::HashSet<Uuid>,
    role: &office_org::RoleSpec,
) -> Option<Bot> {
    let name = role.name.trim().to_lowercase();
    let rank = role.rank.trim().to_lowercase();
    let specialty = role.specialty.trim().to_lowercase();
    let candidates = || bots.iter().filter(|b| !used.contains(&b.id));

    candidates()
        .find(|b| b.name.trim().to_lowercase() == name)
        .or_else(|| {
            candidates().find(|b| {
                b.rank.as_deref().unwrap_or("").trim().to_lowercase() == rank
                    && b.specialty.as_deref().unwrap_or("").trim().to_lowercase() == specialty
            })
        })
        .or_else(|| {
            if specialty.is_empty() {
                None
            } else {
                candidates().find(|b| {
                    b.specialty.as_deref().unwrap_or("").trim().to_lowercase() == specialty
                })
            }
        })
        .or_else(|| {
            // Rank-only match as a last resort for role titles like "CEO".
            if rank.is_empty() {
                None
            } else {
                candidates().find(|b| b.rank.as_deref().unwrap_or("").trim().to_lowercase() == rank)
            }
        })
        .cloned()
}

/// Staff an office from a role blueprint. Existing fleet agents that match a
/// role are reused (added to the office) instead of being duplicated; only
/// genuinely new roles create new agents. Each bot keeps its own model, skills,
/// MCP servers and connectors, all editable by the owner.
#[tauri::command]
async fn provision_office_org(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    roles: Vec<office_org::RoleSpec>,
    // The model that designed the office, when the caller has one. Passing it
    // through means the agents run on the model that just proved it works,
    // instead of re-resolving to a different configured provider.
    provider: Option<String>,
    model: Option<String>,
) -> Result<ProvisionOutcome, String> {
    if roles.is_empty() {
        return Err("No roles to provision".to_string());
    }
    if roles.iter().filter(|r| r.is_lead).count() > 1 {
        return Err("Only one lead (CEO) role is allowed".to_string());
    }

    let all_bots = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let existing_members =
        ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
            .await
            .map_err(|e| e.to_string())?;
    let mut used: std::collections::HashSet<Uuid> =
        existing_members.iter().map(|m| m.bot_id).collect();

    // ── The office half of every brief ──
    //
    // The template knows what a Coder owns; only the room knows where the
    // office works, who else is on the team, and what the rules are. Compose
    // both together once, here, so every agent is provisioned with a brief
    // that names its own folder instead of one that only names a job.
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .ok()
        .flatten();
    let office = {
        let mut ctx = office_org::OfficeContext::new(
            room.as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "Office".into()),
        )
        .with_goal(room.as_ref().and_then(|r| r.goal.clone()))
        .with_policy(room.as_ref().and_then(|r| r.policy.clone()))
        .with_workspace(
            room.as_ref()
                .and_then(|r| r.project_folders.first().cloned()),
        );
        ctx = ctx.with_roster(
            roles
                .iter()
                .map(|r| {
                    office_org::Teammate::new(
                        r.name.clone(),
                        r.rank.clone(),
                        r.specialty.clone(),
                        r.is_lead,
                    )
                })
                .collect(),
        );
        ctx
    };

    let (default_provider, default_model) = match (
        provider
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
        model
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty()),
    ) {
        (Some(p), Some(m)) => (p, m),
        _ => resolve_default_model(state.db.pool()).await,
    };
    let mut created = Vec::new();
    let mut reused = Vec::new();

    for raw_role in roles {
        // Common skills and the office context are attached before the brief is
        // written, so the stored prompt matches what the agent can actually do.
        let role = office_org::with_common_skills(raw_role);
        let brief = office_org::compose_system_prompt(&office, &role);

        let bot = if let Some(mut existing) = find_reusable_agent(&all_bots, &used, &role) {
            // Fill only what's missing — never clobber the owner's settings.
            let mut changed = false;
            // A provider with no key at all is not a setting worth preserving:
            // the agent cannot answer, and reusing it would import the failure
            // into a brand-new office. Local providers need no key, so they are
            // left alone.
            let provider_has_key = {
                let manager = state.runtime.provider_manager().lock().await;
                manager.has_key(&existing.config.model_provider)
                    || existing
                        .config
                        .model_provider
                        .eq_ignore_ascii_case("ollama")
                    || existing.config.model_provider.eq_ignore_ascii_case("local")
            };
            if existing.config.model_provider.trim().is_empty() || !provider_has_key {
                existing.config.model_provider = default_provider.clone();
                existing.config.model_id = default_model.clone();
                changed = true;
            }
            // Only a blank or persona-sized prompt is replaced. An agent whose
            // prompt someone has since written out is left alone, even if it
            // names the wrong office.
            let needs_brief = existing
                .config
                .custom_prompt
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| !office_org::mandate_is_substantive(p))
                .unwrap_or(true);
            if needs_brief && office_org::mandate_is_substantive(&brief) {
                existing.config.custom_prompt = Some(brief.clone());
                changed = true;
            }
            if !role.skills.is_empty() {
                // Add what the role needs; never remove a tool the owner gave
                // this agent. A reused Coder with a stale two-tool list used to
                // stay unable to write files because the list was "not empty".
                let missing: Vec<String> = role
                    .skills
                    .iter()
                    .filter(|s| !existing.skills.iter().any(|have| have == *s))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    existing.skills.extend(missing);
                    changed = true;
                }
            }
            if role.is_lead && !existing.is_orchestrator {
                existing.is_orchestrator = true;
                changed = true;
            }
            // Office work is autonomous. The hard denylist in
            // `ravenbot_skills::safety` blocks machine-destructive commands;
            // approvals are not the boundary here.
            if !matches!(existing.approval_mode, ravenbot_core::ApprovalMode::Full) {
                existing.approval_mode = ravenbot_core::ApprovalMode::Full;
                changed = true;
            }
            if changed {
                ravenbot_db::queries::BotQueries::update(state.db.pool(), &existing)
                    .await
                    .map_err(|e| e.to_string())?;
            }

            // Connectors the designer chose for this role. For a reused agent
            // this must be additive in practice, not just in intent: a bot with
            // no explicit rows inherits every globally enabled connector, so
            // writing only the role's list would silently take the others away.
            // Materialise the effective set first, then add.
            if !role.mcp_servers.is_empty() {
                let registry = state.runtime.mcp_registry();
                match registry.enabled_server_ids(existing.id).await {
                    Ok(mut effective) => {
                        let before = effective.len();
                        for server_id in &role.mcp_servers {
                            if !effective.contains(server_id) {
                                effective.push(server_id.clone());
                            }
                        }
                        if effective.len() != before {
                            if let Err(e) =
                                registry.batch_set_bot_servers(existing.id, effective).await
                            {
                                tracing::warn!(
                                    bot = %existing.name,
                                    error = %e,
                                    "Could not assign connectors to reused office agent"
                                );
                            }
                        }
                    }
                    Err(e) => tracing::warn!(
                        bot = %existing.name,
                        error = %e,
                        "Could not read the reused agent's connectors"
                    ),
                }
            }
            for plugin_id in &role.plugins {
                if let Err(e) = state
                    .runtime
                    .plugin_registry()
                    .store
                    .set_bot_plugin(existing.id, plugin_id, true)
                    .await
                {
                    tracing::warn!(
                        bot = %existing.name,
                        plugin = %plugin_id,
                        error = %e,
                        "Could not assign plugin to reused office agent"
                    );
                }
            }

            reused.push(existing.clone());
            existing
        } else {
            let mut bot = Bot::new(role.name.clone(), role.specialty.clone());
            bot.rank = Some(role.rank.clone());
            bot.specialty = Some(role.specialty.clone());
            bot.is_orchestrator = role.is_lead;
            bot.approval_mode = ravenbot_core::ApprovalMode::Full;
            bot.config.model_provider = default_provider.clone();
            bot.config.model_id = default_model.clone();
            if role.is_lead {
                bot.avatar_color = "#8b5cf6".to_string();
            }
            bot.config.custom_prompt = Some(brief.clone());
            if !role.skills.is_empty() {
                bot.skills = role.skills.clone();
            }
            if let Some(style) = role
                .avatar_style
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                bot.avatar_url = Some(Bot::dicebear_url(&role.name, style));
                bot.avatar_style = Some(style.to_string());
            }
            ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
                .await
                .map_err(|e| e.to_string())?;

            // A non-empty list becomes this agent's connector set. An empty
            // list is left alone so the agent inherits the globally enabled
            // connectors, which is the useful default for a new office.
            if !role.mcp_servers.is_empty() {
                if let Err(e) = state
                    .runtime
                    .mcp_registry()
                    .batch_set_bot_servers(bot.id, role.mcp_servers.clone())
                    .await
                {
                    tracing::warn!(
                        bot = %bot.name,
                        error = %e,
                        "Could not assign connectors to new office agent"
                    );
                }
            }
            for plugin_id in &role.plugins {
                if let Err(e) = state
                    .runtime
                    .plugin_registry()
                    .store
                    .set_bot_plugin(bot.id, plugin_id, true)
                    .await
                {
                    tracing::warn!(
                        bot = %bot.name,
                        plugin = %plugin_id,
                        error = %e,
                        "Could not assign plugin to new office agent"
                    );
                }
            }

            created.push(bot.clone());
            bot
        };

        used.insert(bot.id);
        let member = ChatRoomMember {
            chatroom_id,
            bot_id: bot.id,
            rank: role.rank,
            specialty: role.specialty,
            joined_at: chrono::Utc::now(),
        };
        ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Rewrite OFFICE.md now that the roster exists. The charter was seeded at
    // room creation with an empty team; an agent that reads it first would
    // otherwise be told it has no colleagues and no handoff targets.
    refresh_office_workspace(&state, chatroom_id).await;

    Ok(ProvisionOutcome { created, reused })
}

/// Build the capability list the office designer is allowed to assign.
///
/// This is read from the live registries, not from a static list: if the user
/// installs a plugin, enables a connector, or a new built-in skill ships, the
/// next office design sees it. Unusable connectors (unverified launcher,
/// disabled, or missing credentials) are left out so a role is never assigned
/// a tool that cannot start.
async fn office_capability_catalog(
    runtime: &ravenbot_runtime::Runtime,
) -> office_org::CapabilityCatalog {
    let mut skills: Vec<office_org::Capability> = runtime
        .skill_registry()
        .list()
        .into_iter()
        .map(|skill| office_org::Capability {
            id: skill.id().to_string(),
            name: skill.name().to_string(),
            group: match skill.kind() {
                ravenbot_skills::SkillKind::Tool => "tool".to_string(),
                ravenbot_skills::SkillKind::Workflow => "workflow".to_string(),
            },
            description: skill.description().to_string(),
            extra: String::new(),
        })
        .collect();
    skills.sort_by(|a, b| a.group.cmp(&b.group).then_with(|| a.name.cmp(&b.name)));
    skills.dedup_by(|a, b| a.id == b.id);

    let mut connectors: Vec<office_org::Capability> = runtime
        .mcp_registry()
        .list_server_summaries(None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            // A connector is usable when it is verified, enabled, and its
            // credentials exist either in the app's connector store or in the
            // process environment. The summary only knows the store; checking
            // the OS env here keeps a shell-exported key from disappearing
            // from the designer's catalog.
            let os_configured = !s.env_keys.is_empty()
                && s.env_keys.iter().all(|k| {
                    std::env::var(k)
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false)
                });
            s.verified
                && (s.enabled || s.is_custom)
                && (s.env_configured || s.env_keys.is_empty() || os_configured)
        })
        .map(|s| office_org::Capability {
            id: s.id,
            name: s.name,
            group: s.category,
            description: s.description,
            extra: if s.tools_count > 0 {
                format!("{} tools", s.tools_count)
            } else {
                String::new()
            },
        })
        .collect();
    connectors.sort_by(|a, b| a.name.cmp(&b.name));
    connectors.truncate(60);

    let mut plugins: Vec<office_org::Capability> = runtime
        .plugin_registry()
        .store
        .list_summaries(None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|plugin| plugin.operations > 0)
        .map(|plugin| office_org::Capability {
            id: plugin.id,
            name: plugin.name,
            group: "plugin".to_string(),
            description: plugin.description,
            extra: format!("{} operations", plugin.operations),
        })
        .collect();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins.truncate(24);

    office_org::CapabilityCatalog {
        skills,
        connectors,
        plugins,
    }
}

/// Design an entire office — charter and team — from a name and whatever the
/// user happened to type.
///
/// The create-office wizard used to require a template and send the static
/// blueprint, so a user who typed "Security lab" and nothing else got the
/// engineering template whether it fit or not. This asks the configured default
/// model to infer the mission, write the binding rules, and propose a roster
/// with real mandates. Template data is the fallback at every step, so the
/// office is always created even when the model is unreachable.
#[tauri::command]
async fn draft_office_blueprint(
    state: State<'_, AppState>,
    name: String,
    mission: Option<String>,
    office_template: String,
    brief: Option<String>,
) -> Result<serde_json::Value, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("An office needs a name".to_string());
    }
    let mission = mission.unwrap_or_default().trim().to_string();
    let brief = brief.unwrap_or_default().trim().to_string();
    let template = office_template.trim().to_string();

    tracing::info!(
        office = %name,
        template = %template,
        "Office blueprint: request received"
    );

    let fallback_goal = if mission.is_empty() {
        match template.as_str() {
            "marketing" => format!(
                "Make {name} findable and chosen: a positioned offer, a channel plan, and campaign assets that can ship."
            ),
            "sales" => format!(
                "Build a qualified pipeline for {name}: named accounts, a reason to reach out now, and outreach that can send."
            ),
            "design" => format!(
                "Deliver a coherent product experience for {name}: a design system and working screens, not descriptions of them."
            ),
            "rot-archive" => format!(
                "Answer {name}'s research questions with cited, verified findings and an explicit list of what remains unknown."
            ),
            _ => format!(
                "Ship {name}'s work end to end: a plan, a working implementation, and verification a reviewer can rerun."
            ),
        }
    } else {
        mission.clone()
    };
    let fallback_policy = office_org::template_policy(&template).to_string();
    let fallback_roles = office_org::default_org(&template);

    // The agent that designs offices, and the actual capabilities it may assign.
    // Both are resolved before any model call so the prompt can name the
    // designer and enumerate only tools that exist on this machine.
    let default_agent = find_default_agent(state.db.pool()).await;
    let designer_name = default_agent
        .as_ref()
        .map(|b| b.name.clone())
        .unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string());
    let designer_id = default_agent.as_ref().map(|b| b.id.to_string());
    let designer_persona = default_agent
        .as_ref()
        .and_then(|b| b.config.custom_prompt.clone())
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_AGENT_PROMPT.to_string());
    let catalog = {
        let mut catalog = match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            office_capability_catalog(&state.runtime),
        )
        .await
        {
            Ok(catalog) => catalog,
            Err(_) => {
                tracing::warn!(
                    office = %name,
                    "Office blueprint: capability catalog timed out; using the static fallback"
                );
                office_org::CapabilityCatalog::builtin_default()
            }
        };
        if catalog.is_empty() {
            catalog = office_org::CapabilityCatalog::builtin_default();
        }
        catalog
    };
    tracing::info!(
        office = %name,
        skills = catalog.skills.len(),
        connectors = catalog.connectors.len(),
        plugins = catalog.plugins.len(),
        designer = %designer_name,
        "Office blueprint: capability catalog ready"
    );

    // Try the office architect first, then the other configured providers. A
    // key being present is not the same as a key working: the most recently
    // saved one can be a free tier that is already exhausted, and failing the
    // whole office design over that while a working subscription sits in the
    // next row is the wrong outcome. Only permanent failures fall through, so a
    // transient blip still retries on the same provider.
    let (preferred_provider, preferred_model) = match &default_agent {
        Some(b)
            if !b.config.model_provider.trim().is_empty()
                && !b.config.model_id.trim().is_empty() =>
        {
            (b.config.model_provider.clone(), b.config.model_id.clone())
        }
        _ => {
            let (p, m, _) = resolve_default_model_with_source(state.db.pool()).await;
            (p, m)
        }
    };
    let provider_default = default_model_for_provider(&preferred_provider);
    let mut candidates: Vec<(String, String)> = Vec::new();
    // Design is an auxiliary task: try the provider's fast default first, even
    // when the office architect itself runs on a slow reasoning model. Beam
    // Free, for example, needed 205 seconds for this prompt while DeepSeek V4.1
    // Flash answered in 37. The architect's own model is the next candidate, so
    // a custom choice is still honored when the fast path fails.
    if provider_default != "llama3.1" && !preferred_model.eq_ignore_ascii_case(provider_default) {
        candidates.push((preferred_provider.clone(), provider_default.to_string()));
    }
    candidates.push((preferred_provider.clone(), preferred_model.clone()));
    // OpenCode Go's paid default can 402 while Beam Free on the same key still
    // answers. Trying the free model before giving up on the provider is the
    // difference between a designed office and a template fallback on an
    // account that has a working free tier but no balance.
    if preferred_provider.eq_ignore_ascii_case("opencode-go")
        && !preferred_model.eq_ignore_ascii_case("beam-free")
    {
        candidates.push(("opencode-go".to_string(), "beam-free".to_string()));
    }
    let custom_models: std::collections::HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, default_model FROM custom_providers")
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, model)| (id.to_ascii_lowercase(), model))
            .collect();
    let mut has_go_key = false;
    if let Ok(rows) = sqlx::query_as::<_, (String,)>(
        "SELECT provider FROM provider_keys
         WHERE TRIM(api_key) <> ''
         ORDER BY updated_at DESC",
    )
    .fetch_all(state.db.pool())
    .await
    {
        for (p,) in rows {
            let p = p.trim().to_ascii_lowercase();
            if p.is_empty() {
                continue;
            }
            if p == "opencode-go" {
                has_go_key = true;
            }
            if candidates
                .iter()
                .any(|(existing, _)| existing.eq_ignore_ascii_case(&p))
            {
                continue;
            }
            let m = custom_models
                .get(&p)
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| default_model_for_provider(&p).to_string());
            candidates.push((p, m));
        }
    }
    candidates.truncate(4);
    // Last resort: the free Go model. Paid Go models can 402 while Beam Free
    // on the same key still answers, and an office designed on it is better
    // than a template fallback. Appended after the truncate so a long provider
    // list cannot push it out.
    if has_go_key
        && !candidates.iter().any(|(p, m)| {
            p.eq_ignore_ascii_case("opencode-go") && m.eq_ignore_ascii_case("beam-free")
        })
    {
        candidates.push(("opencode-go".to_string(), "beam-free".to_string()));
    }

    let mut last_error: Option<String> = None;
    for (provider, model) in candidates {
        let prompt = office_org::blueprint_prompt_with_catalog(
            &name,
            Some(&mission),
            &template,
            &brief,
            &catalog,
        );
        let system = format!(
            "{designer_persona}\n\nYou are {designer_name}. Output only valid JSON, with \
             no prose before or after it."
        );
        tracing::info!(
            office = %name,
            provider = %provider,
            model = %model,
            "Office blueprint: asking the model"
        );
        match tokio::time::timeout(
            std::time::Duration::from_secs(75),
            state
                .runtime
                .complete_with_model(&provider, &model, &system, &prompt, 6000),
        )
        .await
        {
            Err(_) => {
                last_error = Some(format!("{provider}/{model} timed out after 75 seconds"));
                tracing::warn!(
                    office = %name,
                    provider = %provider,
                    model = %model,
                    "Office blueprint: model call timed out; trying the next provider"
                );
            }
            Ok(Err(e)) => {
                last_error = Some(e.to_string());
                tracing::warn!(office = %name, provider = %provider, error = %e, "Provider could not design the office; trying the next configured provider");
            }
            Ok(Ok(text)) => {
                tracing::info!(
                    office = %name,
                    provider = %provider,
                    model = %model,
                    chars = text.len(),
                    "Office blueprint: model answered"
                );
                match office_org::parse_blueprint_with_catalog(&text, Some(&catalog)) {
                    Some(bp) => {
                        let usable = office_org::blueprint_roles_are_usable(&bp.roles);
                        let roles = if usable {
                            bp.roles
                        } else {
                            tracing::warn!(
                                office = %name,
                                provider = %provider,
                                "CEO blueprint had no substantive roster; using the template team"
                            );
                            office_org::default_org(&template)
                        };
                        return Ok(serde_json::json!({
                            "goal": if bp.goal.is_empty() { fallback_goal.clone() } else { bp.goal },
                            "policy": if bp.policy.is_empty() { fallback_policy.clone() } else { bp.policy },
                            "roles": roles,
                            "question": bp.question,
                            "source": if usable { "ai" } else { "template" },
                            "provider": provider,
                            "model": model,
                            "designer": designer_name.clone(),
                            "designer_id": designer_id.clone(),
                            "capabilities": {
                                "skills": catalog.skills.len(),
                                "connectors": catalog.connectors.len(),
                                "plugins": catalog.plugins.len(),
                            },
                        }));
                    }
                    None => {
                        last_error = Some(format!(
                            "{provider}/{model} returned text that was not a usable blueprint"
                        ));
                        tracing::warn!(office = %name, provider = %provider, "Blueprint was not parseable; trying the next provider");
                    }
                }
            }
        }
    }

    tracing::warn!(office = %name, "Every configured provider failed to design the office; using the template");
    Ok(serde_json::json!({
        "goal": fallback_goal,
        "policy": fallback_policy,
        "roles": fallback_roles,
        "question": null,
        "source": "template",
        "error": last_error.unwrap_or_else(|| "No configured provider answered".to_string()),
        "provider": null,
        "model": null,
        "designer": designer_name,
        "designer_id": designer_id,
        "capabilities": {
            "skills": catalog.skills.len(),
            "connectors": catalog.connectors.len(),
            "plugins": catalog.plugins.len(),
        },
    }))
}

/// Ask the office's CEO (or any available bot when the office is empty) to
/// propose the team needed for a brief. Falls back to the built-in blueprint
/// when no model is configured or the model output is unusable.
#[tauri::command]
async fn draft_office_org(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    brief: String,
) -> Result<serde_json::Value, String> {
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Office not found")?;
    let members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;

    // The CEO sees the whole fleet, not just this office, so it can reuse
    // agents that already exist anywhere in the app instead of duplicating them.
    let fleet = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .unwrap_or_default();
    let member_ids: std::collections::HashSet<Uuid> = members.iter().map(|m| m.bot_id).collect();
    let mut existing: Vec<(String, String)> = fleet
        .iter()
        .map(|b| {
            let rank = b.rank.clone().unwrap_or_default();
            let label = if member_ids.contains(&b.id) {
                format!("{rank} · in this office")
            } else {
                format!("{rank} · available in fleet")
            };
            (b.name.clone(), label)
        })
        .collect();
    if existing.is_empty() {
        existing.push(("(none — empty fleet)".to_string(), String::new()));
    }

    // Pick the model to think with: the office lead, else any orchestrator,
    // else any bot at all.
    let mut thinker: Option<Bot> = None;
    for m in &members {
        if let Ok(Some(bot)) =
            ravenbot_db::queries::BotQueries::get(state.db.pool(), m.bot_id).await
        {
            if bot.is_orchestrator {
                thinker = Some(bot);
                break;
            }
            if thinker.is_none() {
                thinker = Some(bot);
            }
        }
    }
    if thinker.is_none() {
        if let Ok(bots) = ravenbot_db::queries::BotQueries::list(state.db.pool()).await {
            thinker = bots.into_iter().find(|b| b.is_orchestrator).or(None);
            if thinker.is_none() {
                // Any bot can act as the hiring manager if no orchestrator exists.
                if let Ok(bots) = ravenbot_db::queries::BotQueries::list(state.db.pool()).await {
                    thinker = bots.into_iter().next();
                }
            }
        }
    }

    let fallback = || {
        serde_json::json!({
            "roles": office_org::default_org(&room.office_template),
            "question": serde_json::Value::Null,
            "source": "template",
        })
    };

    let Some(thinker) = thinker else {
        return Ok(fallback());
    };

    let prompt = office_org::org_prompt(&room.name, room.goal.as_deref(), &brief, &existing);
    match state
        .runtime
        .complete_as_bot(
            thinker.id,
            "You are the CEO of an AI office. Output only valid JSON, with no \
             prose before or after it.",
            &prompt,
            // Room for a real brief. 1600 tokens truncated the roster to two or
            // three roles, because each one now carries an artifact, a way to
            // prove the work, and a handoff. 6000 fits a 7-role office with
            // room to spare and is still a bounded, non-streaming call.
            6000,
        )
        .await
    {
        Ok(text) => match office_org::parse_org(&text) {
            Some(org) => Ok(serde_json::json!({
                "roles": org.roles,
                "question": org.question,
                "source": "ceo",
            })),
            None => Ok(fallback()),
        },
        Err(e) => {
            tracing::warn!(error = %e, "CEO org proposal failed; using template blueprint");
            Ok(fallback())
        }
    }
}

#[tauri::command]
async fn get_chatroom_thread(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Option<Uuid>, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string())
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.and_then(|r| Uuid::parse_str(&r.0).ok()))
}
#[tauri::command]
async fn add_office_memory(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    content: String,
    category: String,
    created_by: Option<Uuid>,
) -> Result<OfficeMemory, String> {
    state
        .runtime
        .office_memory()
        .add(chatroom_id, &content, &category, created_by, 0.7)
        .await
}
#[tauri::command]
async fn list_office_memories(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Vec<OfficeMemory>, String> {
    state.runtime.office_memory().list(chatroom_id).await
}
#[tauri::command]
async fn search_office_memories(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    query: String,
) -> Result<Vec<(OfficeMemory, f32)>, String> {
    state
        .runtime
        .office_memory()
        .retrieve(chatroom_id, &query, 10, 0.3)
        .await
}
#[tauri::command]
async fn get_agent_intelligence(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<AgentIntelligence, String> {
    state.runtime.learning().get_intelligence(bot_id).await
}
#[tauri::command]
async fn list_agent_learnings(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<AgentLearning>, String> {
    state.runtime.learning().list_learnings(bot_id, 20).await
}

/// A room's working roster: the member rows, their loaded bots, the
/// orchestration view, and the resolved lead. Shared by the natural-language
/// office flow and Plan mode so both attribute work identically.
struct OfficeRoster {
    bots: Vec<(ravenbot_core::ChatRoomMember, Bot)>,
    members: Vec<ravenbot_runtime::orchestrator::OfficeMember>,
    lead_id: Uuid,
    lead_name: String,
}

async fn load_office_roster(
    pool: &sqlx::SqlitePool,
    chatroom_id: Uuid,
) -> Result<OfficeRoster, String> {
    let rows = ravenbot_db::queries::ChatRoomQueries::list_members(pool, chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err("No bots in this office".to_string());
    }
    let mut bots = Vec::new();
    for m in rows {
        if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, m.bot_id).await {
            bots.push((m, bot));
        }
    }
    if bots.is_empty() {
        return Err("No valid bots in this office".to_string());
    }
    let members: Vec<ravenbot_runtime::orchestrator::OfficeMember> = bots
        .iter()
        .map(|(m, bot)| ravenbot_runtime::orchestrator::OfficeMember {
            bot_id: bot.id,
            name: bot.name.clone(),
            rank: m.rank.clone(),
            specialty: m.specialty.clone(),
        })
        .collect();
    let lead_id = bots
        .iter()
        .find(|(_, bot)| bot.is_orchestrator)
        .map(|(_, bot)| bot.id)
        .or_else(|| {
            bots.iter()
                .find(|(m, _)| {
                    let r = m.rank.to_lowercase();
                    r.contains("lead") || r.contains("chief") || r.contains("manager")
                })
                .map(|(_, bot)| bot.id)
        })
        .unwrap_or(members[0].bot_id);
    let lead_name = bots
        .iter()
        .find(|(_, bot)| bot.id == lead_id)
        .map(|(_, bot)| bot.name.clone())
        .unwrap_or_else(|| "Team Lead".to_string());
    Ok(OfficeRoster {
        bots,
        members,
        lead_id,
        lead_name,
    })
}

/// Resolve a bot's display name within the roster.
fn roster_name(roster: &OfficeRoster, bot_id: Uuid) -> String {
    roster
        .bots
        .iter()
        .find(|(_, bot)| bot.id == bot_id)
        .map(|(_, bot)| bot.name.clone())
        .unwrap_or_else(|| "Teammate".to_string())
}

/// Get (or create) the shared group thread for a chatroom.
async fn ensure_chatroom_thread(
    pool: &sqlx::SqlitePool,
    chatroom_id: Uuid,
    first_bot: Uuid,
) -> Result<Uuid, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(r) = row {
        if let Ok(id) = Uuid::parse_str(&r.0) {
            return Ok(id);
        }
    }
    let room = ravenbot_db::queries::ChatRoomQueries::get(pool, chatroom_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Chatroom not found")?;
    let thread = Thread::new(first_bot, format!("{} — group", room.name));
    ravenbot_db::queries::ThreadQueries::create(pool, &thread)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT OR REPLACE INTO chatroom_threads (chatroom_id, thread_id) VALUES (?, ?)")
        .bind(chatroom_id.to_string())
        .bind(thread.id.to_string())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(thread.id)
}

/// Post an assistant turn attributed to a specific bot (office conversation).
async fn post_bot_message(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
    bot_id: Uuid,
    name: String,
    text: String,
) -> Result<(), String> {
    let msg = Message::assistant_from_bot(thread_id, bot_id, name, text);
    ravenbot_db::queries::MessageQueries::insert(pool, &msg)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Build the planner input from the room transcript plus the latest request, so
/// a follow-up (e.g. an answer to the CEO's clarifying question) is understood
/// in context instead of being planned in isolation.
async fn build_planning_request(pool: &sqlx::SqlitePool, thread_id: Uuid, content: &str) -> String {
    let history = ravenbot_db::queries::MessageQueries::list_by_thread(pool, thread_id)
        .await
        .unwrap_or_default();
    let mut transcript: Vec<String> = Vec::new();
    for msg in history
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let text = match &msg.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.trim().to_string(),
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        if matches!(msg.role, ravenbot_core::MessageRole::User) {
            transcript.push(format!("Client: {}", truncate_chars(&text, 600)));
        } else {
            let who = msg
                .sender_name
                .clone()
                .unwrap_or_else(|| "Team".to_string());
            transcript.push(format!("{}: {}", who, truncate_chars(&text, 600)));
        }
    }
    if transcript.len() > 1 {
        format!(
            "Conversation so far:\n{}\n\nLatest client request:\n{}",
            transcript.join("\n"),
            content
        )
    } else {
        content.to_string()
    }
}

/// The task portion of a node instruction, for compact briefings. Node
/// instructions are self-contained (`Goal: …\n\nYour task: …`); the briefing
/// should not repeat the goal on every line.
fn briefing_label(instruction: &str) -> String {
    instruction
        .rsplit("\n\nYour task: ")
        .next()
        .unwrap_or(instruction)
        .trim()
        .to_string()
}

/// Render a lead's task briefing: one short line per assignment.
///
/// The node receives the full instruction; the room needs the shape of the
/// plan. Dumping the raw prompt (with its READ FIRST list, code fences and
/// policy paragraphs) into a numbered list is what produced the unreadable
/// wall of text in the office feed. The briefing is flattened and capped, and
/// the dependency is a suffix rather than another paragraph.
fn briefing_markdown(items: &[(String, String, Vec<usize>)]) -> String {
    let mut lines = vec!["📋 **Briefing** — the lead has split the work:".to_string()];
    for (i, (who, instruction, deps)) in items.iter().enumerate() {
        let label = briefing_label(instruction);
        let flat = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let flat = if flat.chars().count() > 240 {
            let mut short: String = flat.chars().take(240).collect();
            short.push('…');
            short
        } else {
            flat
        };
        let dep = if deps.is_empty() {
            String::new()
        } else {
            format!(
                " _(after {})_",
                deps.iter()
                    .map(|d| (d + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        lines.push(format!("{}. **{}** — {}{}", i + 1, who, flat, dep));
    }
    lines.join("\n")
}

// ── Per-office message queue ────────────────────────────────────────────────
//
// A second message sent while an office is working used to be rejected. That
// is safe but loses the instruction the user just typed. The queue is the
// middle ground: persist it, show it, and start it as soon as the active turn
// releases the office. A queued item becomes a visible user message only when
// its turn starts, so cancelled items leave no orphan prompt in the transcript.

#[derive(Debug, Clone)]
struct OfficeQueueItem {
    id: Uuid,
    content: String,
    visible: String,
    attachments: Vec<AttachmentInput>,
    reply_to_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct OfficeQueueRow {
    id: String,
    content: String,
    visible: String,
    attachments: String,
    reply_to_id: Option<String>,
    state: String,
    error: Option<String>,
    created_at: String,
    started_at: Option<String>,
}

impl OfficeQueueRow {
    fn into_item(self) -> OfficeQueueItem {
        OfficeQueueItem {
            id: Uuid::parse_str(&self.id).unwrap_or_default(),
            content: self.content,
            visible: self.visible,
            attachments: serde_json::from_str::<Vec<AttachmentInput>>(&self.attachments)
                .unwrap_or_default(),
            reply_to_id: self
                .reply_to_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct OfficeQueueView {
    id: String,
    content: String,
    visible: String,
    attachment_names: Vec<String>,
    state: String,
    error: Option<String>,
    created_at: String,
    started_at: Option<String>,
}

fn office_queue_paused_key(chatroom_id: Uuid) -> String {
    format!("office_queue_paused:{}", chatroom_id)
}

async fn office_queue_paused(db: &ravenbot_db::Database, chatroom_id: Uuid) -> bool {
    ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &office_queue_paused_key(chatroom_id))
        .await
        .ok()
        .flatten()
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

async fn persist_office_queue_paused(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
    paused: bool,
) -> Result<(), String> {
    ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_queue_paused_key(chatroom_id),
        if paused { "1" } else { "0" },
    )
    .await
    .map_err(|e| e.to_string())
}

async fn enqueue_office_message(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
    content: String,
    visible: String,
    attachments: Vec<AttachmentInput>,
    reply_to_id: Option<Uuid>,
) -> Result<OfficeQueueItem, String> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let attachments_json = serde_json::to_string(&attachments).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO office_queue (id, chatroom_id, content, visible, attachments, reply_to_id, state, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(id.to_string())
    .bind(chatroom_id.to_string())
    .bind(&content)
    .bind(&visible)
    .bind(&attachments_json)
    .bind(reply_to_id.map(|id| id.to_string()))
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(OfficeQueueItem {
        id,
        content,
        visible,
        attachments,
        reply_to_id,
    })
}

async fn office_queue_pending_count(db: &ravenbot_db::Database, chatroom_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM office_queue WHERE chatroom_id = ? AND state = 'pending'",
    )
    .bind(chatroom_id.to_string())
    .fetch_one(db.pool())
    .await
    .unwrap_or(0)
}

async fn office_queue_has_pending(db: &ravenbot_db::Database, chatroom_id: Uuid) -> bool {
    office_queue_pending_count(db, chatroom_id).await > 0
}

async fn claim_next_office_queue_item(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
) -> Result<Option<OfficeQueueItem>, String> {
    let mut tx = db.pool().begin().await.map_err(|e| e.to_string())?;
    let row: Option<OfficeQueueRow> = sqlx::query_as(
        "SELECT id, content, visible, attachments, reply_to_id, state, error,
                created_at, started_at
         FROM office_queue
         WHERE chatroom_id = ? AND state = 'pending'
         ORDER BY created_at ASC, rowid ASC
         LIMIT 1",
    )
    .bind(chatroom_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let Some(row) = row else {
        let _ = tx.rollback().await;
        return Ok(None);
    };
    let now = chrono::Utc::now().to_rfc3339();
    let updated = sqlx::query(
        "UPDATE office_queue SET state = 'running', started_at = ?
         WHERE id = ? AND state = 'pending'",
    )
    .bind(&now)
    .bind(&row.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if updated.rows_affected() == 0 {
        let _ = tx.rollback().await;
        return Ok(None);
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(Some(row.into_item()))
}

async fn finish_office_queue_item(
    db: &ravenbot_db::Database,
    item_id: Uuid,
    success: bool,
    error: Option<String>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "UPDATE office_queue SET state = ?, error = ?, finished_at = ?
         WHERE id = ? AND state = 'running'",
    )
    .bind(if success { "done" } else { "failed" })
    .bind(error)
    .bind(now)
    .bind(item_id.to_string())
    .execute(db.pool())
    .await;
}

async fn reset_office_queue_item(db: &ravenbot_db::Database, item_id: Uuid) {
    let _ = sqlx::query(
        "UPDATE office_queue SET state = 'pending', started_at = NULL
         WHERE id = ? AND state = 'running'",
    )
    .bind(item_id.to_string())
    .execute(db.pool())
    .await;
}

async fn cancel_queued_message(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
    item_id: Uuid,
) -> Result<(), String> {
    let updated = sqlx::query(
        "UPDATE office_queue SET state = 'cancelled', finished_at = ?
         WHERE id = ? AND chatroom_id = ? AND state = 'pending'",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(item_id.to_string())
    .bind(chatroom_id.to_string())
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    if updated.rows_affected() == 0 {
        return Err("Only a pending message can be cancelled.".to_string());
    }
    Ok(())
}

async fn dismiss_queued_message(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
    item_id: Uuid,
) -> Result<(), String> {
    let deleted = sqlx::query(
        "DELETE FROM office_queue
         WHERE id = ? AND chatroom_id = ? AND state IN ('failed', 'cancelled', 'done')",
    )
    .bind(item_id.to_string())
    .bind(chatroom_id.to_string())
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    if deleted.rows_affected() == 0 {
        return Err("Only a finished message can be removed from the queue.".to_string());
    }
    Ok(())
}

async fn load_office_queue(
    db: &ravenbot_db::Database,
    chatroom_id: Uuid,
) -> Result<Vec<OfficeQueueView>, String> {
    let rows: Vec<OfficeQueueRow> = sqlx::query_as(
        "SELECT id, content, visible, attachments, reply_to_id, state, error,
                created_at, started_at
         FROM office_queue
         WHERE chatroom_id = ? AND state IN ('pending', 'running', 'failed')
         ORDER BY created_at ASC, rowid ASC
         LIMIT 50",
    )
    .bind(chatroom_id.to_string())
    .fetch_all(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let attachment_names = serde_json::from_str::<Vec<AttachmentInput>>(&row.attachments)
                .unwrap_or_default()
                .into_iter()
                .map(|attachment| attachment.name)
                .collect();
            OfficeQueueView {
                id: row.id,
                content: row.content,
                visible: row.visible,
                attachment_names,
                state: row.state,
                error: row.error,
                created_at: row.created_at,
                started_at: row.started_at,
            }
        })
        .collect())
}

/// Start the next queued user message if the office is idle and the queue is not paused.
/// Returns true when a turn was accepted.
async fn drain_office_queue(
    db: &ravenbot_db::Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    app: &tauri::AppHandle,
    chatroom_id: Uuid,
) -> bool {
    if office_queue_paused(db, chatroom_id).await || runtime.is_paused().await {
        return false;
    }
    if office_is_active(chatroom_id) {
        return false;
    }
    // A parked approval or question is the user's turn, not the queue's.
    if office_has_pending(db, chatroom_id).await {
        return false;
    }
    let item = match claim_next_office_queue_item(db, chatroom_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return false,
        Err(error) => {
            tracing::warn!(chatroom_id = %chatroom_id, error = %error, "Could not claim queued office message");
            return false;
        }
    };
    let attachments = if item.attachments.is_empty() {
        None
    } else {
        Some(item.attachments.clone())
    };
    match start_office_turn(
        db,
        runtime,
        app,
        chatroom_id,
        item.content.clone(),
        item.visible.clone(),
        attachments,
        false,
        Some(item.id),
        item.reply_to_id,
    )
    .await
    {
        Ok(_) => true,
        Err(error) => {
            if error.contains("already working") {
                reset_office_queue_item(db, item.id).await;
            } else {
                finish_office_queue_item(db, item.id, false, Some(error.clone())).await;
                tracing::warn!(
                    chatroom_id = %chatroom_id,
                    error = %error,
                    "Queued office message could not start"
                );
            }
            false
        }
    }
}

/// Start an office turn in the background.
///
/// The UI used to await this entire function — plan, every node, synthesis —
/// as a single IPC call. That made the office's progress depend on the view
/// that asked for it: a component can unmount, but a run has to keep going.
/// So the command now inserts the user turn, spawns the orchestration, and
/// returns the room thread immediately. Live progress arrives as
/// `agent-stream` events, and the board is persisted independently.
async fn start_office_turn(
    db: &ravenbot_db::Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    app: &tauri::AppHandle,
    chatroom_id: Uuid,
    content: String,
    visible: String,
    attachments: Option<Vec<AttachmentInput>>,
    autonomous: bool,
    queue_item_id: Option<Uuid>,
    reply_to_id: Option<Uuid>,
) -> Result<Uuid, String> {
    let roster = load_office_roster(db.pool(), chatroom_id).await?;
    let thread_id = ensure_chatroom_thread(db.pool(), chatroom_id, roster.bots[0].1.id).await?;
    let reply_to_id = match reply_to_id {
        Some(reply_id) if message_in_thread(db.pool(), reply_id, thread_id).await? => {
            Some(reply_id)
        }
        _ => None,
    };
    let lead_id = roster.lead_id;
    let lead_name = roster.lead_name.clone();

    // One turn per office at a time. The claim is atomic, so a user message
    // and a heartbeat racing for the same idle office cannot both start; the
    // second run's `done` would otherwise clear the first run's active flag and
    // unlock the composer while its own turn is still running. Rejecting is
    // honest and keeps one turn's events attributable to one turn.
    let Some(run_counter) = OfficeRunGuard::try_new(chatroom_id) else {
        return Err(
            "This office is already working on a turn. Wait for it to finish, or pause the office."
                .to_string(),
        );
    };

    // Establish the monthly spend baseline before this turn spends anything.
    // The heartbeat also calls this, but a manual turn can be the first one an
    // office ever runs; seeding later would leave its cost uncounted.
    if let Ok(Some(room)) = ravenbot_db::queries::ChatRoomQueries::get(db.pool(), chatroom_id).await
    {
        let member_ids: Vec<Uuid> = roster
            .bots
            .iter()
            .map(|(member, _)| member.bot_id)
            .collect();
        let _ = office_budget_spent(db, &room, &member_ids).await;
    }

    // Save the turn that started the work (with any attachments). `content` is
    // what the planner receives; `visible` is what the conversation shows.
    // They differ only for a heartbeat, whose internal instruction is not a
    // user request and would otherwise fill the room with a paragraph every
    // few minutes.
    let mut user_msg = Message::user(thread_id, &visible);
    if let Some(atts) = attachments {
        user_msg.attachments = build_attachments(atts);
    }
    user_msg.reply_to_id = reply_to_id;
    ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    let db_task = db.clone();
    let runtime_task = runtime.clone();
    let app_task = app.clone();
    mark_office_active(db, chatroom_id, true).await;
    mark_office_phase(db, chatroom_id, "planning").await;
    mark_office_last(db, chatroom_id).await;

    tauri::async_runtime::spawn(async move {
        let _run_counter = run_counter;
        let turn_result = office_turn(
            &db_task,
            &runtime_task,
            &app_task,
            chatroom_id,
            thread_id,
            content,
            roster,
            autonomous,
        )
        .await;
        match &turn_result {
            Ok(_) => {
                if let Some(item_id) = queue_item_id {
                    finish_office_queue_item(&db_task, item_id, true, None).await;
                }
            }
            Err(error) => {
                if let Some(item_id) = queue_item_id {
                    finish_office_queue_item(&db_task, item_id, false, Some(error.clone())).await;
                }
                tracing::error!(chatroom_id = %chatroom_id, error = %error, "Office turn failed");
                let _ = post_bot_message(
                    db_task.pool(),
                    thread_id,
                    lead_id,
                    lead_name,
                    format!("⚠️ **Office run failed:** {error}"),
                )
                .await;
                let _ = app_task.emit(
                    "agent-stream",
                    serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
                );
            }
        }
        mark_office_active(&db_task, chatroom_id, false).await;
        clear_office_phase(&db_task, chatroom_id).await;
        // A completed run is a natural pause point: give the office time to be
        // read (or for the user to redirect it) before the next heartbeat.
        mark_office_last(&db_task, chatroom_id).await;
        // Release the office. The queue worker starts the next pending user
        // message; doing it here would make start_office_turn recursive.
        drop(_run_counter);
    });

    Ok(thread_id)
}

/// The orchestration half of an office turn. Called from a spawned task so no
/// window, route, or component owns the run's lifetime.
async fn office_turn(
    db: &ravenbot_db::Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    app: &tauri::AppHandle,
    chatroom_id: Uuid,
    thread_id: Uuid,
    content: String,
    roster: OfficeRoster,
    autonomous: bool,
) -> Result<serde_json::Value, String> {
    // ── Orchestrate the office with a real planner ────────────────────────
    // A lead bot decomposes the request into a task DAG (who does what, in
    // what order); we execute that, then the lead synthesizes the final
    // answer. If the model cannot produce a parseable plan, use a deterministic
    // role-aware plan: the lead writes the milestone, specialists depend on it
    // and work from Shared/STATUS.md. The old fallback sent the identical
    // request to every member, which made them repeat the same exploration.
    let room = ravenbot_db::queries::ChatRoomQueries::get(db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    let goal = room.as_ref().and_then(|r| r.goal.clone());
    let policy = room.as_ref().and_then(|r| r.policy.clone());

    let planning_request = build_planning_request(db.pool(), thread_id, &content).await;
    let planning_request = format!(
        "{}\n\nWorkspace snapshot:\n{}",
        planning_request,
        office_workspace_snapshot(room.as_ref())
    );

    let mut plan = runtime
        .plan_office(
            roster.lead_id,
            goal.as_deref(),
            policy.as_deref(),
            &roster.members,
            &planning_request,
        )
        .await
        .or_else(|| {
            tracing::warn!(
                lead = %roster.lead_name,
                "Office plan unavailable; using the deterministic role plan"
            );
            let plan = ravenbot_runtime::orchestrator::fallback_plan(
                goal.as_deref(),
                &content,
                &roster.members,
                roster.lead_id,
            );
            if plan.tasks.is_empty() {
                None
            } else {
                Some(plan)
            }
        });

    // A workspace with no code cannot be advanced by another inspection plan.
    // Use a deterministic, creation-first bootstrap instead of asking the lead
    // to re-read the same design docs for the sixth time.
    let continuation = content.trim().to_ascii_lowercase();
    let generic_continue = autonomous
        || continuation.starts_with("[autonomous cycle]")
        || continuation.contains("do next work")
        || continuation == "continue"
        || continuation.contains("continue the office");
    if generic_continue && !office_has_implementation(room.as_ref()) {
        tracing::info!(
            lead = %roster.lead_name,
            "Office workspace has no implementation; using the bootstrap plan"
        );
        plan = Some(ravenbot_runtime::orchestrator::bootstrap_plan(
            goal.as_deref().unwrap_or(content.trim()),
            &roster.members,
            roster.lead_id,
        ));
        let attempts = bump_bootstrap_attempts(db, chatroom_id).await;
        tracing::warn!(
            chatroom_id = %chatroom_id,
            attempts,
            "Bootstrap plan selected because the workspace has no implementation"
        );
    }

    // An autonomous cycle cannot wait for an answer. If the planner returned
    // only a question, use the deterministic role plan and keep moving; a
    // question attached to a usable plan is simply ignored.
    if autonomous && plan.as_ref().map(|p| p.tasks.is_empty()).unwrap_or(true) {
        let fallback = ravenbot_runtime::orchestrator::fallback_plan(
            goal.as_deref(),
            &content,
            &roster.members,
            roster.lead_id,
        );
        tracing::info!(
            lead = %roster.lead_name,
            "Autonomous office has no usable plan; using the deterministic role plan"
        );
        plan = if fallback.tasks.is_empty() {
            None
        } else {
            Some(fallback)
        };
    }

    // ── CEO clarification loop ────────────────────────────────────────────
    // If the lead lacks enough information, it asks the client directly. We
    // post its question into the room and stop; the client's next message is
    // the answer, and the lead plans from there. A heartbeat never asks: no
    // one is there to answer.
    if !autonomous {
        if let Some(question) = plan
            .as_ref()
            .and_then(|p| p.question.clone())
            .filter(|q| !q.trim().is_empty())
        {
            let _ = post_bot_message(
                db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                format!(
                    "❓ I need one thing before I brief the team:\n\n{}",
                    question.trim()
                ),
            )
            .await;
            let _ = app.emit(
                "agent-stream",
                serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
            );
            return Ok(serde_json::json!({
                "thread_id": thread_id,
                "awaiting_answer": true,
                "question": question,
                "checklist": [],
                "blackboard": {},
                "summary": question,
            }));
        }
    }

    // The lead briefs the team in the room before work starts, so the client
    // sees the breakdown the moment it happens.
    if let Some(p) = &plan {
        if !p.tasks.is_empty() {
            let items: Vec<(String, String, Vec<usize>)> = p
                .tasks
                .iter()
                .map(|t| {
                    let who =
                        ravenbot_runtime::orchestrator::resolve_member(&roster.members, &t.bot)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| t.bot.clone());
                    (who, briefing_label(&t.instruction), t.depends_on.clone())
                })
                .collect();
            let _ = post_bot_message(
                db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                briefing_markdown(&items),
            )
            .await;
        }
    }

    mark_office_phase(db, chatroom_id, "executing").await;

    // Build the graph from the plan (or the fan-out fallback), keeping a
    // local node→label map so synthesis can name each contributor without
    // polluting the instructions the bots actually receive.
    let mut graph = TaskGraph::new(&content);
    let mut node_ids: Vec<uuid::Uuid> = Vec::new();
    let mut label_for_node: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    // (node_id, bot_id, board label, dependency task indices) for the live
    // planner board — emitted as `plan_ready` before execution starts.
    let mut board_nodes: Vec<(uuid::Uuid, uuid::Uuid, String, Vec<usize>)> = Vec::new();

    if let Some(plan) = &plan {
        for task in &plan.tasks {
            let assignee =
                ravenbot_runtime::orchestrator::resolve_member(&roster.members, &task.bot)
                    .map(|m| m.bot_id)
                    .unwrap_or(roster.lead_id);
            let nid = graph.add_node(assignee, task.instruction.clone());
            label_for_node.insert(nid, task.bot.clone());
            node_ids.push(nid);
            board_nodes.push((
                nid,
                assignee,
                briefing_label(&task.instruction),
                task.depends_on.clone(),
            ));
        }
        // Wire dependencies after all nodes exist.
        for (i, task) in plan.tasks.iter().enumerate() {
            for dep in &task.depends_on {
                if *dep < node_ids.len() && *dep < i {
                    graph.add_edge(node_ids[*dep], node_ids[i]);
                }
            }
        }
    } else {
        for (m, bot) in &roster.bots {
            let instruction = format!("[{} - {}] {}", m.rank, m.specialty, content);
            let nid = graph.add_node(bot.id, instruction.clone());
            label_for_node.insert(nid, bot.name.clone());
            node_ids.push(nid);
            board_nodes.push((nid, bot.id, truncate_chars(&instruction, 120), Vec::new()));
        }
    }

    let graph = Arc::new(tokio::sync::Mutex::new(graph));

    // The board is part of the run, not of the window that started it. Persist
    // every topology/state event here so leaving the office (or closing the
    // window) cannot leave a frozen "Working" card behind. Events are applied
    // in order by one task, so parallel nodes cannot lose each other's writes.
    let (board_tx, mut board_rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
    {
        let board_db = db.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = board_rx.recv().await {
                apply_office_board_event(&board_db, thread_id, &event).await;
            }
        });
    }

    // Seed the room's live board (kanban + dependency DAG) before any node
    // starts streaming, so the user sees the plan form immediately.
    {
        let nodes: Vec<serde_json::Value> = board_nodes
            .iter()
            .enumerate()
            .map(|(i, (nid, bot, label, deps))| {
                serde_json::json!({
                    "node_id": nid.to_string(),
                    "bot_id": bot.to_string(),
                    "label": label,
                    "depends_on": deps
                        .iter()
                        .filter(|d| **d < node_ids.len() && **d < i)
                        .map(|d| node_ids[*d].to_string())
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        // A heartbeat's prompt is an instruction, not the office's mission;
        // showing it in the board header would replace the goal with a
        // paragraph of housekeeping every few minutes.
        let board_goal = if autonomous {
            goal.as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    room.as_ref().and_then(|room| {
                        let description = room.description.trim();
                        (!description.is_empty()).then_some(description)
                    })
                })
                .unwrap_or_else(|| content.trim())
        } else {
            content.trim()
        };
        let payload = serde_json::json!({
            "kind": "plan_ready",
            "thread_id": thread_id.to_string(),
            "goal": truncate_chars(board_goal, 200),
            "nodes": nodes,
        });
        let _ = board_tx.send(payload.clone());
        let _ = app.emit("agent-stream", payload);
    }
    // Project folders for every node in this office: configured folders, or an
    // auto-created default named after the office.
    let office_dirs: Vec<String> = {
        let configured = room
            .as_ref()
            .map(|r| r.project_folders.clone())
            .unwrap_or_default();
        if configured.is_empty() {
            let name = room
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "office".to_string());
            vec![ravenbot_runtime::default_project_dir(&name)
                .to_string_lossy()
                .to_string()]
        } else {
            configured
        }
    };
    let node_event_app = app.clone();
    let node_event_board = board_tx.clone();
    let emitter_app = app.clone();
    let executor = ravenbot_runtime::executor::GraphExecutor::new(runtime.clone(), db.clone())
        .with_working_dirs(office_dirs)
        .with_thread_emitter(Arc::new(move |_thread_id| {
            Some(make_stream_emitter(emitter_app.clone()))
        }))
        .with_node_events(Arc::new(move |v| {
            let _ = node_event_board.send(v.clone());
            let _ = node_event_app.emit("agent-stream", v);
        }));

    let exec_result = executor.execute(graph.clone()).await;
    mark_office_phase(db, chatroom_id, "summarizing").await;
    // NOTE: `done` is emitted only after the final posts below, so the room
    // never blanks between the last token and the persisted conversation.

    match exec_result {
        Ok(blackboard) => {
            let checklist = graph.lock().await.clone().to_checklist();
            // Pull each node's real output (in dependency order), with the bot
            // that produced it, so the room shows a genuine conversation.
            let node_results: Vec<(uuid::Uuid, String, String, String)> = {
                let g = graph.lock().await;
                let mut out = Vec::new();
                for nid in g.ordered_node_ids() {
                    if let Some(node) = g.nodes.get(&nid) {
                        if let Some(result) = &node.output {
                            let label = label_for_node
                                .get(&nid)
                                .cloned()
                                .unwrap_or_else(|| "Task".to_string());
                            out.push((
                                node.bot_id,
                                label,
                                node.instruction.clone(),
                                result.clone(),
                            ));
                        }
                    }
                }
                out
            };

            // Post every specialist's work into the group thread, attributed to
            // the bot that did it (this is what makes the office "talk").
            // Pure conversation: the bubble carries only the agent's reply —
            // the row's author header/gutter identifies who and the board
            // carries what task they were given.
            for (bot_id, name, _instruction, output) in &node_results {
                let _ = post_bot_message(
                    db.pool(),
                    thread_id,
                    *bot_id,
                    name.clone(),
                    output.trim().to_string(),
                )
                .await;
            }

            // Surface failed teammates instead of failing silently: a node
            // that hit a provider error leaves no output, which used to look
            // like the task simply never ran. Skipped dependents matter too:
            // the lead must not present a blocked branch as completed work.
            let failures: Vec<String> = checklist
                .iter()
                .filter(|c| {
                    matches!(
                        c.status,
                        ravenbot_core::ChecklistStatus::Failed
                            | ravenbot_core::ChecklistStatus::Skipped
                    )
                })
                .map(|c| {
                    let who = c
                        .bot_id
                        .map(|id| roster_name(&roster, id))
                        .unwrap_or_else(|| "Teammate".to_string());
                    let detail = match c.status {
                        ravenbot_core::ChecklistStatus::Skipped => {
                            "skipped because a dependency did not complete".to_string()
                        }
                        _ => c
                            .result
                            .clone()
                            .unwrap_or_else(|| "unknown error".to_string()),
                    };
                    format!("• **{}** — {}", who, detail)
                })
                .collect();
            if !failures.is_empty() {
                let hint = error_hint(failures.first().map(|s| s.as_str()).unwrap_or(""));
                let _ = post_bot_message(
                    db.pool(),
                    thread_id,
                    roster.lead_id,
                    roster.lead_name.clone(),
                    format!(
                        "⚠️ Some teammates did not complete:\n{}\n\n{}",
                        failures.join("\n"),
                        hint
                    ),
                )
                .await;
            }

            // The lead writes the final, integrated answer (also attributed).
            let node_outputs: Vec<(String, String)> = node_results
                .iter()
                .map(|(_, label, _, output)| (label.clone(), output.clone()))
                .collect();
            // Include failures in the synthesis context. Otherwise the lead
            // reads only successful outputs and writes "all done" while a node
            // is sitting in Issues.
            let synthesis_results = if failures.is_empty() {
                node_outputs.clone()
            } else {
                let mut results = node_outputs.clone();
                results.push((
                    "Incomplete or failed tasks".to_string(),
                    failures.join("\n"),
                ));
                results
            };
            let final_summary = if node_outputs.is_empty() {
                checklist
                    .iter()
                    .map(|c| {
                        format!(
                            "{} {} → {}",
                            match c.status {
                                ravenbot_core::ChecklistStatus::Completed => "✓",
                                ravenbot_core::ChecklistStatus::Failed => "✗",
                                _ => "○",
                            },
                            c.label,
                            c.result.clone().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                runtime
                    .synthesize_office(roster.lead_id, &content, &synthesis_results)
                    .await
            };

            let _ = post_bot_message(
                db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                final_summary.clone(),
            )
            .await;
            let _ = app.emit(
                "agent-stream",
                serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
            );
            Ok(serde_json::json!({
                "thread_id": thread_id,
                "checklist": checklist,
                "blackboard": blackboard.data,
                "summary": final_summary,
                "planned": plan.is_some(),
            }))
        }
        Err(err) => {
            let err_str = err.to_string();
            tracing::warn!("Chatroom execution error: {}", err_str);
            let error_summary = format!(
                "⚠️ **Model Error:** {}\n\n{}{}",
                err_str,
                error_hint(&err_str),
                "\n\nYou can also run the office's bots fully offline via local Ollama models."
            );
            let _ = post_bot_message(
                db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                error_summary.clone(),
            )
            .await;
            let _ = app.emit(
                "agent-stream",
                serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
            );
            Ok(
                serde_json::json!({ "thread_id": thread_id, "checklist": [], "blackboard": {}, "summary": error_summary }),
            )
        }
    }
}

#[tauri::command]
async fn compact_thread_context(state: State<'_, AppState>, thread_id: Uuid) -> Result<(), String> {
    state.runtime.request_compaction(thread_id);
    Ok(())
}

#[tauri::command]
async fn get_office_queue(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Vec<OfficeQueueView>, String> {
    load_office_queue(&state.db, chatroom_id).await
}

#[tauri::command]
async fn get_office_queue_paused(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<bool, String> {
    Ok(office_queue_paused(&state.db, chatroom_id).await)
}

#[tauri::command]
async fn set_office_queue_paused(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    chatroom_id: Uuid,
    paused: bool,
) -> Result<(), String> {
    persist_office_queue_paused(&state.db, chatroom_id, paused).await?;
    if !paused {
        let db = state.db.clone();
        let runtime = state.runtime.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            drain_office_queue(&db, &runtime, &app, chatroom_id).await;
        });
    }
    Ok(())
}

#[tauri::command]
async fn cancel_office_queue_item(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    id: Uuid,
) -> Result<(), String> {
    cancel_queued_message(&state.db, chatroom_id, id).await
}

#[tauri::command]
async fn dismiss_office_queue_item(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    id: Uuid,
) -> Result<(), String> {
    dismiss_queued_message(&state.db, chatroom_id, id).await
}

#[tauri::command]
async fn send_to_chatroom(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    chatroom_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
    reply_to_id: Option<Uuid>,
) -> Result<serde_json::Value, String> {
    let visible = content.clone();
    let queue_attachments = attachments.clone().unwrap_or_default();
    if let Some(reply_id) = reply_to_id {
        let thread = chatroom_thread_id(&state.db, chatroom_id).await;
        let valid = match thread {
            Some(thread_id) => {
                message_in_thread(state.db.pool(), reply_id, thread_id).await?
            }
            None => false,
        };
        if !valid {
            return Err("The quoted message is not part of this office conversation.".to_string());
        }
    }
    let paused = office_queue_paused(&state.db, chatroom_id).await;
    let pending = office_queue_has_pending(&state.db, chatroom_id).await;
    let active = office_is_active(chatroom_id);

    // FIFO: if anything is already waiting, or the office is busy, this
    // message joins the queue rather than jumping ahead of it. A paused queue
    // accepts the message but starts nothing.
    if paused || pending || active {
        let item =
            enqueue_office_message(&state.db, chatroom_id, content, visible, queue_attachments, reply_to_id)
                .await?;
        let started = if paused || active {
            false
        } else {
            drain_office_queue(&state.db, &state.runtime, &app, chatroom_id).await
        };
        let position = office_queue_pending_count(&state.db, chatroom_id).await;
        return Ok(serde_json::json!({
            "thread_id": chatroom_thread_id(&state.db, chatroom_id).await.map(|id| id.to_string()),
            "accepted": started,
            "queued": true,
            "started": started,
            "queue_id": item.id.to_string(),
            "position": position,
            "awaiting_answer": false,
            "checklist": [],
            "blackboard": {},
            "summary": "",
        }));
    }

    let direct_content = content.clone();
    let direct_visible = visible.clone();
    let direct_attachments = attachments.clone();
    match start_office_turn(
        &state.db,
        &state.runtime,
        &app,
        chatroom_id,
        direct_content,
        direct_visible,
        direct_attachments,
        false,
        None,
        reply_to_id,
    )
    .await
    {
        Ok(thread_id) => Ok(serde_json::json!({
            "thread_id": thread_id,
            "accepted": true,
            "queued": false,
            "awaiting_answer": false,
            "checklist": [],
            "blackboard": {},
            "summary": "",
        })),
        Err(error) if error.contains("already working") => {
            // A heartbeat claimed the office between the check and the call.
            let item =
                enqueue_office_message(&state.db, chatroom_id, content, visible, queue_attachments, reply_to_id)
                    .await?;
            let position = office_queue_pending_count(&state.db, chatroom_id).await;
            Ok(serde_json::json!({
                "thread_id": chatroom_thread_id(&state.db, chatroom_id).await.map(|id| id.to_string()),
                "accepted": false,
                "queued": true,
                "started": false,
                "queue_id": item.id.to_string(),
                "position": position,
                "awaiting_answer": false,
                "checklist": [],
                "blackboard": {},
                "summary": "",
            }))
        }
        Err(error) => Err(error),
    }
}

/// Offices with a turn currently in flight. A sync mutex is deliberate: the
/// guard is released from `Drop`, which cannot await, and the critical section
/// is one hash-set update.
static OFFICE_ACTIVE_ROOMS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<Uuid>>> =
    std::sync::OnceLock::new();

struct OfficeRunGuard {
    chatroom_id: Uuid,
}

impl OfficeRunGuard {
    /// Atomically claim this office. Returning `None` means another turn
    /// already owns it.
    ///
    /// The old `is_active()` check followed by `new()` was a TOCTOU race: two
    /// starts (a heartbeat and a user message, or two commands) could both see
    /// an idle office and then both insert. The insert itself is the lock.
    fn try_new(chatroom_id: Uuid) -> Option<Self> {
        let rooms = OFFICE_ACTIVE_ROOMS
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
        let mut active = rooms.lock().unwrap_or_else(|e| e.into_inner());
        if !active.insert(chatroom_id) {
            // Do not construct a guard here: its Drop would try to lock the
            // same set while this guard is still held.
            return None;
        }
        Some(Self { chatroom_id })
    }
}

impl Drop for OfficeRunGuard {
    fn drop(&mut self) {
        if let Some(rooms) = OFFICE_ACTIVE_ROOMS.get() {
            let mut active = rooms.lock().unwrap_or_else(|e| e.into_inner());
            active.remove(&self.chatroom_id);
        }
    }
}

fn office_is_active(chatroom_id: Uuid) -> bool {
    OFFICE_ACTIVE_ROOMS
        .get()
        .map(|rooms| {
            rooms
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(&chatroom_id)
        })
        .unwrap_or(false)
}

async fn mark_office_active(db: &ravenbot_db::Database, chatroom_id: Uuid, active: bool) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_active_key(chatroom_id),
        if active { "1" } else { "0" },
    )
    .await;
}

fn office_phase_key(chatroom_id: Uuid) -> String {
    format!("office_phase:{}", chatroom_id)
}

async fn mark_office_phase(db: &ravenbot_db::Database, chatroom_id: Uuid, phase: &str) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_phase_key(chatroom_id),
        phase,
    )
    .await;
}

async fn clear_office_phase(db: &ravenbot_db::Database, chatroom_id: Uuid) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_phase_key(chatroom_id),
        "",
    )
    .await;
}

// ── Office board persistence ─────────────────────────────────────────────
//
// The board used to be persisted by ChatRoomView, which meant a run that
// finished while the user was in another view left its cards frozen at
// "Working". These functions own the projection so the board is a property of
// the run, not of the window that happened to be open.

fn office_autonomy_key(chatroom_id: Uuid) -> String {
    format!("office_autonomy:{}", chatroom_id)
}

fn office_autonomy_last_key(chatroom_id: Uuid) -> String {
    format!("office_autonomy_last:{}", chatroom_id)
}

fn office_active_key(chatroom_id: Uuid) -> String {
    format!("office_active:{}", chatroom_id)
}

fn office_bootstrap_key(chatroom_id: Uuid) -> String {
    format!("office_bootstrap_attempts:{}", chatroom_id)
}

fn office_budget_baseline_key(chatroom_id: Uuid) -> String {
    format!("office_budget_baseline:{}", chatroom_id)
}

fn office_fingerprint_key(chatroom_id: Uuid) -> String {
    format!("office_workspace_fingerprint:{}", chatroom_id)
}

fn office_stagnant_key(chatroom_id: Uuid) -> String {
    format!("office_stagnant_cycles:{}", chatroom_id)
}

async fn bootstrap_attempts(db: &ravenbot_db::Database, chatroom_id: Uuid) -> u32 {
    ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &office_bootstrap_key(chatroom_id))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

async fn bump_bootstrap_attempts(db: &ravenbot_db::Database, chatroom_id: Uuid) -> u32 {
    let attempts = bootstrap_attempts(db, chatroom_id).await.saturating_add(1);
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_bootstrap_key(chatroom_id),
        &attempts.to_string(),
    )
    .await;
    attempts
}

async fn reset_bootstrap_attempts(db: &ravenbot_db::Database, chatroom_id: Uuid) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_bootstrap_key(chatroom_id),
        "0",
    )
    .await;
}

/// Per-bot cost baseline for one office, rolled over monthly.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct OfficeBudgetBaseline {
    #[serde(default)]
    month: String,
    #[serde(default)]
    baseline: std::collections::HashMap<String, f64>,
}

/// Cost this office's members have spent since the first of the month.
///
/// Bot usage is cumulative and a bot can belong to more than one office, so a
/// bot's lifetime spend is not this office's spend. The first time an office
/// sees a member, its current cost becomes that member's baseline; only the
/// delta after that counts. The baseline rolls over at the start of a month,
/// matching the "monthly office budget" the settings promise.
async fn office_budget_spent(
    db: &ravenbot_db::Database,
    room: &ravenbot_core::ChatRoom,
    member_ids: &[Uuid],
) -> f64 {
    let key = office_budget_baseline_key(room.id);
    let stored = ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &key)
        .await
        .ok()
        .flatten();
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    let mut state: OfficeBudgetBaseline = stored
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let mut changed = stored.is_none() || state.month != month;
    if state.month != month {
        state.month = month;
        state.baseline.clear();
    }

    let budgets = ravenbot_governance::BudgetManager::new(db.pool().clone());
    let mut spent = 0.0;
    for bot_id in member_ids {
        let (_, cost) = budgets.get_usage(*bot_id).await.unwrap_or((0, 0.0));
        match state.baseline.get(&bot_id.to_string()).copied() {
            Some(base) if cost < base => {
                // Usage was reset (or the provider corrected its accounting).
                // Re-baseline instead of crediting spend that no longer exists.
                state.baseline.insert(bot_id.to_string(), cost);
                changed = true;
            }
            Some(base) => spent += (cost - base).max(0.0),
            None => {
                state.baseline.insert(bot_id.to_string(), cost);
                changed = true;
            }
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = ravenbot_db::queries::AppSettingsQueries::set(db.pool(), &key, &json).await;
        }
    }
    spent
}

/// `Some((spent, limit))` when a positive ceiling has been reached.
async fn office_budget_exceeded(
    db: &ravenbot_db::Database,
    room: &ravenbot_core::ChatRoom,
    member_ids: &[Uuid],
) -> Option<(f64, f64)> {
    let limit = room.budget.filter(|limit| *limit > 0.0)?;
    let spent = office_budget_spent(db, room, member_ids).await;
    (spent >= limit).then_some((spent, limit))
}

/// A stable fingerprint of the office workspace.
///
/// The autonomous loop used to repeat the same briefing forever because
/// "the run finished" was treated as "the project advanced". This hashes the
/// file tree (relative path, size, and second-resolution mtime) so the
/// heartbeat can tell a cycle that produced something from one that only
/// talked about producing something. Build output, VCS metadata, and virtual
/// environments are excluded: they change constantly without meaning progress.
fn office_workspace_fingerprint(room: Option<&ravenbot_core::ChatRoom>) -> Option<String> {
    use std::hash::Hasher;
    let root = office_root(room)?;
    if !root.is_dir() {
        return Some("missing".to_string());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut files = 0usize;
    let mut bytes = 0u64;
    fingerprint_dir(&root, &root, 0, &mut hasher, &mut files, &mut bytes);
    Some(format!("{:016x}:{files}:{bytes}", hasher.finish()))
}

fn fingerprint_dir(
    root: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    hasher: &mut std::collections::hash_map::DefaultHasher,
    files: &mut usize,
    bytes: &mut u64,
) {
    use std::hash::Hash;
    const MAX_DEPTH: usize = 6;
    const MAX_FILES: usize = 4000;
    if depth > MAX_DEPTH || *files >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if *files >= MAX_FILES {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git"
            || name.starts_with(".venv")
            || matches!(
                name.as_str(),
                "venv"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "__pycache__"
                    | ".pytest_cache"
                    | ".mypy_cache"
                    | ".ruff_cache"
                    | ".next"
                    | ".cache"
                    | "coverage"
            )
        {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let relative = entry.path();
        relative
            .strip_prefix(root)
            .unwrap_or(relative.as_path())
            .hash(hasher);
        if file_type.is_dir() {
            name.hash(hasher);
            fingerprint_dir(root, &relative, depth + 1, hasher, files, bytes);
        } else if file_type.is_file() {
            if let Ok(meta) = entry.metadata() {
                meta.len().hash(hasher);
                *bytes += meta.len();
                if let Ok(modified) = meta.modified() {
                    if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
                        since.as_secs().hash(hasher);
                    }
                }
            }
            *files += 1;
        }
    }
}

async fn stagnant_cycles(db: &ravenbot_db::Database, chatroom_id: Uuid) -> u32 {
    ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &office_stagnant_key(chatroom_id))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

async fn set_stagnant_cycles(db: &ravenbot_db::Database, chatroom_id: Uuid, cycles: u32) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_stagnant_key(chatroom_id),
        &cycles.to_string(),
    )
    .await;
}

/// Give a resumed office a fresh progress window.
async fn reset_office_progress(db: &ravenbot_db::Database, chatroom_id: Uuid) {
    set_stagnant_cycles(db, chatroom_id, 0).await;
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_fingerprint_key(chatroom_id),
        "",
    )
    .await;
}

async fn office_autonomy_enabled(db: &ravenbot_db::Database, chatroom_id: Uuid) -> bool {
    match ravenbot_db::queries::AppSettingsQueries::get(
        db.pool(),
        &office_autonomy_key(chatroom_id),
    )
    .await
    .ok()
    .flatten()
    {
        Some(value) => value != "0" && !value.eq_ignore_ascii_case("false"),
        // Existing offices were sold as autonomous. A missing toggle means on;
        // the header lets the owner pause it.
        None => true,
    }
}

async fn mark_office_last(db: &ravenbot_db::Database, chatroom_id: Uuid) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_autonomy_last_key(chatroom_id),
        &chrono::Utc::now().timestamp().to_string(),
    )
    .await;
}

async fn office_last_seconds(db: &ravenbot_db::Database, chatroom_id: Uuid) -> i64 {
    ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &office_autonomy_last_key(chatroom_id))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

async fn chatroom_thread_id(db: &ravenbot_db::Database, chatroom_id: Uuid) -> Option<Uuid> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string())
            .fetch_optional(db.pool())
            .await
            .ok()?;
    row.and_then(|(id,)| Uuid::parse_str(&id).ok())
}

fn find_board_node_mut<'a>(
    nodes: &'a mut Vec<serde_json::Value>,
    node_id: &str,
) -> Option<&'a mut serde_json::Value> {
    nodes
        .iter_mut()
        .find(|node| node.get("nodeId").and_then(|value| value.as_str()) == Some(node_id))
}

async fn apply_office_board_event(
    db: &ravenbot_db::Database,
    thread_id: Uuid,
    event: &serde_json::Value,
) {
    let key = format!("office_board:{}", thread_id);
    let raw = ravenbot_db::queries::AppSettingsQueries::get(db.pool(), &key)
        .await
        .ok()
        .flatten();
    let mut payload: serde_json::Value = raw
        .as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| serde_json::json!({ "goal": "", "nodes": [] }));
    if !payload
        .get("nodes")
        .map(serde_json::Value::is_array)
        .unwrap_or(false)
    {
        payload["nodes"] = serde_json::json!([]);
    }

    let kind = event
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    match kind {
        "plan_ready" => {
            payload["goal"] = event.get("goal").cloned().unwrap_or(serde_json::json!(""));
            let nodes: Vec<serde_json::Value> = event
                .get("nodes")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|node| {
                    serde_json::json!({
                        "nodeId": node.get("node_id").cloned().unwrap_or(serde_json::json!("")),
                        "botId": node.get("bot_id").cloned().unwrap_or(serde_json::json!("")),
                        "label": node.get("label").cloned().unwrap_or(serde_json::json!("Task")),
                        "dependsOn": node.get("depends_on").cloned().unwrap_or(serde_json::json!([])),
                        "state": "pending",
                    })
                })
                .collect();
            payload["nodes"] = serde_json::Value::Array(nodes);
        }
        "node_open" => {
            let Some(node_id) = event.get("node_id").and_then(serde_json::Value::as_str) else {
                return;
            };
            let nodes = payload["nodes"].as_array_mut().expect("board nodes array");
            if let Some(node) = find_board_node_mut(nodes, node_id) {
                node["state"] = serde_json::json!("running");
                if let Some(thread) = event.get("node_thread_id") {
                    node["nodeThreadId"] = thread.clone();
                }
            } else {
                nodes.push(serde_json::json!({
                    "nodeId": node_id,
                    "botId": event.get("bot_id").cloned().unwrap_or(serde_json::json!("")),
                    "label": event
                        .get("instruction")
                        .cloned()
                        .unwrap_or(serde_json::json!("Working")),
                    "dependsOn": [],
                    "state": "running",
                    "nodeThreadId": event.get("node_thread_id").cloned().unwrap_or(serde_json::json!(null)),
                }));
            }
        }
        "node_finished" => {
            let Some(node_id) = event.get("node_id").and_then(serde_json::Value::as_str) else {
                return;
            };
            let nodes = payload["nodes"].as_array_mut().expect("board nodes array");
            let state = match event
                .get("state")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("done")
            {
                "failed" => "failed",
                "skipped" => "skipped",
                _ => "done",
            };
            if let Some(node) = find_board_node_mut(nodes, node_id) {
                node["state"] = serde_json::json!(state);
                if let Some(preview) = event.get("preview") {
                    node["preview"] = preview.clone();
                }
            }
            if let Some(skipped) = event.get("skipped").and_then(serde_json::Value::as_array) {
                for id in skipped.iter().filter_map(|value| value.as_str()) {
                    if let Some(node) = find_board_node_mut(nodes, id) {
                        node["state"] = serde_json::json!("skipped");
                    }
                }
            }
        }
        "graph_status" => {
            let node_states = event
                .get("nodes")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let nodes = payload["nodes"].as_array_mut().expect("board nodes array");
            for update in node_states {
                let Some(node_id) = update.get("node_id").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(state) = update.get("state").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if let Some(node) = find_board_node_mut(nodes, node_id) {
                    node["state"] = serde_json::json!(state);
                }
            }
        }
        _ => return,
    }

    let encoded = payload.to_string();
    if encoded.len() <= 64 * 1024 {
        let _ = ravenbot_db::queries::AppSettingsQueries::set(db.pool(), &key, &encoded).await;
    }
}

async fn office_has_pending(db: &ravenbot_db::Database, chatroom_id: Uuid) -> bool {
    let Some(thread_id) = chatroom_thread_id(db, chatroom_id).await else {
        return false;
    };
    let mut threads = vec![thread_id];

    // A node runs on its own thread. A question parked there is invisible to a
    // room-thread lookup, and an invisible question means a heartbeat would
    // start a second turn on top of one that is already waiting.
    if let Ok(Some(raw)) = ravenbot_db::queries::AppSettingsQueries::get(
        db.pool(),
        &format!("office_board:{}", thread_id),
    )
    .await
    {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(nodes) = payload.get("nodes").and_then(serde_json::Value::as_array) {
                for node in nodes {
                    if let Some(node_thread) =
                        node.get("nodeThreadId").and_then(serde_json::Value::as_str)
                    {
                        if let Ok(id) = Uuid::parse_str(node_thread) {
                            threads.push(id);
                        }
                    }
                }
            }
        }
    }

    for thread in threads {
        if !ravenbot_db::queries::ApprovalQueries::list_pending_for_thread(db.pool(), thread)
            .await
            .unwrap_or_default()
            .is_empty()
        {
            return true;
        }
        if !ravenbot_db::queries::QuestionQueries::list_pending_for_thread(db.pool(), thread)
            .await
            .unwrap_or_default()
            .is_empty()
        {
            return true;
        }
    }
    false
}

async fn office_board_summary(db: &ravenbot_db::Database, chatroom_id: Uuid) -> String {
    let Some(thread_id) = chatroom_thread_id(db, chatroom_id).await else {
        return "No plan yet.".to_string();
    };
    let Ok(Some(raw)) = ravenbot_db::queries::AppSettingsQueries::get(
        db.pool(),
        &format!("office_board:{}", thread_id),
    )
    .await
    else {
        return "No plan yet.".to_string();
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return "No plan yet.".to_string();
    };
    let Some(nodes) = payload.get("nodes").and_then(serde_json::Value::as_array) else {
        return "No plan yet.".to_string();
    };
    if nodes.is_empty() {
        return "No plan yet.".to_string();
    }
    nodes
        .iter()
        .map(|node| {
            let state = node
                .get("state")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("pending");
            let label = node
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Task");
            format!("- [{state}] {label}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A compact, model-visible picture of the office workspace.
///
/// The planner used to see only the conversation: agents said "no app exists"
/// and the next cycle read the same docs again. The snapshot makes the missing
/// implementation a fact in the prompt: top-level entries, whether a code
/// manifest exists, and the live `Shared/STATUS.md`.
fn office_root(room: Option<&ravenbot_core::ChatRoom>) -> Option<std::path::PathBuf> {
    let room = room?;
    let configured = room
        .project_folders
        .first()
        .cloned()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| {
            ravenbot_runtime::default_project_dir(&room.name)
                .to_string_lossy()
                .to_string()
        });
    Some(ravenbot_core::expand_home(configured.trim()))
}

/// Top-level directories and files worth showing an agent.
///
/// Build output, virtualenvs, and `.git` are noise: an office that sees
/// `.venv/bin/python` before `app/` spends a turn on the wrong thing.
fn office_top_level(root: &std::path::Path) -> (Vec<String>, Vec<String>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git"
                || name.starts_with(".venv")
                || matches!(
                    name.as_str(),
                    "node_modules" | "target" | "dist" | "build" | "__pycache__" | ".pytest_cache"
                )
            {
                continue;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            if is_dir {
                dirs.push(format!("{name}/"));
            } else {
                let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                files.push(format!("{name} ({size} B)"));
            }
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

fn office_has_implementation(room: Option<&ravenbot_core::ChatRoom>) -> bool {
    let Some(root) = office_root(room) else {
        return false;
    };
    let (_dirs, files) = office_top_level(&root);
    let code_dir_has_a_file = ["app", "src", "tests"].iter().any(|dir| {
        std::fs::read_dir(root.join(dir))
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    });
    code_dir_has_a_file
        || files.iter().any(|name| {
            matches!(
                name.split_whitespace().next().unwrap_or(""),
                "pyproject.toml" | "package.json" | "Cargo.toml" | "go.mod" | "Makefile"
            )
        })
}

/// A compact, model-visible picture of the office workspace.
///
/// The planner used to see only the conversation: agents said "no app exists"
/// and the next cycle read the same docs again. The snapshot makes the missing
/// implementation a fact in the prompt: top-level entries, whether a code
/// manifest exists, and the live `Shared/STATUS.md`.
fn office_workspace_snapshot(room: Option<&ravenbot_core::ChatRoom>) -> String {
    let Some(root) = office_root(room) else {
        return "Workspace: (none)".to_string();
    };
    if !root.is_dir() {
        return format!("Workspace: {} (missing)", root.display());
    }
    let (dirs, files) = office_top_level(&root);
    let top = dirs
        .into_iter()
        .chain(files)
        .take(60)
        .collect::<Vec<_>>()
        .join(", ");
    let has_code = office_has_implementation(room);
    let status = std::fs::read_to_string(root.join("Shared/STATUS.md"))
        .map(|text| truncate_chars(text.trim(), 3000))
        .unwrap_or_else(|_| "(no Shared/STATUS.md)".to_string());
    format!(
        "Workspace: {}\nTop level: {}\nImplementation: {}\nShared/STATUS.md:\n{}",
        root.display(),
        if top.is_empty() { "(empty)" } else { &top },
        if has_code {
            "present (a code root or manifest exists)"
        } else {
            "MISSING — no app/, src/, tests/, pyproject.toml, package.json, Cargo.toml, or go.mod at the workspace root"
        },
        status
    )
}

async fn autonomous_prompt(db: &ravenbot_db::Database, room: &ravenbot_core::ChatRoom) -> String {
    let summary = office_board_summary(db, room.id).await;
    let snapshot = office_workspace_snapshot(Some(room));
    let mission = room
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
        .unwrap_or_else(|| room.description.trim());
    format!(
        "[Autonomous cycle — no user is present]\n\n\
Office: {name}\n\
Goal: {goal}\n\n\
Current board:\n{summary}\n\n\
Workspace snapshot:\n{snapshot}\n\n\
Continue the mission from the board above:\n\
- Finish any task that is not done. Do not redo completed work.\n\
- If every planned task is done, choose the next concrete step toward the goal and execute it.\n\
- Read Shared/STATUS.md and the workspace before changing anything.\n\
- If the snapshot says Implementation is MISSING, do not spend this cycle reading docs. The first task must create the minimal runnable skeleton (entrypoint, one module, one test, manifest) and then verify it.\n\
- If the board and workspace are unchanged from the last cycle, do not repeat the same plan. Change the approach or write the blocker; repeating an inspection that produced no artifact is a failure.\n\
- If something is blocked, write the blocker and the exact next step to Shared/STATUS.md, then work on an unblocked task.\n\
- Do not ask the user questions. Proceed with your best judgement.\n\
- Verify the change before reporting it.",
        name = room.name,
        goal = mission,
        summary = summary,
    )
}

/// Stop an office's autonomous loop and leave one visible reason in the room.
async fn pause_autonomous_office(db: &ravenbot_db::Database, chatroom_id: Uuid, message: String) {
    let _ = ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        &office_autonomy_key(chatroom_id),
        "0",
    )
    .await;
    if let Ok(roster) = load_office_roster(db.pool(), chatroom_id).await {
        if let Some(thread_id) = chatroom_thread_id(db, chatroom_id).await {
            let _ = post_bot_message(
                db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name,
                message,
            )
            .await;
        }
    }
}

/// Starts queued user messages as soon as an office is free.
///
/// A separate worker rather than a call at the end of `start_office_turn`:
/// that call would make the async function recursive, and the compiler cannot
/// prove a recursive future is `Send`. The worker also picks up a queue left
/// behind by a crash or an app restart.
async fn office_queue_loop(
    db: ravenbot_db::Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    app: tauri::AppHandle,
) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if runtime.is_paused().await {
            continue;
        }
        let rooms = ravenbot_db::queries::ChatRoomQueries::list(db.pool())
            .await
            .unwrap_or_default();
        for room in rooms {
            if office_is_active(room.id) {
                continue;
            }
            if !office_queue_has_pending(&db, room.id).await {
                continue;
            }
            if drain_office_queue(&db, &runtime, &app, room.id).await {
                tracing::info!(chatroom_id = %room.id, "Queued office message started");
                break;
            }
        }
    }
}

/// Keeps autonomous offices moving while the owner is elsewhere.
///
/// This is deliberately a heartbeat rather than a long-lived office run: each
/// cycle is one normal office turn (plan → graph → synthesis), persisted to the
/// conversation and the board. It runs in the app process, not in a window, so
/// navigating away cannot stop it. The header toggle, the kill switch, and a
/// pending question all pause it.
async fn office_autonomy_loop(
    db: ravenbot_db::Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    app: tauri::AppHandle,
) {
    // Let startup, model discovery, and any restored run settle before
    // spending a token on a heartbeat.
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        if runtime.is_paused().await {
            continue;
        }

        let rooms = ravenbot_db::queries::ChatRoomQueries::list(db.pool())
            .await
            .unwrap_or_default();
        for room in rooms {
            let mission = room
                .goal
                .as_deref()
                .map(str::trim)
                .filter(|goal| !goal.is_empty())
                .or_else(|| Some(room.description.trim()).filter(|desc| !desc.is_empty()));
            if mission.is_none() {
                continue;
            }
            // A queued user message owns the next turn. The queue worker
            // starts it (or the user has paused it); autonomy must not jump
            // ahead of a person's instruction.
            if !office_is_active(room.id) && office_queue_has_pending(&db, room.id).await {
                continue;
            }
            if !office_autonomy_enabled(&db, room.id).await {
                continue;
            }
            // Only this room is serialized. Two offices have independent
            // threads, boards, and stream emitters, so one long run must not
            // stop every other office in the app from making progress.
            if office_is_active(room.id) {
                continue;
            }
            let members = ravenbot_db::queries::ChatRoomQueries::list_members(db.pool(), room.id)
                .await
                .unwrap_or_default();
            if members.is_empty() {
                continue;
            }
            if office_has_pending(&db, room.id).await {
                continue;
            }
            let since = chrono::Utc::now().timestamp() - office_last_seconds(&db, room.id).await;
            if since < 300 {
                continue;
            }

            // A monthly ceiling is only a promise if the unattended loop obeys
            // it. The baseline is per office and per bot, so a shared agent's
            // spend in another office does not count against this one.
            let member_ids: Vec<Uuid> = members.iter().map(|member| member.bot_id).collect();
            if let Some((spent, limit)) = office_budget_exceeded(&db, &room, &member_ids).await {
                pause_autonomous_office(
                    &db,
                    room.id,
                    format!(
                        "⏸️ **Autonomy paused — monthly budget reached.** This office has spent ${spent:.2} of its ${limit:.2} ceiling. Raise the ceiling in Office settings, then press Autonomous to resume."
                    ),
                )
                .await;
                tracing::warn!(
                    chatroom_id = %room.id,
                    spent,
                    limit,
                    "Office paused at its monthly budget ceiling"
                );
                continue;
            }

            // Track whether the last cycle actually changed the workspace. A
            // run that only re-read the same files is not progress, and three
            // of them in a row is the office talking to itself.
            let mut progress_update: Option<(String, u32)> = None;
            if office_has_implementation(Some(&room)) {
                reset_bootstrap_attempts(&db, room.id).await;
                if let Some(fingerprint) = office_workspace_fingerprint(Some(&room)) {
                    let previous = ravenbot_db::queries::AppSettingsQueries::get(
                        db.pool(),
                        &office_fingerprint_key(room.id),
                    )
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                    let unchanged = !previous.trim().is_empty() && previous == fingerprint;
                    let cycles = if unchanged {
                        stagnant_cycles(&db, room.id).await.saturating_add(1)
                    } else {
                        0
                    };
                    if cycles >= 3 {
                        pause_autonomous_office(
                            &db,
                            room.id,
                            "⏸️ **Autonomy paused — no workspace progress.** Three autonomous cycles finished without adding or changing a file in the project folder. The office is paused so it cannot repeat the same briefing. Read `Shared/STATUS.md`, write down the blocker or the next concrete change, then press Autonomous to resume.".to_string(),
                        )
                        .await;
                        tracing::warn!(
                            chatroom_id = %room.id,
                            cycles,
                            "Office paused after repeated no-progress cycles"
                        );
                        continue;
                    }
                    progress_update = Some((fingerprint, cycles));
                }
            } else {
                let attempts = bootstrap_attempts(&db, room.id).await;
                if attempts >= 3 {
                    // Repeating a bootstrap that produces no files burns the
                    // owner's budget without changing anything. Stop and say
                    // why; the pause is visible and resumable in the header.
                    pause_autonomous_office(
                        &db,
                        room.id,
                        "⏸️ **Autonomy paused.** Three bootstrap attempts produced no files in the workspace. Check the project folder, model, and tool permissions, then resume with the Autonomous button. The office will not repeat the same inspection again.".to_string(),
                    )
                    .await;
                    tracing::warn!(chatroom_id = %room.id, "Office paused after repeated empty bootstraps");
                    continue;
                }
            }
            let prompt = autonomous_prompt(&db, &room).await;
            match start_office_turn(
                &db,
                &runtime,
                &app,
                room.id,
                prompt,
                "[Autonomous cycle] Continuing the office mission.".to_string(),
                None,
                true,
                None,
                None,
            )
            .await
            {
                Ok(_) => {
                    // Record the tree the next cycle must beat. Doing this only
                    // after a turn was accepted means a failed start cannot
                    // masquerade as a stagnant cycle.
                    if let Some((fingerprint, cycles)) = progress_update {
                        let _ = ravenbot_db::queries::AppSettingsQueries::set(
                            db.pool(),
                            &office_fingerprint_key(room.id),
                            &fingerprint,
                        )
                        .await;
                        set_stagnant_cycles(&db, room.id, cycles).await;
                    }
                    tracing::info!(
                        chatroom_id = %room.id,
                        office = %room.name,
                        "Autonomous office cycle started"
                    );
                    // One cycle per tick: each run is a full plan/execute/
                    // synthesize turn. Concurrency is bounded by the graph
                    // executor, not by how many rooms the tick can start.
                    break;
                }
                Err(error) => {
                    tracing::warn!(
                        chatroom_id = %room.id,
                        error = %error,
                        "Autonomous office cycle could not start"
                    );
                    mark_office_last(&db, room.id).await;
                }
            }
        }
    }
}

#[tauri::command]
async fn get_office_autonomy(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<bool, String> {
    Ok(office_autonomy_enabled(&state.db, chatroom_id).await)
}

#[tauri::command]
async fn get_office_active(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<bool, String> {
    Ok(ravenbot_db::queries::AppSettingsQueries::get(
        state.db.pool(),
        &office_active_key(chatroom_id),
    )
    .await
    .ok()
    .flatten()
    .as_deref()
        == Some("1"))
}

#[tauri::command]
async fn get_office_phase(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
) -> Result<Option<String>, String> {
    Ok(ravenbot_db::queries::AppSettingsQueries::get(
        state.db.pool(),
        &office_phase_key(chatroom_id),
    )
    .await
    .ok()
    .flatten()
    .filter(|phase| !phase.trim().is_empty()))
}

#[tauri::command]
async fn set_office_autonomy(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    enabled: bool,
) -> Result<(), String> {
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        &office_autonomy_key(chatroom_id),
        if enabled { "1" } else { "0" },
    )
    .await
    .map_err(|e| e.to_string())?;
    if enabled {
        reset_bootstrap_attempts(&state.db, chatroom_id).await;
        reset_office_progress(&state.db, chatroom_id).await;
    }
    Ok(())
}

/// Truncate to a char boundary at most `max` characters (safe for UTF-8).
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

// Input types

#[derive(serde::Deserialize)]
struct GraphTask {
    bot_id: Uuid,
    instruction: String,
    depends_on: Vec<usize>,
}

// Output types

#[derive(serde::Serialize)]
struct StatusInfo {
    active_bots: u32,
    running_tasks: u32,
    session_tokens: u64,
    session_cost: f64,
    kill_switch_active: bool,
}

#[derive(serde::Serialize)]
struct GraphResult {
    goal: String,
    /// Room thread the plan was posted into (None for headless graph runs).
    thread_id: Option<Uuid>,
    checklist: Vec<ChecklistItem>,
    blackboard_data: std::collections::HashMap<String, String>,
    /// Lead's synthesized final answer (None for headless graph runs).
    summary: Option<String>,
}

/// Enable microphone capture on Linux. WebKitGTK disables media streams by
/// default and Wry exposes no setting for it, so `getUserMedia` fails unless we
/// flip the WebKit setting and answer the permission request. macOS/WKWebView
/// only needs the Info.plist usage string (see src-tauri/Info.plist).
#[cfg(target_os = "linux")]
fn enable_linux_media_capture(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("main webview not found; microphone capture not enabled");
        return;
    };
    let _ = window.with_webview(|webview| {
        use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};
        let view = webview.inner();
        if let Some(settings) = view.settings() {
            settings.set_enable_media_stream(true);
            settings.set_enable_webrtc(true);
            settings.set_enable_mediasource(true);
        }
        // Single trusted local UI: grant media permission requests.
        view.connect_permission_request(|_, request| {
            request.allow();
            true
        });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Wayland/WebKitGTK fix for Hyprland/Omarchy (Gdk Error 71: Protocol error)
    // Must be set before any WebKit webview is created.
    // Safe to set unconditionally — WebKit checks these env vars at init.
    if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
        // Only set if not already set; 1 disables accelerated compositing which triggers DMABUF on Wayland
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ravenbot=debug".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .register_uri_scheme_protocol("ravenvisual", |_ctx, request| {
            visual_protocol_response(&request)
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .setup(|app| {
            // Everything RAVENBOT owns lives under one folder (`~/RAVENBOT`),
            // resolved by the same helper the CLI and MCP server use. Deriving
            // it from `app.path().app_data_dir()` here is what let the two
            // drift apart into different databases.
            let db_path = ravenbot_core::default_db_path();

            // An install from before the `~/RAVENBOT` layout has its database
            // in the OS app-data dir. Move it rather than presenting an empty
            // app — the sidecar -wal / -shm files go with it or the moved
            // database is missing committed transactions.
            if let Some(legacy) = ravenbot_core::legacy_db_path() {
                match std::fs::rename(&legacy, &db_path) {
                    Ok(()) => {
                        // `-wal` / `-shm` are named after the database, not
                        // derived from it, so append rather than substitute.
                        for suffix in ["-wal", "-shm"] {
                            let mut side = legacy.clone().into_os_string();
                            side.push(suffix);
                            let _ = std::fs::remove_file(std::path::PathBuf::from(side));
                        }
                        tracing::info!(
                            from = %legacy.display(),
                            to = %db_path.display(),
                            "Moved the existing RAVENBOT database into ~/RAVENBOT"
                        );
                    }
                    Err(e) => tracing::warn!(
                        "Could not move {} to {}: {e}",
                        legacy.display(),
                        db_path.display()
                    ),
                }
            }

            // Initialize database + runtime inside Tokio context so Runtime::new can spawn plugin seeding
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (db, runtime) = rt.block_on(async {
                let db = Database::new(&db_path).await.expect("failed to initialize database");

                // The user's Wayland opt-in survives restarts. Apply it before
                // the runtime can execute any host-control tool.
                if let Ok(Some(value)) =
                    ravenbot_db::queries::AppSettingsQueries::get(
                        db.pool(),
                        "wayland_control_allowed",
                    )
                    .await
                {
                    ravenbot_core::set_wayland_control_allowed(
                        value == "1" || value.eq_ignore_ascii_case("true"),
                    );
                }

                // Runs are in-memory loops: anything still non-terminal at
                // startup was orphaned by a crash or quit. Close them so
                // liveness queries (e.g. the office board) tell the truth.
                let now = chrono::Utc::now().to_rfc3339();
                match sqlx::query(
                    "UPDATE runs SET state = 'failed', outcome = ?,
                        updated_at = ?, completed_at = ?
                     WHERE state IN ('planning','acting','observing','reflecting','waiting_on_user','paused')",
                )
                .bind(r#"{"Failure":{"error":"app quit while the run was active"}}"#)
                .bind(&now)
                .bind(&now)
                .execute(db.pool())
                .await
                {
                    Ok(r) if r.rows_affected() > 0 => {
                        tracing::warn!(orphaned_runs = r.rows_affected(), "Marked interrupted runs failed at startup");
                    }
                    Err(e) => tracing::warn!("Orphaned-run reconciliation failed: {e}"),
                    _ => {}
                }

                // Clear the per-office active marker. It is written before a
                // turn and cleared by the spawned task; a quit between those
                // points leaves it at 1 with no run behind it, and the next
                // send waits on `get_office_active` for a turn that is not
                // there. No office turn can be live before setup finishes.
                if let Ok(rows) = sqlx::query(
                    "UPDATE app_settings SET value = '0' WHERE key LIKE 'office_active:%'",
                )
                .execute(db.pool())
                .await
                {
                    if rows.rows_affected() > 0 {
                        tracing::warn!(
                            markers = rows.rows_affected(),
                            "Cleared stale office-active markers at startup"
                        );
                    }
                }
                let _ = sqlx::query(
                    "UPDATE app_settings SET value = '' WHERE key LIKE 'office_phase:%'",
                )
                .execute(db.pool())
                .await;

                // The same reasoning for agents: a crash mid-run leaves rows
                // saying `thinking`, and the fleet then looks permanently busy
                // with nobody working.
                match ravenbot_db::queries::BotQueries::mark_all_idle(db.pool()).await {
                    Ok(n) if n > 0 => {
                        tracing::info!(bots = n, "Reset agent status at startup");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("Agent status reset failed: {e}"),
                }

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
                // Pre-populate saved Ollama URL if saved
                if let Ok(Some(url)) = ravenbot_db::queries::AppSettingsQueries::get(db.pool(), "ollama_url").await {
                    if !url.trim().is_empty() {
                        let mut manager = runtime.provider_manager().lock().await;
                        manager.set_base_url("ollama", url);
                    }
                }
                // Pre-populate user-defined providers and base URL overrides (P8)
                if let Ok(rows) = ravenbot_db::queries::CustomProviderQueries::list(db.pool()).await {
                    let mut manager = runtime.provider_manager().lock().await;
                    for (id, display_name, kind, base_url, default_model, supports_tools, enabled, headers_json) in rows {
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

                (db, runtime)
            });

            // Store state
            let routine_manager = Arc::new(ravenbot_scheduler::routine::RoutineManager::new(db.pool().clone()));
            let scheduler = Arc::new(ravenbot_scheduler::Scheduler::new(
                routine_manager,
                ravenbot_scheduler::SchedulerConfig::default(),
            ));
            app.manage(AppState {
                db: db.clone(),
                runtime: runtime.clone(),
                scheduler: scheduler.clone(),
                model_discovery: Arc::new(ravenbot_models::ModelDiscovery::default()),
                plugin_catalog: Arc::new(
                    ravenbot_plugins::catalog::CatalogService::default_cached(),
                ),
                webhook_base_url: std::sync::RwLock::new("http://127.0.0.1:8800".to_string()),
            });

            // Agent tools can ask the UI to show a real page. The callback is
            // just an event: the frontend owns the dock's layout and its
            // native bounds, so it cannot be guessed from the backend.
            let dock_app = app.handle().clone();
            ravenbot_core::set_browser_dock_opener(move |url| {
                let _ = dock_app.emit("browser-dock:request", serde_json::json!({"url": url}));
            });

            #[cfg(target_os = "linux")]
            {
                let eval_app = app.handle().clone();
                ravenbot_core::set_browser_dock_evaluator(move |script| {
                    // The skill path reaches this evaluator directly, so the
                    // takeover gate must live here as well as on the command.
                    if browser_dock_takeover_enabled() {
                        return Err(
                            "The user has taken over the browser dock. Agent actions are paused until they release control."
                                .to_string(),
                        );
                    }
                    let value = browser_dock_eval_blocking(&eval_app, &script)?;
                    Ok(value.to_string())
                });
            }

            // One activity channel for the browser and the real desktop. The
            // UI turns these into a live action strip and click marker.
            {
                let activity_app = app.handle().clone();
                ravenbot_core::set_agent_activity_listener(move |event| {
                    let _ = activity_app.emit("agent-activity", event);
                });
            }

            place_main_window(app.handle());

            // Voice input on Linux needs WebKit media-stream enabled explicitly.
            #[cfg(target_os = "linux")]
            enable_linux_media_capture(app.handle());

            // Install the routine executor: due routines create a thread and really run
            let exec_app = app.handle().clone();
            let exec_db = db.clone();
            let exec_runtime = runtime.clone();
            let executor: ravenbot_scheduler::scheduler::RoutineExecutor = Arc::new(move |routine| {
                let app = exec_app.clone();
                let db = exec_db.clone();
                let runtime = exec_runtime.clone();
                Box::pin(async move {
                    execute_routine_instruction(&db, &runtime, &app, &routine).await
                })
            });
            tauri::async_runtime::block_on(scheduler.set_executor(executor.clone()));

            // Start the inbound webhook receiver (loopback only). Best-effort:
            // if the port is taken, the app still runs and the UI shows the
            // trigger as unreachable rather than refusing to start.
            let hook_pool = db.pool().clone();
            let hook_executor = executor.clone();
            let hook_state = app.state::<AppState>();
            match tauri::async_runtime::block_on(async {
                ravenbot_scheduler::WebhookServer::start(
                    hook_pool,
                    hook_executor,
                    ravenbot_scheduler::WebhookConfig::default(),
                )
                .await
            }) {
                Ok(server) => {
                    let url = server.url();
                    if let Ok(mut base) = hook_state.webhook_base_url.write() {
                        *base = url.clone();
                    }
                    // Keep the server alive for the app's lifetime.
                    app.manage(server);
                    tracing::info!("Webhook receiver listening at {}", url);
                }
                Err(e) => tracing::warn!("Webhook receiver disabled: {}", e),
            }

            // Start the scheduler tick loop
            tauri::async_runtime::spawn(async move {
                scheduler.start().await;
            });

            // Offices keep working when the window is on another screen:
            // the heartbeat lives in the app process, not in a component.
            let autonomy_db = db.clone();
            let autonomy_runtime = runtime.clone();
            let autonomy_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                office_autonomy_loop(autonomy_db, autonomy_runtime, autonomy_app).await;
            });

            let queue_db = db.clone();
            let queue_runtime = runtime.clone();
            let queue_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                office_queue_loop(queue_db, queue_runtime, queue_app).await;
            });

            tracing::info!("RAVENBOT initialized, database at: {:?}", db_path);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_model_catalog,
            get_configured_providers,
            set_provider_api_key,
            set_api_key,
            check_api_key,
            fetch_provider_models,
            fetch_all_provider_models,
            get_ollama_url,
            set_ollama_url,
            list_custom_providers,
            upsert_custom_provider,
            delete_custom_provider,
            test_custom_provider,
            set_provider_base_url,
            get_provider_base_url,
            probe_ollama,
            get_default_model,
            set_default_model,
            set_approval_mode,
            get_approval_mode,
            list_pending_approvals,
            list_pending_approvals_for_bot,
            decide_approval,
            list_bot_todos,
            save_office_board,
            get_office_board,
            list_pending_questions,
            list_pending_questions_for_bot,
            answer_question,
            cancel_run,
            pause_run,
            resume_run,
            get_sandbox_report,
            computer_capabilities,
            browse_workspace,
            summarize_workspace,
            default_workspace_for,
            list_projects,
            file_change_totals,
            list_file_changes,
            list_recent_file_changes,
            file_change_diff,
            read_workspace_file,
            read_workspace_media,
            write_workspace_file,
            register_visual,
            release_visual,
            browser_dock_open,
            browser_dock_set_bounds,
            browser_dock_navigate,
            browser_dock_back,
            browser_dock_forward,
            browser_dock_reload,
            browser_dock_hide,
            browser_dock_destroy,
            browser_dock_state,
            browser_dock_eval,
            browser_dock_takeover,
            browser_dock_set_takeover,
            browser_dock_focus,
            computer_takeover,
            set_computer_takeover,
            set_wayland_control_allowed,
            list_desktop_windows,
            focus_desktop_window,
            list_browser_downloads,
            browser_downloads_dir,
            local_skills_list,
            local_skills_read,
            local_skills_install,
            local_skills_marketplace_search,
            local_skills_marketplace_install,
            open_workspace_in_file_manager,
            bot_desktop_status,
            start_bot_desktop,
            stop_bot_desktop,
            capture_screen,
            capture_desktop_window,
            get_input_backend,
            transcribe_audio,
            synthesize_speech,
            list_tts_voices,
            create_bot,
            get_default_agent,
            set_default_agent,
            create_default_agent,
            list_bots,
            get_bot,
            list_runs,
            get_usage_summary,
            update_bot,
            delete_bot,
            list_bot_contacts,
            set_bot_pinned,
            set_bot_hidden,
            mark_bot_read,
            reorder_bots,
            get_unread_counts,
            duplicate_bot,
            create_thread,
            list_threads,
            list_messages,
            toggle_message_reaction,
            rename_thread,
            delete_thread,
            search_messages,
            send_message,
            regenerate_message,
            edit_and_resend,
            execute_graph,
            draft_office_plan,
            pause_all,
            resume_all,
            get_status,
            trigger_kill_switch,
            release_kill_switch,
            get_kill_switch_status,
            create_chatroom,
            list_chatrooms,
            get_chatroom,
            add_member_to_chatroom,
            list_chatroom_members,
            get_chatroom_thread,
            send_to_chatroom,
            compact_thread_context,
            get_office_queue,
            get_office_queue_paused,
            set_office_queue_paused,
            cancel_office_queue_item,
            dismiss_office_queue_item,
            get_office_autonomy,
            get_office_active,
            get_office_phase,
            set_office_autonomy,
            list_all_skills,
            sync_plugins,
            list_plugins,
            search_plugin_catalog,
            install_catalog_plugin,
            get_plugin_details,
            set_plugin_credential,
            set_plugin_base_url,
            clear_plugin_credential,
            list_bot_plugins,
            toggle_bot_plugin,
            toggle_plugin_global,
            list_global_plugins,
            import_openapi_plugin,
            list_mcp_servers,
            toggle_mcp_server,
            toggle_bot_mcp_server,
            list_bot_mcp_servers,
            save_custom_mcp_server,
            delete_mcp_server,
            get_mcp_server_env,
            save_mcp_server_env,
            test_mcp_server,
            batch_assign_bot_mcp,
            batch_set_bot_mcp,
            update_chatroom,
            delete_chatroom,
            remove_chatroom_member,
            update_chatroom_member,
            create_bot_for_office,
            default_office_org,
            provision_office_org,
            draft_office_blueprint,
            draft_office_org,
            add_office_memory,
            list_office_memories,
            search_office_memories,
            get_agent_intelligence,
            get_bot_budget,
            set_bot_budget,
            reset_bot_budget,
            get_session_usage,
            list_agent_learnings,
            export_bot_bundle,
            import_bot_bundle,
            import_bot_bundle_from_file,
            create_routine,
            get_routine,
            list_routines,
            update_routine,
            delete_routine,
            get_scheduler_status,
            run_routine_now,
            enable_routine_webhook,
            disable_routine_webhook,
            get_routine_webhook_status,
            preview_team,
            fetch_and_preview_team,
            import_team,
            list_channels,
            create_channel,
            update_channel,
            delete_channel,
            set_channel_bots,
            list_channel_messages,
            post_channel_message,
            list_channel_tasks,
            create_channel_task,
            start_channel_task,
            channel_task_action,
            list_channel_task_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ravenbot");
}

#[cfg(test)]
mod tier4_tests {
    use super::*;

    #[tokio::test]
    async fn bot_contacts_pin_hide_and_unread() {
        use ravenbot_db::queries::{BotContactQueries, BotQueries, MessageQueries, ThreadQueries};
        let pool = temp_pool().await;
        let bot = Bot::new("Contact Test", "contact state test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        BotContactQueries::ensure_rows(&pool).await.unwrap();

        // A fresh bot starts read (zero unread).
        let unread_for = |rows: Vec<(uuid::Uuid, i64)>| {
            rows.into_iter()
                .find(|(id, _)| *id == bot.id)
                .map(|(_, c)| c)
                .unwrap_or(0)
        };
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 0);

        // A new assistant reply becomes unread.
        let thread = Thread::new(bot.id, "contact thread");
        ThreadQueries::create(&pool, &thread).await.unwrap();
        MessageQueries::insert(&pool, &Message::assistant(thread.id, "hello"))
            .await
            .unwrap();
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 1);

        // Marking read clears it.
        BotContactQueries::mark_read(&pool, bot.id).await.unwrap();
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 0);

        // Pin + hide round-trip.
        BotContactQueries::set_pinned(&pool, bot.id, true)
            .await
            .unwrap();
        BotContactQueries::set_hidden(&pool, bot.id, true)
            .await
            .unwrap();
        let rows = BotContactQueries::list(&pool).await.unwrap();
        let (_, pinned, hidden, _) = rows.iter().find(|(id, ..)| *id == bot.id).unwrap();
        assert!(*pinned && *hidden);
    }

    #[tokio::test]
    async fn channel_lead_resolution_and_mentions() {
        use ravenbot_db::queries::{BotQueries, ChannelQueries};
        let pool = temp_pool().await;
        let mut lead = Bot::new("Lead", "lead");
        lead.is_orchestrator = true;
        let worker = Bot::new("Worker", "worker");
        BotQueries::insert(&pool, &lead).await.unwrap();
        BotQueries::insert(&pool, &worker).await.unwrap();

        let channel = Channel::new("Test Channel");
        ChannelQueries::create(&pool, &channel).await.unwrap();
        ChannelQueries::set_bots(&pool, channel.id, &[lead.id, worker.id])
            .await
            .unwrap();

        // Auto lead = the orchestrator.
        let rules = serde_json::json!({});
        assert_eq!(
            channel_lead_bot(&pool, channel.id, &rules).await,
            Some(lead.id)
        );
        // Explicit lead override wins.
        let rules = serde_json::json!({ "lead_bot_id": worker.id.to_string() });
        assert_eq!(
            channel_lead_bot(&pool, channel.id, &rules).await,
            Some(worker.id)
        );
        // Manual mode: @mention by name.
        assert!(channel_bot_mentioned(&pool, worker.id, "hey @Worker please help").await);
        assert!(!channel_bot_mentioned(&pool, worker.id, "no mention here").await);
    }

    #[tokio::test]
    async fn chatroom_edits_persist_through_json_round_trip() {
        use ravenbot_db::queries::ChatRoomQueries;
        let pool = temp_pool().await;
        let room = ChatRoom::new("Old Office", "old desc", "custom");
        ChatRoomQueries::create(&pool, &room).await.unwrap();

        // Mimic the Tauri invoke contract: serialize → deserialize → update.
        let json = serde_json::to_value(&room).unwrap();
        let mut updated: ChatRoom = serde_json::from_value(json).unwrap();
        updated.name = "New Office".into();
        updated.description = "new desc".into();
        updated.office_template = "it-office".into();
        updated.goal = Some("Ship v1".into());
        updated.policy = Some("Be concise".into());
        updated.terms = Some("Terms text".into());
        updated.budget = Some(250.0);
        ChatRoomQueries::update(&pool, &updated).await.unwrap();

        let back = ChatRoomQueries::get(&pool, room.id).await.unwrap().unwrap();
        assert_eq!(back.name, "New Office");
        assert_eq!(back.description, "new desc");
        assert_eq!(back.office_template, "it-office");
        assert_eq!(back.goal.as_deref(), Some("Ship v1"));
        assert_eq!(back.policy.as_deref(), Some("Be concise"));
        assert_eq!(back.terms.as_deref(), Some("Terms text"));
        assert_eq!(back.budget, Some(250.0));
    }

    #[test]
    fn briefing_lists_assignees_and_dependencies() {
        let items = vec![
            (
                "Growth Marketer".to_string(),
                "Draft the campaign".to_string(),
                vec![],
            ),
            (
                "Chief of Staff".to_string(),
                "Review it".to_string(),
                vec![0],
            ),
        ];
        let brief = briefing_markdown(&items);
        assert!(brief.contains("1. **Growth Marketer** — Draft the campaign"));
        assert!(brief.contains("2. **Chief of Staff** — Review it"));
        assert!(brief.contains("_(after 1)_"), "dependency shown: {brief}");
    }

    fn test_role(name: &str, rank: &str, specialty: &str) -> office_org::RoleSpec {
        office_org::RoleSpec {
            name: name.to_string(),
            rank: rank.to_string(),
            specialty: specialty.to_string(),
            system_prompt: None,
            is_lead: false,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            plugins: Vec::new(),
            avatar_style: None,
        }
    }

    #[test]
    fn reuses_existing_agent_by_name() {
        let mut bot = Bot::new("Coder", "Implementation");
        bot.rank = Some("Developer".into());
        bot.specialty = Some("Implementation".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        let found = find_reusable_agent(&[bot.clone()], &used, &role);
        assert_eq!(
            found.map(|b| b.id),
            Some(bot.id),
            "exact name should be reused"
        );
    }

    #[test]
    fn reuses_agent_by_specialty_when_name_differs() {
        let mut bot = Bot::new("Alice", "Implementation");
        bot.specialty = Some("Implementation".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        let found = find_reusable_agent(&[bot.clone()], &used, &role);
        assert_eq!(
            found.map(|b| b.id),
            Some(bot.id),
            "specialty match should be reused"
        );
    }

    #[test]
    fn never_reuses_an_agent_twice_in_one_run() {
        let mut bot = Bot::new("Coder", "Implementation");
        bot.specialty = Some("Implementation".into());
        let mut used = std::collections::HashSet::new();
        used.insert(bot.id);
        let role = test_role("Coder", "Developer", "Implementation");
        assert!(find_reusable_agent(&[bot], &used, &role).is_none());
    }

    #[test]
    fn creates_new_when_no_existing_agent_matches() {
        let mut bot = Bot::new("Designer", "Visual");
        bot.specialty = Some("Visual".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        assert!(find_reusable_agent(&[bot], &used, &role).is_none());
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("hello world", 5), "hello…");
        // Multi-byte chars must not panic or split a codepoint.
        assert_eq!(truncate_chars("héllo wörld", 5), "héllo…");
    }

    async fn temp_pool() -> sqlx::SqlitePool {
        let path = std::env::temp_dir().join(format!("ravenbot-tier4-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        db.pool().clone()
    }

    const TEAM: &str = r#"---
name: Test Team
description: tiny
bots:
  - name: Lead Bot
    title: Lead
    rank: lead
  - name: Worker Bot
    title: Worker
office:
  name: Test Office
  template: it-office
  goal: Ship it
routines:
  - name: Daily standup
    bot: Lead Bot
    schedule: "0 9 * * 1-5"
    instruction: Run standup
---
# Playbook
"#;

    #[tokio::test]
    async fn import_creates_bots_office_and_paused_routines() {
        let pool = temp_pool().await;
        let summary = import_team_into(&pool, TEAM).await.expect("import");

        assert_eq!(summary["bots"].as_array().unwrap().len(), 2);
        assert!(summary["office_id"].as_str().is_some());
        assert_eq!(summary["routines"].as_array().unwrap().len(), 1);

        // Bots actually exist.
        let bots = ravenbot_db::queries::BotQueries::list(&pool).await.unwrap();
        assert_eq!(bots.len(), 2);

        // The routine is created DISABLED (never runs unattended on import).
        let routines: Vec<(i64,)> = sqlx::query_as("SELECT is_enabled FROM routines")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(routines.len(), 1);
        assert_eq!(routines[0].0, 0, "imported routines must be paused");
    }

    #[tokio::test]
    async fn channels_roundtrip_instructions_and_roster() {
        let pool = temp_pool().await;
        // One bot to assign.
        let bot = Bot::new("ChanBot", "channel test");
        ravenbot_db::queries::BotQueries::insert(&pool, &bot)
            .await
            .unwrap();

        let mut channel = Channel::new("Work");
        channel.instructions = "Be concise.".to_string();
        channel.working_folder = Some("/tmp/work".to_string());
        ravenbot_db::queries::ChannelQueries::create(&pool, &channel)
            .await
            .unwrap();
        ravenbot_db::queries::ChannelQueries::set_bots(&pool, channel.id, &[bot.id])
            .await
            .unwrap();

        let list = ravenbot_db::queries::ChannelQueries::list(&pool)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        let got = &list[0];
        assert_eq!(got.name, "Work");
        assert_eq!(got.instructions, "Be concise.");
        assert_eq!(got.working_folder.as_deref(), Some("/tmp/work"));
        let bots = ravenbot_db::queries::ChannelQueries::bots(&pool, channel.id)
            .await
            .unwrap();
        assert_eq!(bots, vec![bot.id]);
    }

    #[tokio::test]
    async fn webhook_secret_lookup_only_returns_enabled() {
        let pool = temp_pool().await;
        let bot = Bot::new("HookBot", "hook test");
        ravenbot_db::queries::BotQueries::insert(&pool, &bot)
            .await
            .unwrap();
        let routine = Routine::new(bot.id, "Hooked", "0 9 * * *", "do work");
        sqlx::query(
            "INSERT INTO routines (id, bot_id, name, description, schedule, instruction, is_enabled, created_at, updated_at)
             VALUES (?, ?, ?, '', ?, ?, 1, datetime('now'), datetime('now'))",
        )
        .bind(routine.id.to_string())
        .bind(routine.bot_id.to_string())
        .bind(&routine.name)
        .bind(&routine.schedule)
        .bind(&routine.instruction)
        .execute(&pool)
        .await
        .unwrap();

        // Enabled → lookup finds it.
        ravenbot_db::queries::WebhookQueries::set(&pool, routine.id, Some("tok"), true)
            .await
            .unwrap();
        assert_eq!(
            ravenbot_db::queries::WebhookQueries::routine_for_secret(&pool, "tok")
                .await
                .unwrap(),
            Some(routine.id)
        );
        // Disabled → lookup misses even with the secret present.
        ravenbot_db::queries::WebhookQueries::set(&pool, routine.id, Some("tok"), false)
            .await
            .unwrap();
        assert_eq!(
            ravenbot_db::queries::WebhookQueries::routine_for_secret(&pool, "tok")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn default_agent_falls_back_to_the_first_orchestrator_and_is_pinned() {
        use ravenbot_db::queries::{AppSettingsQueries, BotQueries};

        let pool = temp_pool().await;
        assert!(
            find_default_agent(&pool).await.is_none(),
            "an empty fleet has no designer"
        );

        let mut lead = Bot::new("Architect", "designs offices");
        lead.is_orchestrator = true;
        BotQueries::insert(&pool, &lead).await.unwrap();

        let found = find_default_agent(&pool)
            .await
            .expect("fallback to the lead");
        assert_eq!(found.id, lead.id);
        let pinned = AppSettingsQueries::get(&pool, DEFAULT_AGENT_SETTING)
            .await
            .unwrap();
        assert_eq!(pinned, Some(lead.id.to_string()));

        // A second orchestrator does not steal the role while the pin is valid.
        let mut other = Bot::new("Other Lead", "another lead");
        other.is_orchestrator = true;
        BotQueries::insert(&pool, &other).await.unwrap();
        assert_eq!(find_default_agent(&pool).await.unwrap().id, lead.id);

        // Changing the pin changes the answer.
        AppSettingsQueries::set(&pool, DEFAULT_AGENT_SETTING, &other.id.to_string())
            .await
            .unwrap();
        assert_eq!(find_default_agent(&pool).await.unwrap().id, other.id);
    }

    /// Live proof that a name + one line of context becomes a real office.
    ///
    /// Ignored by default: needs the network and a working OpenCode Go key in
    /// the database. Run with:
    /// `RAVENBOT_DB=/path/to/ravenbot.db cargo test -p ravenbot live_office_blueprint -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "network + real OpenCode Go key in the database"]
    async fn live_office_blueprint_designs_a_team_from_a_name() {
        let db_path = std::env::var("RAVENBOT_DB").unwrap_or_else(|_| "ravenbot.db".to_string());
        let db = ravenbot_db::Database::new(&db_path)
            .await
            .expect("database");
        let runtime = ravenbot_runtime::Runtime::new(db.clone());

        let row: Option<(String,)> =
            sqlx::query_as("SELECT api_key FROM provider_keys WHERE provider = 'opencode-go'")
                .fetch_optional(db.pool())
                .await
                .expect("provider_keys query");
        let key = row.expect("opencode-go key must be configured").0;
        runtime
            .provider_manager()
            .lock()
            .await
            .set_api_key("opencode-go", key);

        let catalog = office_capability_catalog(&runtime).await;

        let test_model = std::env::var("RAVENBOT_TEST_MODEL")
            .unwrap_or_else(|_| "deepseek-v4.1-flash".to_string());
        let prompt = office_org::blueprint_prompt_with_catalog(
            "Nightwatch",
            None,
            "custom",
            "A small studio that assesses the security of web apps its clients own.",
            &catalog,
        );
        let text = runtime
            .complete_with_model(
                "opencode-go",
                &test_model,
                "You are the Chief of Staff and Office Architect. Output only valid JSON.",
                &prompt,
                6000,
            )
            .await
            .expect("the model must answer");
        eprintln!(
            "--- blueprint ---\n{text}\n--- catalog: {} skills / {} connectors / {} plugins ---",
            catalog.skills.len(),
            catalog.connectors.len(),
            catalog.plugins.len()
        );

        let bp = office_org::parse_blueprint_with_catalog(&text, Some(&catalog))
            .expect("a parseable blueprint");
        assert!(!bp.goal.trim().is_empty(), "a goal must be inferred");
        assert!(!bp.policy.trim().is_empty(), "a policy must be written");
        assert!(bp.roles.len() >= 2, "at least a lead and one specialist");
        assert!(
            office_org::blueprint_roles_are_usable(&bp.roles),
            "the roster must carry real mandates, not personas"
        );
        assert_eq!(
            bp.roles.iter().filter(|r| r.is_lead).count(),
            1,
            "exactly one lead"
        );

        let allowed_skills: std::collections::HashSet<&str> =
            catalog.skills.iter().map(|s| s.id.as_str()).collect();
        let allowed_mcp: std::collections::HashSet<&str> =
            catalog.connectors.iter().map(|s| s.id.as_str()).collect();
        for role in &bp.roles {
            for skill in &role.skills {
                assert!(
                    allowed_skills.contains(skill.as_str()),
                    "{} was assigned skill {skill}, which is not in the catalog",
                    role.name
                );
            }
            for server in &role.mcp_servers {
                assert!(
                    allowed_mcp.contains(server.as_str()),
                    "{} was assigned connector {server}, which is not in the catalog",
                    role.name
                );
            }
            eprintln!(
                "{}: {} skills, {} connectors, {} plugins",
                role.name,
                role.skills.len(),
                role.mcp_servers.len(),
                role.plugins.len()
            );
        }
    }

    // ── Projects editor ──────────────────────────────────────────────────
    //
    // These exercise the filesystem half of `write_workspace_file`, with a
    // freshly created directory per test. Nothing here names a real office:
    // the allowlist is whatever roots the caller passes, which is the property
    // that makes custom project folders safe.

    fn temp_workspace(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ravenbot-editor-{tag}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::canonicalize(path).unwrap()
    }

    #[test]
    fn editor_writes_only_inside_a_configured_workspace() {
        let office = temp_workspace("inside");
        let outside = temp_workspace("outside");
        let roots = vec![office.clone()];

        let file = office.join("src").join("main.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let saved =
            write_workspace_file_impl(&file, "fn main() {}\n", None, false, &roots).unwrap();
        assert_eq!(saved["status"].as_str(), Some("saved"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "fn main() {}\n");

        let escape = outside.join("secret.txt");
        let err = write_workspace_file_impl(&escape, "nope", None, false, &roots).unwrap_err();
        assert!(
            err.contains("outside every configured project workspace"),
            "{err}"
        );
        assert!(!escape.exists(), "the refused file must not be created");

        // `..` is resolved before the allowlist, so path arithmetic cannot
        // walk out of the office either.
        let traversal = office.join("src").join("..").join("..").join("escape.txt");
        let err = write_workspace_file_impl(&traversal, "nope", None, false, &roots).unwrap_err();
        assert!(
            err.contains("outside every configured project workspace"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(&office);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn editor_reports_a_concurrent_agent_write_and_can_overwrite() {
        let office = temp_workspace("conflict");
        let roots = vec![office.clone()];
        let file = office.join("README.md");
        std::fs::write(&file, "one\n").unwrap();
        let open_hash = file_version(b"one\n");

        // An agent writes after the editor opened the file.
        std::fs::write(&file, "agent wrote this\n").unwrap();
        let res =
            write_workspace_file_impl(&file, "mine\n", Some(&open_hash), false, &roots).unwrap();
        assert_eq!(res["status"].as_str(), Some("conflict"));
        assert_eq!(res["text"].as_str(), Some("agent wrote this\n"));
        let agent_hash = file_version(b"agent wrote this\n");
        assert_eq!(res["sha256"].as_str(), Some(agent_hash.as_str()));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "agent wrote this\n",
            "a conflict must not touch the agent's file"
        );

        // The explicit overwrite is the only path that wins.
        let saved =
            write_workspace_file_impl(&file, "mine\n", Some(&open_hash), true, &roots).unwrap();
        assert_eq!(saved["status"].as_str(), Some("saved"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine\n");

        let _ = std::fs::remove_dir_all(&office);
    }

    #[test]
    fn editor_treats_a_deleted_file_as_a_conflict_then_recreates_it_on_overwrite() {
        let office = temp_workspace("deleted");
        let roots = vec![office.clone()];
        let file = office.join("notes.md");
        std::fs::write(&file, "gone\n").unwrap();
        let open_hash = file_version(b"gone\n");
        std::fs::remove_file(&file).unwrap();

        let res =
            write_workspace_file_impl(&file, "back\n", Some(&open_hash), false, &roots).unwrap();
        assert_eq!(res["status"].as_str(), Some("conflict"));
        assert!(res["sha256"].is_null());
        assert!(!file.exists(), "a conflict must not recreate the file");

        let saved =
            write_workspace_file_impl(&file, "back\n", Some(&open_hash), true, &roots).unwrap();
        assert_eq!(saved["status"].as_str(), Some("saved"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "back\n");

        let _ = std::fs::remove_dir_all(&office);
    }

    #[test]
    fn editor_save_is_atomic_and_leaves_no_temp_files() {
        let office = temp_workspace("atomic");
        let file = office.join("app.py");
        std::fs::write(&file, "print('old')\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        }

        let saved = write_workspace_file_impl(
            &file,
            "print('new')\n",
            None,
            false,
            std::slice::from_ref(&office),
        )
        .unwrap();
        assert_eq!(saved["status"].as_str(), Some("saved"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "print('new')\n");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o640, "an atomic replace must preserve the file mode");
        }

        let leftovers: Vec<_> = std::fs::read_dir(&office)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".ravenbot-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temp file should survive a successful save"
        );

        let _ = std::fs::remove_dir_all(&office);
    }

    #[test]
    fn editor_refuses_binary_files_and_oversized_edits() {
        let office = temp_workspace("guards");
        let roots = [office.clone()];

        let binary = office.join("image.bin");
        let original = [0xff_u8, 0xfe, 0x00, 0x01];
        std::fs::write(&binary, original).unwrap();
        let err = write_workspace_file_impl(&binary, "text", None, false, &roots).unwrap_err();
        assert!(err.contains("binary"), "{err}");
        assert_eq!(std::fs::read(&binary).unwrap(), original);

        let big = office.join("big.txt");
        let oversized = "a".repeat(4 * 1024 * 1024 + 1);
        let err = write_workspace_file_impl(&big, &oversized, None, false, &roots).unwrap_err();
        assert!(err.contains("4MB"), "{err}");
        assert!(!big.exists());

        let _ = std::fs::remove_dir_all(&office);
    }

    #[test]
    fn editor_refuses_a_file_that_grew_past_the_limit_even_on_force() {
        let office = temp_workspace("grew");
        let file = office.join("log.txt");
        std::fs::write(&file, "small\n").unwrap();
        let open_hash = file_version(b"small\n");
        let oversized = "a".repeat(4 * 1024 * 1024 + 1);
        std::fs::write(&file, &oversized).unwrap();

        let err = write_workspace_file_impl(
            &file,
            "mine\n",
            Some(&open_hash),
            false,
            std::slice::from_ref(&office),
        )
        .unwrap_err();
        assert!(err.contains("larger than the 4MB"), "{err}");

        // Force means "ignore the version mismatch", not "ignore the size".
        let err =
            write_workspace_file_impl(&file, "mine\n", None, true, std::slice::from_ref(&office))
                .unwrap_err();
        assert!(err.contains("larger than the 4MB"), "{err}");
        assert_eq!(
            std::fs::metadata(&file).unwrap().len(),
            oversized.len() as u64,
            "the agent's file must survive the refusal"
        );

        let _ = std::fs::remove_dir_all(&office);
    }

    #[cfg(unix)]
    #[test]
    fn editor_cannot_follow_a_symlink_out_of_the_workspace() {
        let office = temp_workspace("link-office");
        let outside = temp_workspace("link-outside");
        let target = outside.join("secret.txt");
        std::fs::write(&target, "secret\n").unwrap();
        std::os::unix::fs::symlink(&target, office.join("link.txt")).unwrap();

        let err = write_workspace_file_impl(
            &office.join("link.txt"),
            "owned",
            None,
            false,
            std::slice::from_ref(&office),
        )
        .unwrap_err();
        assert!(
            err.contains("outside every configured project workspace"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "secret\n");

        let _ = std::fs::remove_dir_all(&office);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn editor_cannot_create_a_file_through_a_dangling_symlink() {
        let office = temp_workspace("dangling-office");
        let outside = temp_workspace("dangling-outside");
        let target = outside.join("created.txt");
        std::os::unix::fs::symlink(&target, office.join("link.txt")).unwrap();

        let err = write_workspace_file_impl(
            &office.join("link.txt"),
            "owned",
            None,
            false,
            std::slice::from_ref(&office),
        )
        .unwrap_err();
        assert!(
            err.contains("Cannot resolve file")
                || err.contains("outside every configured project workspace"),
            "{err}"
        );
        assert!(
            !target.exists(),
            "a dangling link must not create a file outside the workspace"
        );

        let _ = std::fs::remove_dir_all(&office);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn viewer_reads_inside_a_workspace_and_refuses_outside() {
        let office = temp_workspace("read-inside");
        let outside = temp_workspace("read-outside");
        let file = office.join("src").join("lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "pub fn audit() {}\n").unwrap();

        let res = read_workspace_file_impl(&file, 4 * 1024 * 1024, std::slice::from_ref(&office))
            .unwrap();
        assert_eq!(res["text"].as_str(), Some("pub fn audit() {}\n"));
        assert_eq!(res["truncated"].as_bool(), Some(false));
        let expected_hash = file_version(b"pub fn audit() {}\n");
        assert_eq!(res["sha256"].as_str(), Some(expected_hash.as_str()));

        let secret = outside.join("id_rsa");
        std::fs::write(&secret, "private\n").unwrap();
        let err = read_workspace_file_impl(&secret, 4 * 1024 * 1024, std::slice::from_ref(&office))
            .unwrap_err();
        assert!(
            err.contains("outside every configured project workspace"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(&office);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn viewer_cannot_follow_a_symlink_out_of_the_workspace() {
        let office = temp_workspace("read-link-office");
        let outside = temp_workspace("read-link-outside");
        let secret = outside.join("id_rsa");
        std::fs::write(&secret, "private\n").unwrap();
        std::os::unix::fs::symlink(&secret, office.join("innocent.txt")).unwrap();

        let err = read_workspace_file_impl(
            &office.join("innocent.txt"),
            4 * 1024 * 1024,
            std::slice::from_ref(&office),
        )
        .unwrap_err();
        assert!(
            err.contains("outside every configured project workspace"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(&office);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn viewer_truncates_on_a_char_boundary_and_hashes_the_full_file() {
        let office = temp_workspace("read-truncate");
        let file = office.join("long.txt");
        // The cap lands in the middle of the two-byte `é`, which is exactly the
        // boundary that would panic a naive byte slice.
        let content = format!("{}é{}", "a".repeat(1023), "b".repeat(50));
        std::fs::write(&file, &content).unwrap();

        let res = read_workspace_file_impl(&file, 1024, std::slice::from_ref(&office)).unwrap();
        assert_eq!(res["truncated"].as_bool(), Some(true));
        let expected_hash = file_version(content.as_bytes());
        assert_eq!(res["sha256"].as_str(), Some(expected_hash.as_str()));
        let text = res["text"].as_str().unwrap();
        assert!(text.starts_with(&"a".repeat(1023)));
        assert!(text.ends_with("… truncated …"));
        assert!(
            !text.contains('é'),
            "a split character must not become mojibake"
        );

        let _ = std::fs::remove_dir_all(&office);
    }

    async fn temp_db(tag: &str) -> (ravenbot_db::Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("ravenbot-{tag}-{}.db", uuid::Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        (db, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn visual_pages_are_served_with_a_sandboxed_csp() {
        let url = register_visual("<html><body><h1>visual</h1></body></html>".to_string())
            .await
            .expect("register visual");
        let id = url.rsplit('/').next().unwrap().to_string();
        let request = tauri::http::Request::builder()
            .uri(format!("/{id}"))
            .body(Vec::new())
            .unwrap();
        let response = visual_protocol_response(&request);
        assert_eq!(response.status(), 200);
        let csp = response
            .headers()
            .get("Content-Security-Policy")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(csp.contains("script-src 'unsafe-inline'"), "csp={csp}");
        let body = String::from_utf8(response.into_body()).expect("utf8 body");
        assert!(body.contains("<h1>visual</h1>"));

        release_visual(id.clone()).await;
        let missing = tauri::http::Request::builder()
            .uri(format!("/{id}"))
            .body(Vec::new())
            .unwrap();
        assert_eq!(visual_protocol_response(&missing).status(), 404);
    }

    #[test]
    fn media_mime_mapping_covers_the_previewable_types() {
        use std::path::Path;
        assert_eq!(media_mime_for(Path::new("out.mp4")), Some("video/mp4"));
        assert_eq!(media_mime_for(Path::new("cover.PNG")), Some("image/png"));
        assert_eq!(media_mime_for(Path::new("voice.wav")), Some("audio/wav"));
        assert_eq!(
            media_mime_for(Path::new("deck.pdf")),
            Some("application/pdf")
        );
        assert_eq!(media_mime_for(Path::new("archive.zip")), None);
        assert_eq!(media_mime_for(Path::new("no-extension")), None);
    }

    #[test]
    fn a_workspace_without_a_code_root_is_a_bootstrap() {
        let root = std::env::temp_dir().join(format!("ravenbot-bootstrap-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/design.md"), "design").unwrap();
        let mut room = ravenbot_core::ChatRoom::new("Bootstrap", "test", "custom");
        room.project_folders = vec![root.to_string_lossy().to_string()];

        assert!(!office_has_implementation(Some(&room)));
        let snapshot = office_workspace_snapshot(Some(&room));
        assert!(snapshot.contains("Implementation: MISSING"), "{snapshot}");
        assert!(snapshot.contains("docs/"), "{snapshot}");

        std::fs::create_dir_all(root.join("app")).unwrap();
        assert!(
            !office_has_implementation(Some(&room)),
            "an empty app/ directory is not an implementation"
        );
        std::fs::write(root.join("app/main.py"), "print('hi')").unwrap();
        assert!(office_has_implementation(Some(&room)));
        assert!(
            office_workspace_snapshot(Some(&room)).contains("Implementation: present"),
            "a code root must flip the bootstrap switch"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workspace_fingerprint_changes_only_with_real_edits() {
        let root = std::env::temp_dir().join(format!("ravenbot-fingerprint-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/main.py"), "print('hi')").unwrap();
        let mut room = ravenbot_core::ChatRoom::new("Fingerprint", "test", "custom");
        room.project_folders = vec![root.to_string_lossy().to_string()];

        let first = office_workspace_fingerprint(Some(&room)).expect("fingerprint");
        assert_eq!(
            first,
            office_workspace_fingerprint(Some(&room)).expect("fingerprint"),
            "the same tree must fingerprint identically"
        );

        // Build output and virtualenvs churn constantly; they are not progress.
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join("target/debug/artifact"), "binary").unwrap();
        std::fs::create_dir_all(root.join(".venv/bin")).unwrap();
        std::fs::write(root.join(".venv/bin/python"), "python").unwrap();
        assert_eq!(
            first,
            office_workspace_fingerprint(Some(&room)).expect("fingerprint"),
            "ignored directories must not fake progress"
        );

        std::fs::write(root.join("app/main.py"), "print('hello world')").unwrap();
        let edited = office_workspace_fingerprint(Some(&room)).expect("fingerprint");
        assert_ne!(first, edited, "an edit must change the fingerprint");

        std::fs::write(root.join("app/services.py"), "def charge(): pass").unwrap();
        let added = office_workspace_fingerprint(Some(&room)).expect("fingerprint");
        assert_ne!(edited, added, "a new file must change the fingerprint");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn office_queue_is_fifo_and_pausable() {
        let (db, path) = temp_db("office-queue").await;
        let room = ravenbot_core::ChatRoom::new("Queue", "test", "custom");
        ravenbot_db::queries::ChatRoomQueries::create(db.pool(), &room)
            .await
            .unwrap();

        // The reply target must survive the queue: the message it quotes may
        // not exist as part of the transcript until the turn actually starts.
        let reply_id = Uuid::new_v4();
        let first = enqueue_office_message(
            &db,
            room.id,
            "first task".to_string(),
            "first task".to_string(),
            Vec::new(),
            Some(reply_id),
        )
        .await
        .unwrap();
        let second = enqueue_office_message(
            &db,
            room.id,
            "second task".to_string(),
            "second task".to_string(),
            Vec::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(office_queue_pending_count(&db, room.id).await, 2);

        let listed = load_office_queue(&db, room.id).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, first.id.to_string());
        assert_eq!(listed[1].id, second.id.to_string());

        let claimed = claim_next_office_queue_item(&db, room.id)
            .await
            .unwrap()
            .expect("first claim");
        assert_eq!(claimed.id, first.id);
        assert_eq!(claimed.reply_to_id, Some(reply_id));
        assert_eq!(
            load_office_queue(&db, room.id).await.unwrap()[0].state,
            "running"
        );

        finish_office_queue_item(&db, first.id, true, None).await;
        let listed = load_office_queue(&db, room.id).await.unwrap();
        assert_eq!(listed.len(), 1, "a completed item leaves the queue view");
        assert_eq!(listed[0].id, second.id.to_string());

        cancel_queued_message(&db, room.id, second.id)
            .await
            .expect("cancel pending");
        assert!(!office_queue_has_pending(&db, room.id).await);
        assert!(load_office_queue(&db, room.id).await.unwrap().is_empty());

        persist_office_queue_paused(&db, room.id, true)
            .await
            .unwrap();
        assert!(office_queue_paused(&db, room.id).await);
        persist_office_queue_paused(&db, room.id, false)
            .await
            .unwrap();
        assert!(!office_queue_paused(&db, room.id).await);

        drop(db);
        cleanup_db(&path);
    }

    #[test]
    fn attachment_sizes_are_decoded_bytes_not_base64_characters() {
        assert_eq!(decoded_base64_len(""), 0);
        assert_eq!(decoded_base64_len("aGk="), 2, "hi");
        assert_eq!(decoded_base64_len("aGVsbG8="), 5, "hello");
        assert_eq!(decoded_base64_len("aGVsbG8h"), 6, "hello!");
    }

    #[test]
    fn catalog_installs_only_accept_directory_urls_as_fallback() {
        // A resolved catalog id wins over whatever the renderer sent.
        assert_eq!(
            resolve_catalog_install_url(
                "stripe",
                Some("https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json".to_string()),
                Some("https://evil.example/spec.json".to_string()),
            )
            .unwrap(),
            "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json"
        );

        // A directory entry stays installable after an index refresh.
        assert_eq!(
            resolve_catalog_install_url(
                "apisguru:notion.com",
                None,
                Some("https://api.apis.guru/v2/specs/notion.com/1.0.0/openapi.json".to_string()),
            )
            .unwrap(),
            "https://api.apis.guru/v2/specs/notion.com/1.0.0/openapi.json"
        );

        // The catalog path is not an arbitrary-URL proxy.
        assert!(resolve_catalog_install_url(
            "apisguru:notion.com",
            None,
            Some("https://evil.example/spec.json".to_string()),
        )
        .is_err());
        assert!(resolve_catalog_install_url("unknown", None, None).is_err());
    }

    #[tokio::test]
    async fn openapi_import_follows_manifest_and_parses_real_operations() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn serve(body: String) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listener");
            let address = listener.local_addr().expect("address");
            tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut buffer = [0u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
            format!("http://{}", address)
        }

        let spec = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Billing API", "description": "Ledger operations"},
            "servers": [{"url": "https://api.billing.test"}],
            "components": {"securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}}},
            "paths": {
                "/invoices": {
                    "get": {
                        "operationId": "listInvoices",
                        "summary": "List invoices",
                        "description": "Return invoices for a customer"
                    }
                }
            }
        })
        .to_string();
        let spec_base = serve(spec).await;
        let spec_url = format!("{spec_base}/spec.json");
        let manifest = serde_json::json!({
            "schema_version": "v1",
            "name_for_human": "Billing",
            "api": {"url": spec_url},
            "logo_url": "https://api.billing.test/logo.png"
        })
        .to_string();
        let manifest_base = serve(manifest).await;
        let manifest_url = format!("{manifest_base}/ai-plugin.json");

        let (db, path) = temp_db("openapi-import").await;
        let imported = import_openapi_document(&db, &manifest_url)
            .await
            .expect("import");
        assert_eq!(imported.summary.name, "Billing API");
        assert_eq!(imported.summary.operations, 1);
        assert!(imported.summary.requires_auth);
        assert_eq!(
            imported.summary.credential_header_hint.as_deref(),
            Some("Authorization")
        );
        assert_eq!(imported.summary.source_url.as_deref(), Some(spec_url.as_str()));
        assert_eq!(imported.operations.len(), 1);
        assert_eq!(imported.operations[0].method, "GET");
        assert!(imported.operations[0].id.starts_with("billing_api__"));

        // Some public documents omit their security scheme. A curated
        // catalog entry can supply the known requirement without inventing a
        // parser result.
        let public_spec = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Public API", "description": "No security block"},
            "servers": [{"url": "https://api.public.test"}],
            "paths": {
                "/status": {
                    "get": {"operationId": "status", "summary": "Status"}
                }
            }
        })
        .to_string();
        let public_url = format!("{}/public.json", serve(public_spec).await);
        let overridden =
            import_openapi_document_with_auth(&db, &public_url, Some(true), Some("Authorization"))
                .await
                .expect("overridden import");
        assert_eq!(overridden.summary.name, "Public API");
        assert!(overridden.summary.requires_auth);
        assert_eq!(
            overridden.summary.credential_header_hint.as_deref(),
            Some("Authorization")
        );

        drop(db);
        cleanup_db(&path);
    }

    #[tokio::test]
    #[ignore = "downloads live OpenAPI and Swagger specs"]
    async fn installs_real_openapi_and_swagger_directory_specs() {
        let (db, path) = temp_db("live-catalog-install").await;

        let notion = import_openapi_document(
            &db,
            "https://api.apis.guru/v2/specs/notion.com/1.0.0/openapi.json",
        )
        .await
        .expect("notion import");
        assert_eq!(notion.summary.name, "Notion API");
        assert!(notion.summary.operations > 5, "notion operations: {}", notion.summary.operations);
        assert_eq!(notion.operations.len(), notion.summary.operations as usize);
        assert_eq!(
            notion.summary.source_url.as_deref(),
            Some("https://api.apis.guru/v2/specs/notion.com/1.0.0/openapi.json")
        );

        let legacy = import_openapi_document(
            &db,
            "https://api.apis.guru/v2/specs/1forge.com/0.0.1/swagger.json",
        )
        .await
        .expect("swagger 2 import");
        assert_eq!(legacy.summary.name, "1Forge Finance APIs");
        assert!(legacy.summary.operations > 0, "1forge operations: {}", legacy.summary.operations);
        assert!(legacy.operations.iter().any(|operation| operation.method == "GET"));

        // The imported operations are durable in the store, not just in the
        // returned struct.
        let store = ravenbot_plugins::store::PluginStore::new(db.pool().clone());
        let summaries = store.list_summaries(None).await.expect("summaries");
        assert!(summaries.iter().any(|summary| summary.name == "Notion API" && summary.operations > 5));
        assert!(summaries.iter().any(|summary| summary.name == "1Forge Finance APIs" && summary.operations > 0));

        drop(store);
        drop(db);
        cleanup_db(&path);
    }

    #[tokio::test]
    async fn reply_targets_are_confined_to_their_thread() {
        use ravenbot_db::queries::{BotQueries, MessageQueries, ThreadQueries};

        let (db, path) = temp_db("reply-scope").await;
        let bot = ravenbot_core::Bot::new("Reply Bot", "reply scope test");
        BotQueries::insert(db.pool(), &bot).await.unwrap();

        let first = ravenbot_core::Thread::new(bot.id, "first");
        let second = ravenbot_core::Thread::new(bot.id, "second");
        ThreadQueries::create(db.pool(), &first).await.unwrap();
        ThreadQueries::create(db.pool(), &second).await.unwrap();

        let message = ravenbot_core::Message::user(first.id, "quote me");
        MessageQueries::insert(db.pool(), &message).await.unwrap();

        assert!(message_in_thread(db.pool(), message.id, first.id)
            .await
            .unwrap());
        assert!(!message_in_thread(db.pool(), message.id, second.id)
            .await
            .unwrap());

        drop(db);
        cleanup_db(&path);
    }

    #[tokio::test]
    async fn office_budget_counts_only_this_offices_spend() {
        let (db, path) = temp_db("office-budget").await;
        let bot = ravenbot_core::Bot::new("Budget Bot", "budget test");
        let other = ravenbot_core::Bot::new("Other Bot", "budget test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        ravenbot_db::queries::BotQueries::insert(db.pool(), &other)
            .await
            .unwrap();
        let bot_id = bot.id;
        let other_bot = other.id;
        let mut room = ravenbot_core::ChatRoom::new("Budget", "test", "custom");
        room.budget = Some(1.0);
        ravenbot_db::queries::ChatRoomQueries::create(db.pool(), &room)
            .await
            .unwrap();
        let budgets = ravenbot_governance::BudgetManager::new(db.pool().clone());

        // Lifetime spend before the office existed is the baseline, not this
        // office's spend.
        budgets.record_usage(bot_id, 10, 0.25).await.unwrap();
        assert_eq!(
            office_budget_spent(&db, &room, &[bot_id]).await,
            0.0,
            "the first check seeds the baseline"
        );

        budgets.record_usage(bot_id, 10, 0.25).await.unwrap();
        let spent = office_budget_spent(&db, &room, &[bot_id]).await;
        assert!((spent - 0.25).abs() < f64::EPSILON, "spent={spent}");

        // A bot that has never been part of this office does not count until it
        // joins, and then only its spend after joining counts.
        budgets.record_usage(other_bot, 10, 0.5).await.unwrap();
        let spent = office_budget_spent(&db, &room, &[bot_id]).await;
        assert!((spent - 0.25).abs() < f64::EPSILON, "spent={spent}");
        let _ = office_budget_spent(&db, &room, &[bot_id, other_bot]).await;
        budgets.record_usage(other_bot, 10, 0.5).await.unwrap();
        let spent = office_budget_spent(&db, &room, &[bot_id, other_bot]).await;
        assert!((spent - 0.75).abs() < f64::EPSILON, "spent={spent}");
        assert!(
            office_budget_exceeded(&db, &room, &[bot_id, other_bot])
                .await
                .is_none(),
            "0.75 of 1.00 is still allowed"
        );

        budgets.record_usage(bot_id, 10, 0.5).await.unwrap();
        let (spent, limit) = office_budget_exceeded(&db, &room, &[bot_id, other_bot])
            .await
            .expect("the ceiling must be enforced");
        assert!((spent - 1.25).abs() < f64::EPSILON, "spent={spent}");
        assert!((limit - 1.0).abs() < f64::EPSILON, "limit={limit}");

        drop(db);
        cleanup_db(&path);
    }

    #[tokio::test]
    async fn office_phase_marker_round_trips_and_clears() {
        let (db, path) = temp_db("office-phase").await;
        let chatroom_id = Uuid::new_v4();
        mark_office_phase(&db, chatroom_id, "planning").await;
        let phase = ravenbot_db::queries::AppSettingsQueries::get(
            db.pool(),
            &office_phase_key(chatroom_id),
        )
        .await
        .unwrap();
        assert_eq!(phase.as_deref(), Some("planning"));

        clear_office_phase(&db, chatroom_id).await;
        let phase = ravenbot_db::queries::AppSettingsQueries::get(
            db.pool(),
            &office_phase_key(chatroom_id),
        )
        .await
        .unwrap();
        assert_eq!(phase.as_deref(), Some(""));

        drop(db);
        cleanup_db(&path);
    }

    #[test]
    fn an_office_rejects_a_second_turn_while_one_is_active() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        assert!(!office_is_active(first));
        let guard = OfficeRunGuard::try_new(first).expect("first claim");
        assert!(office_is_active(first));
        assert!(
            OfficeRunGuard::try_new(first).is_none(),
            "the same room must not start two turns"
        );
        assert!(!office_is_active(second), "rooms must be independent");
        drop(guard);
        assert!(!office_is_active(first), "the guard must release the room");
    }

    #[tokio::test]
    async fn office_board_projection_survives_the_window() {
        let (db, path) = temp_db("office-board").await;
        let thread_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        let node_thread = Uuid::new_v4();

        apply_office_board_event(
            &db,
            thread_id,
            &serde_json::json!({
                "kind": "plan_ready",
                "goal": "Ship the audit",
                "nodes": [{
                    "node_id": "node-1",
                    "bot_id": bot_id.to_string(),
                    "label": "Write the schema",
                    "depends_on": [],
                }],
            }),
        )
        .await;

        apply_office_board_event(
            &db,
            thread_id,
            &serde_json::json!({
                "kind": "node_open",
                "node_id": "node-1",
                "bot_id": bot_id.to_string(),
                "node_thread_id": node_thread.to_string(),
            }),
        )
        .await;

        apply_office_board_event(
            &db,
            thread_id,
            &serde_json::json!({
                "kind": "node_finished",
                "node_id": "node-1",
                "state": "done",
                "preview": "schema.md written",
            }),
        )
        .await;

        let raw = ravenbot_db::queries::AppSettingsQueries::get(
            db.pool(),
            &format!("office_board:{}", thread_id),
        )
        .await
        .unwrap()
        .expect("board snapshot");
        let board: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(board["goal"].as_str(), Some("Ship the audit"));
        let node = &board["nodes"][0];
        assert_eq!(node["state"].as_str(), Some("done"));
        assert_eq!(node["preview"].as_str(), Some("schema.md written"));
        let node_thread_text = node_thread.to_string();
        assert_eq!(
            node["nodeThreadId"].as_str(),
            Some(node_thread_text.as_str())
        );

        drop(db);
        cleanup_db(&path);
    }

    #[tokio::test]
    async fn offices_are_autonomous_by_default_and_can_be_paused() {
        let (db, path) = temp_db("office-autonomy").await;
        let chatroom_id = Uuid::new_v4();
        assert!(office_autonomy_enabled(&db, chatroom_id).await);
        ravenbot_db::queries::AppSettingsQueries::set(
            db.pool(),
            &office_autonomy_key(chatroom_id),
            "0",
        )
        .await
        .unwrap();
        assert!(!office_autonomy_enabled(&db, chatroom_id).await);

        drop(db);
        cleanup_db(&path);
    }
}

#[cfg(test)]
mod browser_dock_takeover_tests {
    use super::*;

    #[test]
    fn takeover_round_trips_and_starts_released() {
        set_browser_dock_takeover(false);
        assert!(!browser_dock_takeover_enabled());
        assert!(set_browser_dock_takeover(true));
        assert!(browser_dock_takeover_enabled());
        assert!(!set_browser_dock_takeover(false));
        assert!(!browser_dock_takeover_enabled());
    }
}

#[cfg(test)]
mod browser_dock_tests {
    use super::*;

    #[test]
    fn only_http_and_https_are_loadable() {
        assert!(parse_browser_dock_url("https://example.com/a").is_ok());
        assert!(parse_browser_dock_url("http://localhost:1420").is_ok());
        assert!(parse_browser_dock_url("file:///etc/passwd").is_err());
        assert!(parse_browser_dock_url("javascript:alert(1)").is_err());
        assert!(parse_browser_dock_url("   ").is_err());
    }

    #[test]
    fn bounds_are_clamped_to_a_usable_rectangle() {
        let tiny = browser_dock_size(&BrowserDockBounds {
            x: -40.0,
            y: -10.0,
            width: 1.0,
            height: 1.0,
        });
        assert_eq!(tiny.width, 180.0);
        assert_eq!(tiny.height, 120.0);

        let position = browser_dock_position(&BrowserDockBounds {
            x: -40.0,
            y: -10.0,
            width: 500.0,
            height: 400.0,
        });
        assert_eq!(position.x, 0.0);
        assert_eq!(position.y, 0.0);
    }
}
