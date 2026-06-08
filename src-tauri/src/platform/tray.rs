use tauri::{
    App, AppHandle, Emitter, Manager, Wry,
    menu::{Menu, MenuItem, MenuItemBuilder},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

use crate::AppState;

const OPEN_ID: &str = "tray-open";
const PAUSE_ID: &str = "tray-pause";
const QUIT_ID: &str = "tray-quit";

#[derive(Clone)]
pub struct TrayController {
    pause_item: MenuItem<Wry>,
}

impl TrayController {
    pub fn create(app: &App) -> tauri::Result<(Self, Menu<Wry>)> {
        let open_item = MenuItemBuilder::with_id(OPEN_ID, "Открыть TextPilot").build(app)?;
        let pause_item = MenuItemBuilder::with_id(PAUSE_ID, "Пауза").build(app)?;
        let quit_item = MenuItemBuilder::with_id(QUIT_ID, "Выход").build(app)?;
        let menu = Menu::with_items(app, &[&open_item, &pause_item, &quit_item])?;

        Ok((Self { pause_item }, menu))
    }

    pub fn build(app: &App, menu: &Menu<Wry>) -> tauri::Result<()> {
        let mut builder = TrayIconBuilder::with_id("textpilot-tray")
            .menu(menu)
            .tooltip("TextPilot")
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                OPEN_ID => show_main_window(app),
                PAUSE_ID => toggle_paused(app),
                QUIT_ID => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            });

        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }

        builder.build(app)?;
        Ok(())
    }

    pub fn set_paused(&self, paused: bool) -> tauri::Result<()> {
        self.pause_item.set_text(if paused {
            "Продолжить"
        } else {
            "Пауза"
        })
    }
}

pub fn hide_on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }

    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if let Err(error) = window.hide() {
            tracing::warn!(%error, "failed to hide TextPilot window");
        }
    }
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    if let Err(error) = window.show() {
        tracing::warn!(%error, "failed to show TextPilot window");
        return;
    }
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn toggle_paused(app: &AppHandle) {
    let state = app.state::<AppState>();
    let paused = !state.snippets.is_paused();
    state.snippets.set_paused(paused);
    if paused {
        state.autocomplete.hide();
    }

    if let Err(error) = state.tray.set_paused(paused) {
        tracing::warn!(%error, "failed to update tray pause label");
    }
    if let Err(error) = app.emit("pause-changed", paused) {
        tracing::warn!(%error, "failed to notify UI about pause state");
    }
}
