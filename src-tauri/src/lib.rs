// Amdion — Tauri app setup and command registration.
//
// Amdion lives in the macOS menu bar (an Accessory app — no Dock icon). The
// window starts hidden and is summoned by clicking the tray icon (panel drops
// anchored under it) or pressing ⌘⇧Space. It auto-hides when it loses focus, so
// it stays ephemeral. First run shows the window for onboarding.

mod commands;
mod config;
mod gemini;
mod state;

use state::AppState;
use std::time::{Duration, Instant};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, PhysicalPosition, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Summon shortcut: toggles the panel (show+focus, or hide if already up).
const SUMMON_SHORTCUT: &str = "CommandOrControl+Shift+Space";

/// Show/hide the panel based on its current state (used by the shortcut).
fn toggle_panel(win: &tauri::WebviewWindow) {
    let up = win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false);
    if up {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Position the panel centered under a menu-bar anchor point and show it,
/// clamped to stay on the current monitor.
fn show_panel_under(win: &tauri::WebviewWindow, anchor_x: f64, anchor_y: f64) {
    if let Ok(size) = win.outer_size() {
        let w = size.width as f64;
        let mut x = anchor_x - w / 2.0;
        let y = anchor_y + 6.0;
        let monitor = win
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| win.primary_monitor().ok().flatten());
        if let Some(m) = monitor {
            let left = m.position().x as f64;
            let right = left + m.size().width as f64;
            if x < left + 8.0 {
                x = left + 8.0;
            }
            if x + w > right - 8.0 {
                x = right - w - 8.0;
            }
        }
        let _ = win.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([SUMMON_SHORTCUT])
                .expect("failed to register summon shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if let Some(win) = app.get_webview_window("main") {
                        toggle_panel(&win);
                    }
                })
                .build(),
        )
        .manage(AppState::default())
        .setup(|app| {
            // Menu-bar app: no Dock icon, no app menu.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

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
            let icon = app.default_window_icon().expect("default icon").clone();

            TrayIconBuilder::with_id("amdion-tray")
                .icon(icon)
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
                        position,
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
                            show_panel_under(&win, position.x, position.y);
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
            // Mac tuning
            commands::tuning::list_mac_tweaks,
            commands::tuning::apply_mac_tuning,
            commands::tuning::revert_mac_tuning,
            commands::tuning::open_settings_pane,
            // AI chat + transcription
            commands::chat::send_chat_message,
            commands::chat::transcribe_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
