pub mod core;
pub mod platform;
mod storage;

use std::{env, fs, path::PathBuf};

use serde::Serialize;
use storage::{
    BackupSummary, CreateCategoryInput, CreatePhraseInput, DashboardData, Database,
    DictionaryWordInput, ExportResult, ImportConflictStrategy, ImportPreview, ImportResult,
    ProfileSnapshotSummary, QuickSearchItem, RenameCategoryInput, ReorderCategoryInput,
    RestoreBackupResult, RestoreProfileSnapshotResult, UpdatePhraseInput,
};
use tauri::{Manager, State, WebviewWindow};
use tracing_subscriber::EnvFilter;

#[cfg(target_os = "windows")]
use platform::{
    autocomplete_overlay::{AutocompleteOverlay, AutocompleteSuggestion},
    quick_search::{self, QuickSearchWindow},
    save_dialog,
    tray::{self, TrayController},
    windows::{
        self, AutocompleteController, AutocorrectController, InputMonitor, QuickSearchController,
        SnippetController,
    },
};

struct AppState {
    database: Database,
    #[cfg(target_os = "windows")]
    snippets: SnippetController,
    #[cfg(target_os = "windows")]
    autocomplete: AutocompleteController,
    #[cfg(target_os = "windows")]
    autocorrect: AutocorrectController,
    #[cfg(target_os = "windows")]
    autocomplete_overlay: AutocompleteOverlay,
    #[cfg(target_os = "windows")]
    quick_search: QuickSearchWindow,
    #[cfg(target_os = "windows")]
    input_monitor: Option<InputMonitor>,
    #[cfg(target_os = "windows")]
    tray: TrayController,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapStatus {
    app_name: &'static str,
    database_ready: bool,
    profile_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    #[serde(flatten)]
    data: DashboardData,
    is_paused: bool,
    engine_available: bool,
}

#[tauri::command]
fn bootstrap_status(state: State<'_, AppState>) -> Result<BootstrapStatus, String> {
    let profile_count = state
        .database
        .profile_count()
        .map_err(|error| error.to_string())?;

    Ok(BootstrapStatus {
        app_name: "TextPilot",
        database_ready: true,
        profile_count,
    })
}

#[tauri::command]
fn dashboard_data(state: State<'_, AppState>) -> Result<DashboardResponse, String> {
    let data = state
        .database
        .dashboard_data()
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "windows")]
    {
        Ok(DashboardResponse {
            data,
            is_paused: state.snippets.is_paused(),
            engine_available: state.input_monitor.is_some(),
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(DashboardResponse {
            data,
            is_paused: true,
            engine_available: false,
        })
    }
}

#[tauri::command]
fn create_category(state: State<'_, AppState>, input: CreateCategoryInput) -> Result<(), String> {
    state
        .database
        .create_category(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rename_category(state: State<'_, AppState>, input: RenameCategoryInput) -> Result<(), String> {
    state
        .database
        .rename_category(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_category(state: State<'_, AppState>, category_id: i64) -> Result<(), String> {
    state
        .database
        .delete_category(category_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reorder_category(state: State<'_, AppState>, input: ReorderCategoryInput) -> Result<(), String> {
    state
        .database
        .reorder_category(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_profile(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state
        .database
        .create_profile(&name)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn rename_profile(state: State<'_, AppState>, profile_id: i64, name: String) -> Result<(), String> {
    state
        .database
        .rename_profile(profile_id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_active_profile(state: State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state
        .database
        .set_active_profile(profile_id)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn delete_profile(state: State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state
        .database
        .delete_profile(profile_id)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn create_phrase(state: State<'_, AppState>, input: CreatePhraseInput) -> Result<(), String> {
    state
        .database
        .create_phrase(input)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn update_phrase(state: State<'_, AppState>, input: UpdatePhraseInput) -> Result<(), String> {
    state
        .database
        .update_phrase(input)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn set_phrase_enabled(
    state: State<'_, AppState>,
    phrase_id: i64,
    enabled: bool,
) -> Result<(), String> {
    state
        .database
        .set_phrase_enabled(phrase_id, enabled)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn delete_phrase(state: State<'_, AppState>, phrase_id: i64) -> Result<(), String> {
    state
        .database
        .delete_phrase(phrase_id)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn save_dictionary_word(
    state: State<'_, AppState>,
    input: DictionaryWordInput,
) -> Result<(), String> {
    state
        .database
        .save_dictionary_word(input)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn delete_dictionary_word(state: State<'_, AppState>, word_id: i64) -> Result<(), String> {
    state
        .database
        .delete_dictionary_word(word_id)
        .map_err(|error| error.to_string())?;
    refresh_input_indexes(&state)
}

#[tauri::command]
fn export_configuration(
    window: WebviewWindow,
    state: State<'_, AppState>,
    profile_id: Option<i64>,
) -> Result<Option<ExportResult>, String> {
    #[cfg(target_os = "windows")]
    {
        let suggested_file_name = state.database.suggested_export_file_name(profile_id)?;
        let owner = window.hwnd().map_err(|error| error.to_string())?;
        let Some(destination) = save_dialog::choose_json_save_path(owner, &suggested_file_name)?
        else {
            return Ok(None);
        };
        return state
            .database
            .export_configuration(profile_id, &destination)
            .map(Some);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, state, profile_id);
        Err("Выбор места сохранения пока поддерживается только в Windows.".into())
    }
}

#[tauri::command]
fn preview_import(state: State<'_, AppState>, json: String) -> Result<ImportPreview, String> {
    state.database.preview_import(&json)
}

#[tauri::command]
fn import_configuration(
    state: State<'_, AppState>,
    json: String,
    strategy: ImportConflictStrategy,
) -> Result<ImportResult, String> {
    let result = state.database.import_configuration(&json, strategy)?;
    refresh_input_indexes(&state)?;
    Ok(result)
}

#[tauri::command]
fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupSummary>, String> {
    state.database.list_backups()
}

#[tauri::command]
fn restore_backup(
    state: State<'_, AppState>,
    file_name: String,
) -> Result<RestoreBackupResult, String> {
    #[cfg(target_os = "windows")]
    {
        let was_paused = state.snippets.is_paused();
        state.snippets.set_paused(true);
        state.autocomplete.hide();

        let result = state
            .database
            .restore_backup(&file_name)
            .and_then(|result| {
                refresh_input_indexes(&state)?;
                Ok(result)
            });
        state.snippets.set_paused(was_paused);
        return result;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let result = state.database.restore_backup(&file_name)?;
        refresh_input_indexes(&state)?;
        Ok(result)
    }
}

#[tauri::command]
fn create_profile_snapshot(
    state: State<'_, AppState>,
    profile_id: i64,
) -> Result<ProfileSnapshotSummary, String> {
    state.database.create_profile_snapshot(profile_id)
}

#[tauri::command]
fn list_profile_snapshots(
    state: State<'_, AppState>,
    profile_id: i64,
) -> Result<Vec<ProfileSnapshotSummary>, String> {
    state.database.list_profile_snapshots(profile_id)
}

#[tauri::command]
fn restore_profile_snapshot(
    state: State<'_, AppState>,
    profile_id: i64,
    file_name: String,
) -> Result<RestoreProfileSnapshotResult, String> {
    #[cfg(target_os = "windows")]
    {
        let was_paused = state.snippets.is_paused();
        state.snippets.set_paused(true);
        state.autocomplete.hide();

        let result = state
            .database
            .restore_profile_snapshot(profile_id, &file_name)
            .and_then(|result| {
                refresh_input_indexes(&state)?;
                Ok(result)
            });
        state.snippets.set_paused(was_paused);
        return result;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let result = state
            .database
            .restore_profile_snapshot(profile_id, &file_name)?;
        refresh_input_indexes(&state)?;
        Ok(result)
    }
}

#[tauri::command]
fn set_paused(state: State<'_, AppState>, paused: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        state.snippets.set_paused(paused);
        if paused {
            state.autocomplete.hide();
        }
        state
            .tray
            .set_paused(paused)
            .map_err(|error| error.to_string())?;
    }

    Ok(paused)
}

#[tauri::command]
fn autocomplete_suggestion(state: State<'_, AppState>) -> Option<AutocompleteSuggestion> {
    #[cfg(target_os = "windows")]
    {
        state.autocomplete_overlay.current()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[tauri::command]
fn select_autocomplete(state: State<'_, AppState>, index: usize) {
    #[cfg(target_os = "windows")]
    state.autocomplete.select(index);
}

#[tauri::command]
fn accept_autocomplete(state: State<'_, AppState>, index: usize) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let accepted = state
            .autocomplete
            .accept(index)
            .ok_or_else(|| "autocomplete suggestion is no longer available".to_owned())?;
        windows::insert_text_into_window(accepted.target_window, &accepted.suffix)
            .map_err(|error| error.to_string())?;
        state.autocomplete.record_external_accept(&accepted);
    }

    Ok(())
}

#[tauri::command]
fn quick_search_items(state: State<'_, AppState>) -> Result<Vec<QuickSearchItem>, String> {
    state
        .database
        .quick_search_items()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn close_quick_search(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let target_window = state.quick_search.target_window();
        state.quick_search.hide();
        windows::focus_window(target_window).map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn insert_quick_search_item(
    state: State<'_, AppState>,
    kind: String,
    item_id: i64,
) -> Result<(), String> {
    let text = state
        .database
        .quick_search_item_text(&kind, item_id)
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "windows")]
    {
        let target_window = state.quick_search.target_window();
        state.quick_search.hide();
        windows::insert_text_into_window(target_window, &text)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn refresh_input_indexes(state: &AppState) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let snippets = state
            .database
            .active_snippets()
            .map_err(|error| error.to_string())?;
        let autocomplete_words = state
            .database
            .active_autocomplete_words()
            .map_err(|error| error.to_string())?;
        let autocorrect_words = state
            .database
            .active_autocorrect_words()
            .map_err(|error| error.to_string())?;
        state.snippets.replace_snippets(snippets);
        state.autocomplete.replace_words(autocomplete_words);
        state.autocorrect.replace_words(autocorrect_words);
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .init();

    tauri::Builder::default()
        .on_window_event(|window, event| {
            #[cfg(target_os = "windows")]
            {
                tray::hide_on_close(window, event);
                quick_search::hide_on_close(window, event);
            }
        })
        .setup(|app| {
            let app_data_dir = env::var_os("TEXTPILOT_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            fs::create_dir_all(&app_data_dir)?;

            let database = Database::open(app_data_dir.join("textpilot.db"))?;

            #[cfg(target_os = "windows")]
            {
                let (tray, tray_menu) = TrayController::create(app)?;
                let snippets = SnippetController::new(database.active_snippets()?);
                let autocomplete_overlay = AutocompleteOverlay::create(app)?;
                let overlay = autocomplete_overlay.clone();
                let autocomplete = AutocompleteController::new(
                    database.active_autocomplete_words()?,
                    move |suggestion| match suggestion {
                        Some(suggestion) => overlay.present(
                            AutocompleteSuggestion {
                                prefix: suggestion.prefix,
                                words: suggestion.words,
                                selected_index: suggestion.selected_index,
                            },
                            suggestion.anchor_x,
                            suggestion.anchor_top,
                            suggestion.anchor_bottom,
                        ),
                        None => overlay.hide(),
                    },
                );
                let autocorrect = AutocorrectController::new(database.active_autocorrect_words()?);
                let quick_search = QuickSearchWindow::create(app)?;
                let search_window = quick_search.clone();
                let search_overlay = autocomplete_overlay.clone();
                let quick_search_controller =
                    QuickSearchController::new(move |target_window, anchor| {
                        search_overlay.hide();
                        if search_window.is_visible() {
                            let previous_target = search_window.target_window();
                            search_window.hide();
                            if let Err(error) = windows::focus_window(previous_target) {
                                tracing::warn!(%error, "failed to restore quick search target");
                            }
                        } else {
                            search_window.open(target_window, anchor);
                        }
                    });
                let input_monitor = match InputMonitor::start(
                    snippets.clone(),
                    autocomplete.clone(),
                    autocorrect.clone(),
                    quick_search_controller,
                ) {
                    Ok(monitor) => Some(monitor),
                    Err(error) => {
                        tracing::error!(%error, "failed to start keyboard hook");
                        None
                    }
                };
                app.manage(AppState {
                    database,
                    snippets,
                    autocomplete,
                    autocorrect,
                    autocomplete_overlay,
                    quick_search,
                    input_monitor,
                    tray,
                });
                TrayController::build(app, &tray_menu)?;
            }

            #[cfg(not(target_os = "windows"))]
            app.manage(AppState { database });

            tracing::info!("TextPilot storage initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_status,
            dashboard_data,
            create_profile,
            rename_profile,
            set_active_profile,
            delete_profile,
            create_category,
            rename_category,
            delete_category,
            reorder_category,
            create_phrase,
            update_phrase,
            set_phrase_enabled,
            delete_phrase,
            save_dictionary_word,
            delete_dictionary_word,
            export_configuration,
            preview_import,
            import_configuration,
            list_backups,
            restore_backup,
            create_profile_snapshot,
            list_profile_snapshots,
            restore_profile_snapshot,
            set_paused,
            autocomplete_suggestion,
            select_autocomplete,
            accept_autocomplete,
            quick_search_items,
            close_quick_search,
            insert_quick_search_item
        ])
        .run(tauri::generate_context!())
        .expect("failed to run TextPilot");
}
