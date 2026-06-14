// Amdion — Tauri app setup and command registration.
//
// Amdion lives in the macOS menu bar (an Accessory app — no Dock icon). The
// window starts hidden and is summoned by clicking the tray icon (panel drops
// anchored under it) or pressing ⌘⇧Space. It auto-hides when it loses focus, so
// it stays ephemeral. First run shows the window for onboarding.

mod bridge_ws;
mod classify;
mod commands;
mod config;
mod db;
mod gemini;
mod sensing;
mod state;

use state::AppState;
use std::time::{Duration, Instant};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, PhysicalPosition, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Summon shortcut: toggles the panel (show+focus, or hide if already up).
const SUMMON_SHORTCUT: &str = "CommandOrControl+Shift+Space";

/// Tray icon id, used to look the icon's screen rect back up for positioning.
const TRAY_ID: &str = "amdion-tray";

/// The point the panel should drop from for a given menu-bar icon rect:
/// horizontally centered on the icon, vertically at its bottom edge.
fn anchor_from_rect(rect: &tauri::Rect, scale: f64) -> (f64, f64) {
    let pos = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    (pos.x + size.width / 2.0, pos.y + size.height)
}

/// Current anchor point under the tray icon, if the platform reports its rect.
fn tray_anchor(app: &tauri::AppHandle, scale: f64) -> Option<(f64, f64)> {
    let rect = app.tray_by_id(TRAY_ID)?.rect().ok().flatten()?;
    Some(anchor_from_rect(&rect, scale))
}

/// Toggle the panel for the ⌘⇧Space summon: hide if it's already up, otherwise
/// drop it anchored under the tray icon (falling back to its last position if
/// the tray rect is unavailable).
fn summon_panel(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let up = win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false);
    if up {
        let _ = win.hide();
        return;
    }
    let scale = win.scale_factor().unwrap_or(1.0);
    match tray_anchor(app, scale) {
        Some((x, y)) => show_panel_under(&win, x, y),
        None => {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// Position the panel just under a menu-bar anchor point and show it.
///
/// Multi-monitor was finicky: the old code leaned on `monitor_from_point`, which
/// can miss on macOS (the platform Y is flipped) and then fell back to the
/// window's *current* monitor — so the panel opened on whatever display it last
/// used instead of the one the user clicked (the reported left/right swap).
/// Instead we pick the display whose horizontal span contains the anchor's X:
/// menu-bar icons sit at the top of the active display, and X alone uniquely
/// identifies a side-by-side display regardless of how Y is reported. We then
/// size the window in *that* display's DPI (it can differ) and clamp the drop
/// point to its bounds.
fn show_panel_under(win: &tauri::WebviewWindow, anchor_x: f64, anchor_y: f64) {
    let monitors = win.available_monitors().unwrap_or_default();
    let target = monitors
        .iter()
        .find(|m| {
            let left = m.position().x as f64;
            left <= anchor_x && anchor_x < left + m.size().width as f64
        })
        .cloned()
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());

    // Window width in the TARGET display's pixels: derive the logical width from
    // the current size/scale, then re-scale, since the target DPI may differ
    // from the display the window last lived on.
    let cur_scale = win.scale_factor().unwrap_or(1.0);
    let logical_w = win
        .outer_size()
        .map(|s| s.width as f64 / cur_scale)
        .unwrap_or(420.0);

    let (mut x, mut y) = (anchor_x - logical_w / 2.0, anchor_y + 6.0);
    if let Some(m) = &target {
        let scale = m.scale_factor();
        let w = logical_w * scale;
        let left = m.position().x as f64;
        let right = left + m.size().width as f64;
        let top = m.position().y as f64;
        let min_x = left + 8.0;
        let max_x = (right - w - 8.0).max(min_x);
        x = (anchor_x - w / 2.0).clamp(min_x, max_x);
        // Drop just under this display's menu bar. Trust the icon's Y when it's
        // sane, but never let a flipped/odd Y fling the panel off-screen.
        y = (anchor_y + 6.0).clamp(top + 4.0, top + 140.0 * scale);
    }
    let _ = win.set_position(PhysicalPosition::new(x as i32, y as i32));
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([SUMMON_SHORTCUT])
                .expect("failed to register summon shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    summon_panel(app);
                })
                .build(),
        )
        .manage(AppState::default())
        .setup(|app| {
            // Menu-bar app: no Dock icon, no app menu. (The bundled Info.plist
            // also sets LSUIElement so there isn't even a launch-time flash.)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Launch-at-login: keep the OS login item in sync with the saved
            // preference (on by default; toggled in Settings → Advanced).
            {
                use tauri_plugin_autostart::ManagerExt;
                let want = config::read_config().autostart;
                let mgr = app.autolaunch();
                let _ = if want { mgr.enable() } else { mgr.disable() };
            }

            // Open the SQLite event store BEFORE the bridge spawns: the
            // extension can send activity the instant it connects, and
            // `route_event` persists it via `try_state::<Db>()` — which would
            // silently return `None` (dropping events) if the bridge raced ahead
            // of `manage`.
            app.manage(db::Db::new());

            // Host the localhost WebSocket bridge the Chrome extension connects
            // to. Spawned on Tauri's async runtime; binds loopback only.
            {
                let st = app.state::<AppState>();
                let handle = app.handle().clone();
                let tx = st.bridge_tx.clone();
                let token = st.bridge_token.clone();
                let conns = st.ext_connections.clone();
                tauri::async_runtime::spawn(bridge_ws::serve(handle, tx, token, conns));
            }

            // Sensing engine: a background thread polling OS idle + frontmost app
            // into the same event store (macOS only; a no-op stub elsewhere).
            sensing::start(app.handle());

            let win = app.get_webview_window("main").expect("main window missing");

            // Auto-hide when the panel loses focus (click away → vanish).
            {
                let w = win.clone();
                let handle = app.handle().clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        if let Some(st) = handle.try_state::<AppState>() {
                            *st.last_hide.lock().unwrap() = Some(Instant::now());
                        }
                        let _ = w.hide();
                    }
                });
            }

            // Tray icon: left-click toggles the panel; right-click shows a menu.
            let open_i = MenuItemBuilder::with_id("open", "Open Amdion").build(app)?;
            let quit_i = MenuItemBuilder::with_id("quit", "Quit Amdion").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&open_i, &quit_i]).build()?;

            // The two-tone hourglass mark — black top, white bottom on a neutral
            // disc, i.e. the inside of the Amdion logo. Rendered in COLOR (not a
            // macOS template, which would flatten it to a single tint) so both
            // tones show; the disc keeps it legible on a light or dark menu bar.
            let tray_icon = tauri::include_image!("icons/tray.png");

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(tray_icon)
                .icon_as_template(false)
                .tooltip("Amdion")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let Some(win) = app.get_webview_window("main") else {
                            return;
                        };
                        // If the panel just auto-hid from this same click's blur,
                        // treat the click as "close" and leave it hidden.
                        let recently_hidden = app
                            .try_state::<AppState>()
                            .and_then(|st| *st.last_hide.lock().unwrap())
                            .map(|t| t.elapsed() < Duration::from_millis(400))
                            .unwrap_or(false);
                        if recently_hidden {
                            if let Some(st) = app.try_state::<AppState>() {
                                *st.last_hide.lock().unwrap() = None;
                            }
                            let _ = win.hide();
                        } else {
                            let scale = win.scale_factor().unwrap_or(1.0);
                            let (x, y) = anchor_from_rect(&rect, scale);
                            show_panel_under(&win, x, y);
                        }
                    }
                })
                .build(app)?;

            // First run → expand to full screen so onboarding is prominent;
            // it retreats into the menu bar when the user finishes.
            if !config::read_config().onboarding_complete {
                commands::session::expand_window(&win);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Apps (state-only in Step 0/1; Step 2 drives real Chrome)
            commands::browser::open_app,
            commands::browser::switch_tab,
            commands::browser::close_tab,
            commands::browser::go_home,
            commands::browser::toggle_sidebar,
            commands::browser::toggle_right_sidebar,
            commands::browser::get_state,
            // Front door: intent + panel
            commands::session::set_intent,
            commands::session::hide_panel,
            commands::session::expand_for_onboarding,
            commands::session::retreat_to_menubar,
            // Config
            commands::config::get_config,
            commands::config::save_config,
            // Observer (Step 3): typed daily stats over the event store
            commands::observer::get_daily_summary,
            commands::observer::get_sessions,
            // Mac tuning
            commands::tuning::list_mac_tweaks,
            commands::tuning::apply_mac_tuning,
            commands::tuning::revert_mac_tuning,
            commands::tuning::open_settings_pane,
            // AI chat + transcription
            commands::chat::send_chat_message,
            commands::chat::transcribe_audio,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Write a clean `shutdown` marker so the classifier can tell a quit
            // from a crash (a crash leaves the trailing block open; see
            // `sensing`/`classify`). Idempotent across ExitRequested + Exit.
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                sensing::on_exit(app);
            }
        });
}
