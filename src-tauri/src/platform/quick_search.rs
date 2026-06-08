use std::sync::{Arc, RwLock};

use tauri::{App, Emitter, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};

#[derive(Clone)]
pub struct QuickSearchWindow {
    window: WebviewWindow<Wry>,
    target_window: Arc<RwLock<isize>>,
}

impl QuickSearchWindow {
    pub fn create(app: &App<Wry>) -> tauri::Result<Self> {
        let window =
            WebviewWindowBuilder::new(app, "quick-search", WebviewUrl::App("index.html".into()))
                .title("TextPilot quick search")
                .inner_size(680.0, 500.0)
                .min_inner_size(520.0, 380.0)
                .resizable(true)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(true)
                .focusable(true)
                .visible(false)
                .center()
                .build()?;

        Ok(Self {
            window,
            target_window: Arc::new(RwLock::new(0)),
        })
    }

    pub fn open(&self, target_window: isize, anchor_x: i32, anchor_y: i32) {
        if let Ok(mut current) = self.target_window.write() {
            *current = target_window;
        }

        self.center_on_anchor_monitor(anchor_x, anchor_y);
        if let Err(error) = self.window.show() {
            tracing::warn!(%error, "failed to show quick search");
            return;
        }
        if let Err(error) = self.window.set_focus() {
            tracing::warn!(%error, "failed to focus quick search");
        }
        if let Err(error) = self.window.emit("quick-search-opened", ()) {
            tracing::warn!(%error, "failed to initialize quick search");
        }
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(false)
    }

    pub fn hide(&self) {
        if let Err(error) = self.window.hide() {
            tracing::warn!(%error, "failed to hide quick search");
        }
    }

    pub fn target_window(&self) -> isize {
        self.target_window.read().map(|target| *target).unwrap_or(0)
    }

    fn center_on_anchor_monitor(&self, anchor_x: i32, anchor_y: i32) {
        let Ok(Some(monitor)) = self
            .window
            .monitor_from_point(anchor_x as f64, anchor_y as f64)
        else {
            if let Err(error) = self.window.center() {
                tracing::warn!(%error, "failed to center quick search");
            }
            return;
        };

        let work_area = monitor.work_area();

        // Move while hidden so Windows applies the target monitor's DPI before
        // the final centering calculation.
        if let Err(error) = self.window.set_position(work_area.position) {
            tracing::warn!(%error, "failed to move quick search to target monitor");
            return;
        }

        let Ok(window_size) = self.window.outer_size() else {
            if let Err(error) = self.window.center() {
                tracing::warn!(%error, "failed to center quick search");
            }
            return;
        };

        let x = work_area.position.x
            + work_area
                .size
                .width
                .saturating_sub(window_size.width)
                .div_ceil(2) as i32;
        let y = work_area.position.y
            + work_area
                .size
                .height
                .saturating_sub(window_size.height)
                .div_ceil(2) as i32;

        if let Err(error) = self.window.set_position(PhysicalPosition::new(x, y)) {
            tracing::warn!(%error, "failed to center quick search on target monitor");
        }
    }
}

pub fn hide_on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "quick-search" {
        return;
    }

    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if let Err(error) = window.hide() {
            tracing::warn!(%error, "failed to hide quick search");
        }
    }
}
