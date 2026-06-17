use localagentmanager_core::{list_accounts, refresh_all_quotas, resolve_home_root, AppError};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    image::Image,
    include_image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Rect, Runtime, Size, WebviewWindow,
};

const TRAY_MENU_ICON: Image<'_> = include_image!("icons/tray-menu-template.png");

pub const TRAY_ID: &str = "lam-quota-tray";
pub const POPOVER_LABEL: &str = "quota-popover";
const REFRESH_MENU_ID: &str = "tray-refresh";
const SHOW_MENU_ID: &str = "tray-show";

static TRAY_BUSY: Mutex<bool> = Mutex::new(false);
static POPOVER_OPACITY_PERCENT: AtomicU8 = AtomicU8::new(100);
static TRAY_CLICK_SUPPRESS_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

const TRAY_CLICK_SUPPRESS_AFTER_CLOSE: Duration = Duration::from_millis(650);

fn suppress_tray_clicks_for(duration: Duration) {
    if let Ok(mut guard) = TRAY_CLICK_SUPPRESS_UNTIL.lock() {
        *guard = Some(Instant::now() + duration);
    }
}

fn tray_clicks_suppressed() -> bool {
    let Ok(mut guard) = TRAY_CLICK_SUPPRESS_UNTIL.lock() else {
        return false;
    };
    let Some(until) = *guard else {
        return false;
    };
    if Instant::now() < until {
        return true;
    }
    *guard = None;
    false
}

#[cfg(target_os = "macos")]
fn apply_macos_popover_chrome<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), AppError> {
    use objc2_app_kit::{NSColor, NSWindow};
    use tauri::window::Color;

    window
        .set_background_color(Some(Color(0, 0, 0, 0)))
        .map_err(|err| AppError::new("POPOVER_OPACITY", err.to_string()))?;

    let ptr = window
        .ns_window()
        .map_err(|err| AppError::new("POPOVER_OPACITY", err.to_string()))?;
    let ns_window: &NSWindow = unsafe { &*ptr.cast() };
    ns_window.setOpaque(false);
    ns_window.setHasShadow(true);
    let clear = NSColor::clearColor();
    ns_window.setBackgroundColor(Some(clear.as_ref()));
    if let Some(content_view) = ns_window.contentView() {
        content_view.setWantsLayer(true);
        if let Some(layer) = content_view.layer() {
            layer.setCornerRadius(16.0);
            layer.setMasksToBounds(true);
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_macos_popover_opacity<R: Runtime>(
    window: &WebviewWindow<R>,
    percent: u8,
) -> Result<(), AppError> {
    use objc2_app_kit::NSWindow;

    let alpha = (percent as f32 / 100.0).clamp(0.85, 1.0);
    apply_macos_popover_chrome(window)?;

    let ptr = window
        .ns_window()
        .map_err(|err| AppError::new("POPOVER_OPACITY", err.to_string()))?;
    let ns_window: &NSWindow = unsafe { &*ptr.cast() };
    ns_window.setAlphaValue(alpha.into());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn apply_macos_popover_opacity<R: Runtime>(
    _window: &WebviewWindow<R>,
    _percent: u8,
) -> Result<(), AppError> {
    Ok(())
}

pub fn set_quota_popover_opacity<R: Runtime>(
    app: &AppHandle<R>,
    percent: u8,
) -> Result<(), AppError> {
    let percent = percent.clamp(85, 100);
    POPOVER_OPACITY_PERCENT.store(percent, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        apply_macos_popover_opacity(&window, percent)?;
    }
    Ok(())
}

fn prepare_quota_popover_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let percent = POPOVER_OPACITY_PERCENT.load(Ordering::Relaxed);
        if let Err(err) = apply_macos_popover_opacity(&window, percent) {
            eprintln!("LAM: quota popover chrome setup failed: {}", err.message);
        }
    }
}

pub fn hide_quota_popover<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return Ok(());
    };
    if !window.is_visible().unwrap_or(false) {
        return Ok(());
    }
    suppress_tray_clicks_for(TRAY_CLICK_SUPPRESS_AFTER_CLOSE);
    window
        .hide()
        .map_err(|err| AppError::new("POPOVER_HIDE_FAILED", err.to_string()))
}

fn home_root() -> Result<std::path::PathBuf, AppError> {
    resolve_home_root()
}

fn load_tray_icon() -> Image<'static> {
    TRAY_MENU_ICON.clone()
}

fn build_tray_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, AppError> {
    let refresh = MenuItem::with_id(app, REFRESH_MENU_ID, "Refresh quotas", true, None::<&str>)
        .map_err(|err| AppError::new("TRAY_MENU_FAILED", err.to_string()))?;
    let show = MenuItem::with_id(app, SHOW_MENU_ID, "Open LAM", true, None::<&str>)
        .map_err(|err| AppError::new("TRAY_MENU_FAILED", err.to_string()))?;

    Menu::with_items(app, &[&refresh, &show])
        .map_err(|err| AppError::new("TRAY_MENU_FAILED", err.to_string()))
}

fn position_quota_popover<R: Runtime>(window: &WebviewWindow<R>, tray_rect: Option<Rect>) {
    if let Some(rect) = tray_rect {
        let (x, y, h) = match (rect.position, rect.size) {
            (Position::Physical(pos), Size::Physical(size)) => {
                (pos.x as f64, pos.y as f64, size.height as f64)
            }
            (Position::Logical(pos), Size::Logical(size)) => (pos.x, pos.y, size.height),
            _ => return,
        };
        let _ = window.set_position(PhysicalPosition::new(
            x.max(8.0) as i32,
            (y + h + 6.0) as i32,
        ));
        return;
    }
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let width = 328.0;
        let x = (size.width as f64 / scale) - width - 10.0;
        let _ = window.set_position(PhysicalPosition::new(
            (x * scale) as i32,
            (28.0 * scale) as i32,
        ));
    }
}

pub fn toggle_quota_popover<R: Runtime>(app: &AppHandle<R>, tray_rect: Option<Rect>) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        eprintln!("LAM: quota popover window not found");
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    if visible {
        let _ = window.hide();
        return;
    }

    position_quota_popover(&window, tray_rect);
    let percent = POPOVER_OPACITY_PERCENT.load(Ordering::Relaxed);
    if let Err(err) = apply_macos_popover_opacity(&window, percent) {
        eprintln!("LAM: quota popover opacity apply failed: {}", err.message);
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(POPOVER_LABEL, "quota-popover-refresh", ());
}

pub fn refresh_tray_menu<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| AppError::new("TRAY_NOT_FOUND", "menu bar tray is not initialized"))?;
    let menu = build_tray_menu(app)?;
    tray.set_menu(Some(menu))
        .map_err(|err| AppError::new("TRAY_MENU_FAILED", err.to_string()))?;
    Ok(())
}

fn refresh_tray_with_fetch<R: Runtime>(
    app: &AppHandle<R>,
    force_fetch: bool,
) -> Result<(), AppError> {
    if force_fetch {
        let home = home_root()?;
        let accounts = list_accounts(&home)?;
        let ids: Vec<String> = accounts.iter().map(|a| a.id.clone()).collect();
        if !ids.is_empty() {
            let _ = refresh_all_quotas(&home, Some(ids));
        }
    }
    refresh_tray_menu(app)?;
    let _ = app.emit_to(POPOVER_LABEL, "quota-popover-refresh", ());
    Ok(())
}

pub fn refresh_tray_menu_background<R: Runtime>(app: AppHandle<R>, force_fetch: bool) {
    let Ok(mut busy) = TRAY_BUSY.lock() else {
        return;
    };
    if *busy {
        return;
    }
    *busy = true;
    drop(busy);

    tauri::async_runtime::spawn_blocking(move || {
        if let Err(err) = refresh_tray_with_fetch(&app, force_fetch) {
            eprintln!("LAM tray refresh failed: {}", err.message);
        }
        if let Ok(mut busy) = TRAY_BUSY.lock() {
            *busy = false;
        }
    });
}

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let icon = load_tray_icon();
    let menu = build_tray_menu(app)?;
    let app_click = app.clone();

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .tooltip("LAM · Codex quota")
        .show_menu_on_left_click(false)
        .icon_as_template(true)
        .on_tray_icon_event(move |_tray, event| {
            if tray_clicks_suppressed() {
                return;
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_quota_popover(&app_click, Some(rect));
            }
        })
        .on_menu_event(move |app_handle, event| match event.id.as_ref() {
            REFRESH_MENU_ID => refresh_tray_menu_background(app_handle.clone(), true),
            SHOW_MENU_ID => show_main_window(app_handle),
            _ => {}
        })
        .build(app)
        .map_err(|err| AppError::new("TRAY_INIT_FAILED", err.to_string()))?;

    prepare_quota_popover_window(app);
    eprintln!("LAM: menu bar tray ready — left-click opens colorful quota panel");

    let app_bg = app.clone();
    refresh_tray_menu_background(app_bg.clone(), false);

    tauri::async_runtime::spawn(async move {
        std::thread::sleep(Duration::from_secs(12));
        refresh_tray_menu_background(app_bg.clone(), true);
        loop {
            std::thread::sleep(Duration::from_secs(300));
            refresh_tray_menu_background(app_bg.clone(), true);
        }
    });

    Ok(())
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    for (label, window) in app.webview_windows() {
        if label == "main" {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            break;
        }
    }
}
