use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{
    App, Emitter, LogicalSize, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    Wry,
};

const OVERLAY_WIDTH: f64 = 360.0;
const ROW_HEIGHT: f64 = 42.0;
const FOOTER_HEIGHT: f64 = 34.0;
const WINDOW_PADDING: f64 = 8.0;
const SCREEN_MARGIN: f64 = 8.0;
const CARET_GAP: f64 = 6.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutocompleteSuggestion {
    pub prefix: String,
    pub words: Vec<String>,
    pub selected_index: usize,
}

#[derive(Clone)]
pub struct AutocompleteOverlay {
    window: WebviewWindow<Wry>,
    current: Arc<RwLock<Option<AutocompleteSuggestion>>>,
}

impl AutocompleteOverlay {
    pub fn create(app: &App<Wry>) -> tauri::Result<Self> {
        let window =
            WebviewWindowBuilder::new(app, "autocomplete", WebviewUrl::App("index.html".into()))
                .title("TextPilot autocomplete")
                .inner_size(OVERLAY_WIDTH, 84.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(false)
                .focusable(false)
                .focused(false)
                .transparent(true)
                .visible(false)
                .build()?;
        window.set_ignore_cursor_events(false)?;

        Ok(Self {
            window,
            current: Arc::new(RwLock::new(None)),
        })
    }

    pub fn present(
        &self,
        suggestion: AutocompleteSuggestion,
        anchor_x: i32,
        anchor_top: i32,
        anchor_bottom: i32,
    ) {
        if let Ok(mut current) = self.current.write() {
            *current = Some(suggestion.clone());
        }

        let logical_height =
            WINDOW_PADDING + suggestion.words.len() as f64 * ROW_HEIGHT + FOOTER_HEIGHT;
        if let Err(error) = self
            .window
            .set_size(LogicalSize::new(OVERLAY_WIDTH, logical_height))
        {
            tracing::warn!(%error, "failed to resize autocomplete overlay");
        }

        let (x, y) = self.overlay_position(
            anchor_x,
            anchor_top,
            anchor_bottom,
            OVERLAY_WIDTH,
            logical_height,
        );
        if let Err(error) = self.window.set_position(PhysicalPosition::new(x, y)) {
            tracing::warn!(%error, "failed to position autocomplete overlay");
        }
        if let Err(error) = self.window.show() {
            tracing::warn!(%error, "failed to show autocomplete overlay");
        }
        if let Err(error) = self.window.emit("autocomplete-suggestion", suggestion) {
            tracing::warn!(%error, "failed to update autocomplete overlay");
        }
    }

    pub fn hide(&self) {
        if let Ok(mut current) = self.current.write() {
            *current = None;
        }
        if let Err(error) = self.window.hide() {
            tracing::warn!(%error, "failed to hide autocomplete overlay");
        }
    }

    pub fn current(&self) -> Option<AutocompleteSuggestion> {
        self.current.read().ok()?.clone()
    }

    fn overlay_position(
        &self,
        anchor_x: i32,
        anchor_top: i32,
        anchor_bottom: i32,
        logical_width: f64,
        logical_height: f64,
    ) -> (i32, i32) {
        let Ok(Some(monitor)) = self
            .window
            .monitor_from_point(anchor_x as f64, anchor_bottom as f64)
        else {
            return (anchor_x, anchor_bottom + CARET_GAP as i32);
        };

        let scale = monitor.scale_factor();
        let width = (logical_width * scale).round() as i32;
        let height = (logical_height * scale).round() as i32;
        let margin = (SCREEN_MARGIN * scale).round() as i32;
        let gap = (CARET_GAP * scale).round() as i32;
        let work_area = monitor.work_area();
        let left = work_area.position.x + margin;
        let top = work_area.position.y + margin;
        let right = work_area.position.x + work_area.size.width as i32 - margin;
        let bottom = work_area.position.y + work_area.size.height as i32 - margin;

        let x = anchor_x.clamp(left, (right - width).max(left));
        let below = anchor_bottom + gap;
        let y = if below + height <= bottom {
            below
        } else {
            (anchor_top - gap - height).max(top)
        };

        (x, y)
    }
}
