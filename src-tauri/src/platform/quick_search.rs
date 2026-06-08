use std::{
    io, mem, ptr,
    sync::{Arc, RwLock},
};

use tauri::{App, Emitter, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
    },
    UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    },
};

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

    pub fn open(&self, target_window: isize, anchor: Option<(i32, i32)>) {
        if let Ok(mut current) = self.target_window.write() {
            *current = target_window;
        }

        if let Err(error) = self.center_on_target_monitor(target_window, anchor) {
            tracing::warn!(%error, "failed to position quick search on target monitor");
            if let Err(error) = self.window.center() {
                tracing::warn!(%error, "failed to center quick search");
            }
        }
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

    fn center_on_target_monitor(
        &self,
        target_window: isize,
        anchor: Option<(i32, i32)>,
    ) -> io::Result<()> {
        let monitor = anchor
            .map(|(x, y)| unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) })
            .filter(|monitor| !monitor.is_null())
            .unwrap_or_else(|| unsafe {
                MonitorFromWindow(target_window as HWND, MONITOR_DEFAULTTONEAREST)
            });
        if monitor.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut monitor_info = MONITORINFO {
            cbSize: mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut monitor_info) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let hwnd = self
            .window
            .hwnd()
            .map_err(|error| io::Error::other(error.to_string()))?
            .0 as HWND;

        // The first move applies the target monitor's DPI. The second pass
        // uses the resulting physical size for exact centering.
        center_native_window(hwnd, monitor_info.rcWork)?;
        center_native_window(hwnd, monitor_info.rcWork)
    }
}

fn center_native_window(hwnd: HWND, work_area: RECT) -> io::Result<()> {
    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let width = window_rect.right - window_rect.left;
    let height = window_rect.bottom - window_rect.top;
    let available_width = work_area.right - work_area.left;
    let available_height = work_area.bottom - work_area.top;
    let x = work_area.left + (available_width - width).max(0) / 2;
    let y = work_area.top + (available_height - height).max(0) / 2;

    if unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }

    Ok(())
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
