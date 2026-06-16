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
use tauri::{LogicalPosition, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Default summon shortcut: toggles the panel (show+focus, or hide if already
/// up). User-rebindable at runtime (see `commands::shortcut`); this is the
/// binding registered at launch and the fallback if a custom one won't bind.
const SUMMON_SHORTCUT: &str = "CommandOrControl+Shift+Space";

/// Tray icon id, used to look the icon's screen rect back up for positioning.
const TRAY_ID: &str = "amdion-tray";

/// The point the panel should drop from for a given menu-bar icon rect:
/// horizontally centered on the icon, vertically at its bottom edge.
///
/// Returns *physical* pixels. On macOS the tray rect is reported as the icon's
/// macOS point coordinates multiplied by the *tray display's* backing scale, so
/// these values live in that display's physical space — `show_panel_under` turns
/// them back into logical points once it knows which display the icon is on.
/// (`rect.position` is already a `Physical` variant here, so `scale` is a no-op;
/// kept for call-site symmetry.)
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

/// Does `m`'s logical frame contain the point `(px, py)`? A monitor's logical
/// frame — its physical position/size ÷ its own scale — is its true macOS point
/// rect (i.e. its `CGDisplayBounds`): the one coordinate space that stays
/// consistent and non-overlapping across mixed-DPI displays. `(px, py)` must be
/// in that same logical-points space.
fn frame_contains(m: &tauri::Monitor, px: f64, py: f64) -> bool {
    let s = m.scale_factor();
    let (lx, ly) = (m.position().x as f64 / s, m.position().y as f64 / s);
    let (lw, lh) = (m.size().width as f64 / s, m.size().height as f64 / s);
    lx <= px && px < lx + lw && ly <= py && py < ly + lh
}

/// Cursor location in CoreGraphics global display coordinates: points, top-left
/// origin. That's the ONE space consistent across mixed-DPI displays — exactly
/// what each monitor's logical frame uses — so it pins the clicked display
/// unambiguously. The tray rect can't: its physical X is the icon's point ×
/// *that display's* scale, so an external (1×) icon's coordinate ÷ the main's
/// (2×) scale folds back into the main's range and the two displays become
/// indistinguishable. The cursor sits on the clicked icon, so it resolves the
/// display cleanly. Permission-free, same CoreGraphics FFI family as the idle
/// sensor.
#[cfg(target_os = "macos")]
fn cursor_point() -> Option<(f64, f64)> {
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    type CFTypeRef = *mut std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
        fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }
    // SAFETY: CGEventCreate(NULL) returns a retained event snapshotting the
    // current input state (including the cursor); we read its location and
    // release it.
    unsafe {
        let ev = CGEventCreate(std::ptr::null_mut());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        Some((p.x, p.y))
    }
}

/// The display under the cursor, matched in the consistent logical-points space.
#[cfg(target_os = "macos")]
fn monitor_under_cursor(monitors: &[tauri::Monitor]) -> Option<tauri::Monitor> {
    let (cx, cy) = cursor_point()?;
    monitors.iter().find(|m| frame_contains(m, cx, cy)).cloned()
}
#[cfg(not(target_os = "macos"))]
fn monitor_under_cursor(_: &[tauri::Monitor]) -> Option<tauri::Monitor> {
    None
}

/// The display a tray-summoned panel should open on. Prefer the cursor's
/// display (it's on the clicked icon); fall back to locating the icon anchor
/// itself, then the window's current/primary display — so the ⌘⇧Space summon
/// (no click, cursor may be elsewhere) and non-macOS builds still get an answer.
fn target_monitor(
    win: &tauri::WebviewWindow,
    anchor_x: f64,
    anchor_y: f64,
) -> Option<tauri::Monitor> {
    let monitors = win.available_monitors().unwrap_or_default();
    monitor_under_cursor(&monitors)
        .or_else(|| {
            // The icon anchor ÷ a display's own scale lands inside exactly one
            // logical frame: the display the icon actually sits on.
            monitors
                .iter()
                .find(|m| {
                    let s = m.scale_factor();
                    frame_contains(m, anchor_x / s, anchor_y / s)
                })
                .cloned()
        })
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten())
}

/// Position the panel just under a menu-bar anchor point and show it.
///
/// Two macOS multi-display traps to dodge. (1) The per-monitor "physical"
/// values tao reports are each that display's point frame × *its own* scale, so
/// beside a 1× display the physical spans of a 2× display overlap and can't be
/// compared — and `set_position(Physical)` then divides by the *window's
/// current* display scale, not the target's, flinging the panel to a random X.
/// (2) The tray rect's own X is ambiguous for the same scaling reason (see
/// `cursor_point`). So: pick the display from the cursor, do all the math in
/// logical points (the one consistent space), and place with a `LogicalPosition`
/// that tao writes through without re-scaling.
fn show_panel_under(win: &tauri::WebviewWindow, anchor_x: f64, anchor_y: f64) {
    // Panel width in logical points (DPI-independent), from its current size.
    let cur_scale = win.scale_factor().unwrap_or(1.0);
    let logical_w = win
        .outer_size()
        .map(|s| s.width as f64 / cur_scale)
        .unwrap_or(420.0);

    let target = target_monitor(win, anchor_x, anchor_y);

    let (x, y) = match &target {
        Some(m) => {
            // The icon is on this display, so its scale == the tray rect's
            // scale: anchor ÷ scale gives the icon's center/bottom in points.
            let s = m.scale_factor();
            let lx = m.position().x as f64 / s;
            let ly = m.position().y as f64 / s;
            let lw = m.size().width as f64 / s;
            let (ax, ay) = (anchor_x / s, anchor_y / s);
            let min_x = lx + 8.0;
            let max_x = (lx + lw - logical_w - 8.0).max(min_x);
            let x = (ax - logical_w / 2.0).clamp(min_x, max_x);
            // Drop just under this display's menu bar; never let an odd Y fling
            // the panel off-screen.
            let y = (ay + 6.0).clamp(ly + 4.0, ly + 80.0);
            (x, y)
        }
        // No display matched (shouldn't happen): center on the raw anchor via
        // the window's current scale.
        None => (anchor_x / cur_scale - logical_w / 2.0, anchor_y / cur_scale + 6.0),
    };

    // Run with AMDION_DEBUG_TRAY=1 to dump the geometry if placement is still
    // off — cursor, anchor, every display's frame, and the chosen drop point.
    if std::env::var_os("AMDION_DEBUG_TRAY").is_some() {
        #[cfg(target_os = "macos")]
        let cur = cursor_point();
        #[cfg(not(target_os = "macos"))]
        let cur: Option<(f64, f64)> = None;
        eprintln!(
            "[tray] cursor_pts={cur:?} anchor_phys=({anchor_x:.0},{anchor_y:.0}) cur_scale={cur_scale} logical_w={logical_w:.0}"
        );
        for m in &win.available_monitors().unwrap_or_default() {
            let s = m.scale_factor();
            let (p, sz) = (m.position(), m.size());
            eprintln!(
                "[tray]   display scale={s} -> logical x∈[{:.0},{:.0}) y∈[{:.0},{:.0})",
                p.x as f64 / s,
                (p.x as f64 + sz.width as f64) / s,
                p.y as f64 / s,
                (p.y as f64 + sz.height as f64) / s,
            );
        }
        eprintln!("[tray]   matched={} drop_logical=({x:.0},{y:.0})", target.is_some());
    }

    let _ = win.set_position(LogicalPosition::new(x, y));
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            // Nothing is pre-registered here: setup() registers the user's saved
            // summon binding (rebound live by commands::shortcut), and this one
            // global handler fires for whichever accelerator is currently active.
            tauri_plugin_global_shortcut::Builder::new()
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

            // Register the global summon shortcut from the saved config, falling
            // back to the built-in default if a customized binding won't take
            // (e.g. another app already owns it). Rebound live in Settings via
            // commands::shortcut::set_summon_shortcut.
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let gs = app.global_shortcut();
                let want = config::read_config().summon_shortcut;
                if gs.register(want.as_str()).is_err() && want != SUMMON_SHORTCUT {
                    let _ = gs.register(SUMMON_SHORTCUT);
                }
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

            // Silent auto-update: a short while after launch, check GitHub
            // Releases for a newer signed build and stage it (applied next
            // launch). Best-effort — network/endpoint errors are swallowed (e.g.
            // before the first release exists, or while offline). Release builds
            // only: `tauri dev` has no bundled .app to replace, and a manual
            // `check_for_updates` command stays available either way.
            #[cfg(not(debug_assertions))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let _ = commands::updater::check_and_install(&handle).await;
                });
            }

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

            // The Amdion hourglass mark in its cyan→blue gradient, disc-less so it
            // fills the menu bar. Rendered in COLOR (not a macOS template, which
            // would flatten it to one tint) so the gradient survives; the
            // saturated hue keeps it legible on a light or dark menu bar.
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
            commands::shortcut::set_summon_shortcut,
            // Read Mode: enter/exit the in-page reader on the active tab
            commands::read::enter_read_mode,
            commands::read::exit_read_mode,
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
            // Auto-update
            commands::updater::check_for_updates,
            commands::updater::relaunch_app,
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
