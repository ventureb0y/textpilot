use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use rusqlite::{Connection, MAIN_DB, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use super::{
    Database, active_profile_id, initialize_connection, normalize_case_key, validate_category_name,
    validate_dictionary_word, validate_phrase, validate_profile_name,
};

const CONFIGURATION_VERSION: u32 = 1;
const MAX_IMPORT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportConflictStrategy {
    Overwrite,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub profile_count: u32,
    pub category_count: u32,
    pub phrase_count: u32,
    pub word_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub version: u32,
    pub target_profile_name: Option<String>,
    pub profile_count: u32,
    pub category_count: u32,
    pub phrase_count: u32,
    pub word_count: u32,
    pub snippet_conflicts: u32,
    pub word_conflicts: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub profile_count: u32,
    pub category_count: u32,
    pub phrase_count: u32,
    pub word_count: u32,
    pub skipped_conflicts: u32,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupResult {
    pub safety_backup_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshotSummary {
    pub file_name: String,
    pub profile_id: i64,
    pub profile_name: String,
    pub size_bytes: u64,
    pub modified_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreProfileSnapshotResult {
    pub safety_snapshot_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigurationFile {
    version: u32,
    exported_at: String,
    profiles: Vec<ConfigurationProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigurationProfile {
    name: String,
    #[serde(default)]
    categories: Vec<ConfigurationCategory>,
    #[serde(default)]
    phrases: Vec<ConfigurationPhrase>,
    #[serde(default)]
    words: Vec<ConfigurationWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigurationCategory {
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigurationPhrase {
    title: String,
    snippet: String,
    #[serde(default)]
    category_path: Option<String>,
    content: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true")]
    is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigurationWord {
    word: String,
    #[serde(default)]
    category_path: Option<String>,
    #[serde(default)]
    priority: i32,
    #[serde(default = "default_true")]
    is_enabled: bool,
    #[serde(default = "default_true")]
    allow_autocomplete: bool,
    #[serde(default = "default_true")]
    allow_autocorrect: bool,
}

impl Database {
    pub fn export_configuration(
        &self,
        profile_id: Option<i64>,
        destination: &Path,
    ) -> Result<ExportResult, String> {
        let connection = self.lock().map_err(|error| error.to_string())?;
        let configuration = build_configuration(&connection, profile_id)?;
        let counts = configuration_counts(&configuration);
        let json =
            serde_json::to_string_pretty(&configuration).map_err(|error| error.to_string())?;
        fs::write(destination, format!("{json}\n")).map_err(|error| error.to_string())?;

        Ok(ExportResult {
            path: destination.display().to_string(),
            profile_count: counts.profile_count,
            category_count: counts.category_count,
            phrase_count: counts.phrase_count,
            word_count: counts.word_count,
        })
    }

    pub fn suggested_export_file_name(&self, profile_id: Option<i64>) -> Result<String, String> {
        let connection = self.lock().map_err(|error| error.to_string())?;
        let date: String = connection
            .query_row("SELECT strftime('%Y-%m-%d', 'now')", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;

        match profile_id {
            Some(profile_id) => {
                let profile_name: String = connection
                    .query_row(
                        "SELECT name FROM profiles WHERE id = ?1",
                        [profile_id],
                        |row| row.get(0),
                    )
                    .map_err(|_| "Профиль для экспорта не найден.".to_owned())?;
                Ok(format!(
                    "textpilot-profile-{}-{date}.json",
                    file_slug(&profile_name)
                ))
            }
            None => Ok(format!("textpilot-export-{date}.json")),
        }
    }

    pub fn preview_import(&self, json: &str) -> Result<ImportPreview, String> {
        let configuration = parse_configuration(json)?;
        let connection = self.lock().map_err(|error| error.to_string())?;
        preview_configuration(&connection, &configuration)
    }

    pub fn import_configuration(
        &self,
        json: &str,
        strategy: ImportConflictStrategy,
    ) -> Result<ImportResult, String> {
        let configuration = parse_configuration(json)?;
        let mut connection = self.lock().map_err(|error| error.to_string())?;
        let backup_path = create_backup(&connection, self.path.as_deref(), "before-import")?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let result = apply_configuration(&transaction, &configuration, strategy, backup_path)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    pub fn list_backups(&self) -> Result<Vec<BackupSummary>, String> {
        let Some(database_path) = self.path.as_deref() else {
            return Ok(Vec::new());
        };
        let backup_directory = backup_directory(database_path)?;
        if !backup_directory.exists() {
            return Ok(Vec::new());
        }

        let mut backups = fs::read_dir(&backup_directory)
            .map_err(|error| error.to_string())?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if !is_backup_file(&path) {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                let modified_at_ms = metadata
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_millis() as u64;
                Some(BackupSummary {
                    file_name: entry.file_name().to_string_lossy().into_owned(),
                    size_bytes: metadata.len(),
                    modified_at_ms,
                })
            })
            .collect::<Vec<_>>();
        backups.sort_by(|left, right| right.modified_at_ms.cmp(&left.modified_at_ms));
        Ok(backups)
    }

    pub fn restore_backup(&self, file_name: &str) -> Result<RestoreBackupResult, String> {
        let database_path = self
            .path
            .as_deref()
            .ok_or_else(|| "Восстановление недоступно для временной базы.".to_owned())?;
        let backup_path = resolve_backup_path(database_path, file_name)?;
        validate_backup(&backup_path)?;

        let mut connection = self.lock().map_err(|error| error.to_string())?;
        let safety_backup_path = create_backup(&connection, Some(database_path), "before-restore")?
            .ok_or_else(|| "Не удалось создать страховочную резервную копию.".to_owned())?;

        if let Err(error) = restore_connection(&mut connection, &backup_path) {
            let rollback = restore_connection(&mut connection, Path::new(&safety_backup_path));
            return match rollback {
                Ok(()) => Err(format!(
                    "Не удалось восстановить выбранную версию: {error}. Текущая база возвращена."
                )),
                Err(rollback_error) => Err(format!(
                    "Не удалось восстановить выбранную версию: {error}. \
                     Автоматический возврат также завершился ошибкой: {rollback_error}"
                )),
            };
        }

        Ok(RestoreBackupResult { safety_backup_path })
    }

    pub fn create_profile_snapshot(
        &self,
        profile_id: i64,
    ) -> Result<ProfileSnapshotSummary, String> {
        let database_path = self
            .path
            .as_deref()
            .ok_or_else(|| "Снимки профиля недоступны для временной базы.".to_owned())?;
        let connection = self.lock().map_err(|error| error.to_string())?;
        let path = write_profile_snapshot(&connection, database_path, profile_id, "manual")?;
        profile_snapshot_summary(profile_id, &path)
    }

    pub fn list_profile_snapshots(
        &self,
        profile_id: i64,
    ) -> Result<Vec<ProfileSnapshotSummary>, String> {
        let Some(database_path) = self.path.as_deref() else {
            return Ok(Vec::new());
        };
        let connection = self.lock().map_err(|error| error.to_string())?;
        ensure_profile_exists(&connection, profile_id)?;
        drop(connection);

        let directory = profile_snapshot_directory(database_path, profile_id)?;
        if !directory.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = fs::read_dir(&directory)
            .map_err(|error| error.to_string())?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| profile_snapshot_summary(profile_id, &entry.path()).ok())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| right.modified_at_ms.cmp(&left.modified_at_ms));
        Ok(snapshots)
    }

    pub fn restore_profile_snapshot(
        &self,
        profile_id: i64,
        file_name: &str,
    ) -> Result<RestoreProfileSnapshotResult, String> {
        let database_path = self
            .path
            .as_deref()
            .ok_or_else(|| "Снимки профиля недоступны для временной базы.".to_owned())?;
        let snapshot_path = resolve_profile_snapshot_path(database_path, profile_id, file_name)?;
        let json = fs::read_to_string(&snapshot_path).map_err(|error| error.to_string())?;
        let configuration = parse_configuration(&json)?;
        if configuration.profiles.len() != 1 {
            return Err("Снимок должен содержать ровно один профиль.".into());
        }

        let mut connection = self.lock().map_err(|error| error.to_string())?;
        ensure_profile_exists(&connection, profile_id)?;
        let safety_snapshot_path =
            write_profile_snapshot(&connection, database_path, profile_id, "before-restore")?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        replace_profile_configuration(&transaction, profile_id, &configuration.profiles[0])?;
        transaction.commit().map_err(|error| error.to_string())?;

        Ok(RestoreProfileSnapshotResult {
            safety_snapshot_path: safety_snapshot_path.display().to_string(),
        })
    }
}

fn build_configuration(
    connection: &Connection,
    selected_profile_id: Option<i64>,
) -> Result<ConfigurationFile, String> {
    let exported_at: String = connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;

    let mut statement = connection
        .prepare(
            "SELECT id, name
             FROM profiles
             WHERE ?1 IS NULL OR id = ?1
             ORDER BY name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let profile_rows = statement
        .query_map([selected_profile_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    if profile_rows.is_empty() {
        return Err("Профиль для экспорта не найден.".into());
    }

    let mut profiles = Vec::with_capacity(profile_rows.len());
    for (profile_id, name) in profile_rows {
        let categories = {
            let mut statement = connection
                .prepare(
                    "SELECT path
                     FROM categories
                     WHERE profile_id = ?1
                     ORDER BY path COLLATE NOCASE",
                )
                .map_err(|error| error.to_string())?;
            statement
                .query_map([profile_id], |row| {
                    Ok(ConfigurationCategory { path: row.get(0)? })
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?
        };

        let phrases = {
            let mut statement = connection
                .prepare(
                    "SELECT
                        phrases.title,
                        phrases.snippet,
                        categories.path,
                        phrases.body,
                        phrases.description,
                        phrases.is_enabled
                     FROM phrases
                     LEFT JOIN categories ON categories.id = phrases.category_id
                     WHERE phrases.profile_id = ?1
                     ORDER BY phrases.title COLLATE NOCASE",
                )
                .map_err(|error| error.to_string())?;
            statement
                .query_map([profile_id], |row| {
                    Ok(ConfigurationPhrase {
                        title: row.get(0)?,
                        snippet: row.get(1)?,
                        category_path: row.get(2)?,
                        content: row.get(3)?,
                        description: row.get(4)?,
                        is_enabled: row.get::<_, i64>(5)? == 1,
                    })
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?
        };

        let words = {
            let mut statement = connection
                .prepare(
                    "SELECT
                        dictionary_words.word,
                        categories.path,
                        dictionary_words.priority,
                        dictionary_words.is_enabled,
                        dictionary_words.autocomplete_enabled,
                        dictionary_words.autocorrect_enabled
                     FROM dictionary_words
                     LEFT JOIN categories ON categories.id = dictionary_words.category_id
                     WHERE dictionary_words.profile_id = ?1
                     ORDER BY dictionary_words.word COLLATE NOCASE",
                )
                .map_err(|error| error.to_string())?;
            statement
                .query_map([profile_id], |row| {
                    Ok(ConfigurationWord {
                        word: row.get(0)?,
                        category_path: row.get(1)?,
                        priority: row.get(2)?,
                        is_enabled: row.get::<_, i64>(3)? == 1,
                        allow_autocomplete: row.get::<_, i64>(4)? == 1,
                        allow_autocorrect: row.get::<_, i64>(5)? == 1,
                    })
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?
        };

        profiles.push(ConfigurationProfile {
            name,
            categories,
            phrases,
            words,
        });
    }

    Ok(ConfigurationFile {
        version: CONFIGURATION_VERSION,
        exported_at,
        profiles,
    })
}

fn parse_configuration(json: &str) -> Result<ConfigurationFile, String> {
    if json.len() > MAX_IMPORT_BYTES {
        return Err("JSON-файл превышает допустимый размер 10 МБ.".into());
    }

    let mut configuration: ConfigurationFile =
        serde_json::from_str(json).map_err(|error| format!("Некорректный JSON: {error}"))?;
    if configuration.version != CONFIGURATION_VERSION {
        return Err(format!(
            "Версия конфигурации {} не поддерживается. Ожидается версия {CONFIGURATION_VERSION}.",
            configuration.version
        ));
    }
    if configuration.exported_at.trim().is_empty() {
        return Err("В конфигурации отсутствует дата экспорта.".into());
    }
    if configuration.profiles.is_empty() {
        return Err("В конфигурации нет профилей.".into());
    }

    normalize_and_validate(&mut configuration)?;
    Ok(configuration)
}

fn normalize_and_validate(configuration: &mut ConfigurationFile) -> Result<(), String> {
    let mut profile_names = HashSet::new();
    for profile in &mut configuration.profiles {
        profile.name = profile.name.trim().to_owned();
        validate_profile_name(&profile.name)
            .map_err(|_| "Название профиля не может быть пустым.")?;
        if !profile_names.insert(profile.name.to_lowercase()) {
            return Err(format!(
                "Профиль «{}» указан в конфигурации несколько раз.",
                profile.name
            ));
        }

        let mut paths = BTreeSet::new();
        for category in &mut profile.categories {
            category.path = normalize_category_path(&category.path)?;
            add_path_with_parents(&mut paths, &category.path);
        }

        let mut snippets = HashSet::new();
        for phrase in &mut profile.phrases {
            phrase.title = phrase.title.trim().to_owned();
            phrase.snippet = normalize_snippet(&phrase.snippet);
            phrase.content = phrase.content.trim().to_owned();
            phrase.description = phrase.description.trim().to_owned();
            phrase.category_path = normalize_optional_path(phrase.category_path.take())?;
            if let Some(path) = &phrase.category_path {
                add_path_with_parents(&mut paths, path);
            }
            validate_phrase(&phrase.title, &phrase.snippet, &phrase.content)
                .map_err(|_| format!("Фраза «{}» содержит некорректные данные.", phrase.title))?;
            if !snippets.insert(phrase.snippet.to_lowercase()) {
                return Err(format!(
                    "Сниппет «{}» указан в профиле «{}» несколько раз.",
                    phrase.snippet, profile.name
                ));
            }
        }

        let mut words = HashSet::new();
        for word in &mut profile.words {
            word.word = word.word.trim().to_owned();
            word.category_path = normalize_optional_path(word.category_path.take())?;
            if let Some(path) = &word.category_path {
                add_path_with_parents(&mut paths, path);
            }
            validate_dictionary_word(&word.word)
                .map_err(|_| format!("Слово «{}» содержит недопустимые символы.", word.word))?;
            if !(0..=100).contains(&word.priority) {
                return Err(format!(
                    "Приоритет слова «{}» должен быть от 0 до 100.",
                    word.word
                ));
            }
            if !words.insert(word.word.to_lowercase()) {
                return Err(format!(
                    "Слово «{}» указано в профиле «{}» несколько раз.",
                    word.word, profile.name
                ));
            }
        }

        profile.categories = paths
            .into_iter()
            .map(|path| ConfigurationCategory { path })
            .collect();
    }

    Ok(())
}

fn preview_configuration(
    connection: &Connection,
    configuration: &ConfigurationFile,
) -> Result<ImportPreview, String> {
    let counts = configuration_counts(configuration);
    let single_profile_target = single_profile_target(connection, configuration)?;
    let mut snippet_conflicts = 0;
    let mut word_conflicts = 0;

    for profile in &configuration.profiles {
        let profile_id = match single_profile_target.as_ref() {
            Some((profile_id, _)) => Some(*profile_id),
            None => find_profile_id(connection, &profile.name)?,
        };
        let Some(profile_id) = profile_id else {
            continue;
        };

        for phrase in &profile.phrases {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM phrases
                        WHERE profile_id = ?1 AND snippet_key = ?2
                     )",
                    params![profile_id, normalize_case_key(&phrase.snippet)],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            snippet_conflicts += u32::from(exists);
        }

        for word in &profile.words {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM dictionary_words
                        WHERE profile_id = ?1 AND word_key = ?2
                     )",
                    params![profile_id, normalize_case_key(&word.word)],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            word_conflicts += u32::from(exists);
        }
    }

    Ok(ImportPreview {
        version: configuration.version,
        target_profile_name: single_profile_target.map(|(_, name)| name),
        profile_count: counts.profile_count,
        category_count: counts.category_count,
        phrase_count: counts.phrase_count,
        word_count: counts.word_count,
        snippet_conflicts,
        word_conflicts,
    })
}

fn apply_configuration(
    transaction: &Transaction<'_>,
    configuration: &ConfigurationFile,
    strategy: ImportConflictStrategy,
    backup_path: Option<String>,
) -> Result<ImportResult, String> {
    let single_profile_target = single_profile_target(transaction, configuration)?;
    let mut profile_count = 0;
    let mut category_count = 0;
    let mut phrase_count = 0;
    let mut word_count = 0;
    let mut skipped_conflicts = 0;

    for profile in &configuration.profiles {
        let profile_id = match single_profile_target.as_ref() {
            Some((profile_id, _)) => *profile_id,
            None => match find_profile_id(transaction, &profile.name)? {
                Some(profile_id) => profile_id,
                None => {
                    transaction
                        .execute(
                            "INSERT INTO profiles (name, is_active) VALUES (?1, 0)",
                            [&profile.name],
                        )
                        .map_err(|error| error.to_string())?;
                    profile_count += 1;
                    transaction.last_insert_rowid()
                }
            },
        };

        let mut category_ids = HashMap::new();
        for category in &profile.categories {
            let (category_id, created) =
                ensure_category_path(transaction, profile_id, &category.path, &mut category_ids)?;
            category_ids.insert(category.path.clone(), category_id);
            category_count += u32::from(created);
        }

        for phrase in &profile.phrases {
            let category_id = phrase
                .category_path
                .as_ref()
                .and_then(|path| category_ids.get(path))
                .copied();
            let existing_id: Option<i64> = transaction
                .query_row(
                    "SELECT id FROM phrases
                     WHERE profile_id = ?1 AND snippet_key = ?2",
                    params![profile_id, normalize_case_key(&phrase.snippet)],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;

            match (existing_id, strategy) {
                (Some(_), ImportConflictStrategy::Skip) => skipped_conflicts += 1,
                (Some(phrase_id), ImportConflictStrategy::Overwrite) => {
                    transaction
                        .execute(
                            "UPDATE phrases
                             SET category_id = ?1,
                                 title = ?2,
                                 snippet = ?3,
                                 snippet_key = ?4,
                                 body = ?5,
                                 description = ?6,
                                 is_enabled = ?7,
                                 updated_at = CURRENT_TIMESTAMP
                             WHERE id = ?8",
                            params![
                                category_id,
                                phrase.title,
                                phrase.snippet,
                                normalize_case_key(&phrase.snippet),
                                phrase.content,
                                phrase.description,
                                i64::from(phrase.is_enabled),
                                phrase_id
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    phrase_count += 1;
                }
                (None, _) => {
                    transaction
                        .execute(
                            "INSERT INTO phrases (
                                profile_id, category_id, title, snippet, snippet_key, body,
                                description, is_enabled
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![
                                profile_id,
                                category_id,
                                phrase.title,
                                phrase.snippet,
                                normalize_case_key(&phrase.snippet),
                                phrase.content,
                                phrase.description,
                                i64::from(phrase.is_enabled)
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    phrase_count += 1;
                }
            }
        }

        for word in &profile.words {
            let category_id = word
                .category_path
                .as_ref()
                .and_then(|path| category_ids.get(path))
                .copied();
            let existing_id: Option<i64> = transaction
                .query_row(
                    "SELECT id FROM dictionary_words
                     WHERE profile_id = ?1 AND word_key = ?2",
                    params![profile_id, normalize_case_key(&word.word)],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;

            match (existing_id, strategy) {
                (Some(_), ImportConflictStrategy::Skip) => skipped_conflicts += 1,
                (Some(word_id), ImportConflictStrategy::Overwrite) => {
                    transaction
                        .execute(
                            "UPDATE dictionary_words
                             SET category_id = ?1,
                                 word = ?2,
                                 word_key = ?3,
                                 priority = ?4,
                                 is_enabled = ?5,
                                 autocomplete_enabled = ?6,
                                 autocorrect_enabled = ?7,
                                 updated_at = CURRENT_TIMESTAMP
                             WHERE id = ?8",
                            params![
                                category_id,
                                word.word,
                                normalize_case_key(&word.word),
                                word.priority,
                                i64::from(word.is_enabled),
                                i64::from(word.allow_autocomplete),
                                i64::from(word.allow_autocorrect),
                                word_id
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    word_count += 1;
                }
                (None, _) => {
                    transaction
                        .execute(
                            "INSERT INTO dictionary_words (
                                profile_id,
                                category_id,
                                word,
                                word_key,
                                priority,
                                is_enabled,
                                autocomplete_enabled,
                                autocorrect_enabled
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![
                                profile_id,
                                category_id,
                                word.word,
                                normalize_case_key(&word.word),
                                word.priority,
                                i64::from(word.is_enabled),
                                i64::from(word.allow_autocomplete),
                                i64::from(word.allow_autocorrect)
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    word_count += 1;
                }
            }
        }
    }

    // Preserve the current active profile even when all imported profiles are new.
    active_profile_id(transaction).map_err(|error| error.to_string())?;

    Ok(ImportResult {
        profile_count,
        category_count,
        phrase_count,
        word_count,
        skipped_conflicts,
        backup_path,
    })
}

fn replace_profile_configuration(
    transaction: &Transaction<'_>,
    profile_id: i64,
    profile: &ConfigurationProfile,
) -> Result<(), String> {
    ensure_profile_exists(transaction, profile_id)?;
    transaction
        .execute("DELETE FROM phrases WHERE profile_id = ?1", [profile_id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM dictionary_words WHERE profile_id = ?1",
            [profile_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM categories WHERE profile_id = ?1", [profile_id])
        .map_err(|error| error.to_string())?;

    let mut category_ids = HashMap::new();
    for category in &profile.categories {
        let (category_id, _) =
            ensure_category_path(transaction, profile_id, &category.path, &mut category_ids)?;
        category_ids.insert(category.path.clone(), category_id);
    }

    for phrase in &profile.phrases {
        let category_id = phrase
            .category_path
            .as_ref()
            .and_then(|path| category_ids.get(path))
            .copied();
        transaction
            .execute(
                "INSERT INTO phrases (
                    profile_id, category_id, title, snippet, snippet_key, body,
                    description, is_enabled
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    profile_id,
                    category_id,
                    phrase.title,
                    phrase.snippet,
                    normalize_case_key(&phrase.snippet),
                    phrase.content,
                    phrase.description,
                    i64::from(phrase.is_enabled)
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    for word in &profile.words {
        let category_id = word
            .category_path
            .as_ref()
            .and_then(|path| category_ids.get(path))
            .copied();
        transaction
            .execute(
                "INSERT INTO dictionary_words (
                    profile_id, category_id, word, word_key, priority, is_enabled,
                    autocomplete_enabled, autocorrect_enabled
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    profile_id,
                    category_id,
                    word.word,
                    normalize_case_key(&word.word),
                    word.priority,
                    i64::from(word.is_enabled),
                    i64::from(word.allow_autocomplete),
                    i64::from(word.allow_autocorrect)
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn ensure_category_path(
    transaction: &Transaction<'_>,
    profile_id: i64,
    path: &str,
    known_ids: &mut HashMap<String, i64>,
) -> Result<(i64, bool), String> {
    let mut parent_id = None;
    let mut current_path = String::new();
    let mut final_id = 0;
    let mut final_created = false;

    for part in path.split('/') {
        if !current_path.is_empty() {
            current_path.push('/');
        }
        current_path.push_str(part);

        if let Some(category_id) = known_ids.get(&current_path).copied() {
            parent_id = Some(category_id);
            final_id = category_id;
            continue;
        }

        let existing_id: Option<i64> = transaction
            .query_row(
                "SELECT id FROM categories WHERE profile_id = ?1 AND path = ?2",
                params![profile_id, current_path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        let (category_id, created) = match existing_id {
            Some(category_id) => (category_id, false),
            None => {
                transaction
                    .execute(
                        "INSERT INTO categories (profile_id, parent_id, name, path)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![profile_id, parent_id, part, current_path],
                    )
                    .map_err(|error| error.to_string())?;
                (transaction.last_insert_rowid(), true)
            }
        };
        known_ids.insert(current_path.clone(), category_id);
        parent_id = Some(category_id);
        final_id = category_id;
        final_created = created;
    }

    Ok((final_id, final_created))
}

fn create_backup(
    connection: &Connection,
    database_path: Option<&Path>,
    reason: &str,
) -> Result<Option<String>, String> {
    let Some(database_path) = database_path else {
        return Ok(None);
    };
    let backup_directory = backup_directory(database_path)?;
    fs::create_dir_all(&backup_directory).map_err(|error| error.to_string())?;

    let timestamp: String = connection
        .query_row("SELECT strftime('%Y%m%d-%H%M%S', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let backup_path =
        unique_path(backup_directory.join(format!("textpilot-{reason}-{timestamp}.db")));
    let backup_path_string = backup_path.to_string_lossy().into_owned();
    connection
        .execute("VACUUM INTO ?1", [&backup_path_string])
        .map_err(|error| format!("Не удалось создать backup базы: {error}"))?;

    Ok(Some(backup_path.display().to_string()))
}

fn backup_directory(database_path: &Path) -> Result<PathBuf, String> {
    database_path
        .parent()
        .map(|parent| parent.join("backups"))
        .ok_or_else(|| "Не удалось определить папку базы данных.".to_owned())
}

fn profile_snapshot_directory(database_path: &Path, profile_id: i64) -> Result<PathBuf, String> {
    Ok(backup_directory(database_path)?
        .join("profiles")
        .join(format!("profile-{profile_id}")))
}

fn write_profile_snapshot(
    connection: &Connection,
    database_path: &Path,
    profile_id: i64,
    reason: &str,
) -> Result<PathBuf, String> {
    let configuration = build_configuration(connection, Some(profile_id))?;
    let profile = configuration
        .profiles
        .first()
        .ok_or_else(|| "Профиль для снимка не найден.".to_owned())?;
    let timestamp: String = connection
        .query_row("SELECT strftime('%Y%m%d-%H%M%S', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let directory = profile_snapshot_directory(database_path, profile_id)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = unique_path(directory.join(format!(
        "textpilot-profile-{profile_id}-{}-{reason}-{timestamp}.json",
        file_slug(&profile.name)
    )));
    let json = serde_json::to_string_pretty(&configuration).map_err(|error| error.to_string())?;
    fs::write(&path, format!("{json}\n")).map_err(|error| error.to_string())?;
    Ok(path)
}

fn profile_snapshot_summary(
    expected_profile_id: i64,
    path: &Path,
) -> Result<ProfileSnapshotSummary, String> {
    if !path.is_file()
        || path.extension() != Some(OsStr::new("json"))
        || !path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with(&format!("textpilot-profile-{expected_profile_id}-"))
        })
    {
        return Err("Некорректный файл снимка профиля.".into());
    }
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_at_ms = metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let configuration = parse_configuration(&json)?;
    if configuration.profiles.len() != 1 {
        return Err("Снимок должен содержать ровно один профиль.".into());
    }

    Ok(ProfileSnapshotSummary {
        file_name: path
            .file_name()
            .ok_or_else(|| "Некорректное имя снимка.".to_owned())?
            .to_string_lossy()
            .into_owned(),
        profile_id: expected_profile_id,
        profile_name: configuration.profiles[0].name.clone(),
        size_bytes: metadata.len(),
        modified_at_ms,
    })
}

fn resolve_profile_snapshot_path(
    database_path: &Path,
    profile_id: i64,
    file_name: &str,
) -> Result<PathBuf, String> {
    let requested = Path::new(file_name);
    let required_prefix = format!("textpilot-profile-{profile_id}-");
    if requested.file_name() != Some(OsStr::new(file_name))
        || requested.extension() != Some(OsStr::new("json"))
        || !file_name.starts_with(&required_prefix)
    {
        return Err("Некорректное имя снимка профиля.".into());
    }

    let directory = profile_snapshot_directory(database_path, profile_id)?;
    let path = directory.join(requested);
    if !path.is_file() {
        return Err("Снимок профиля не найден.".into());
    }

    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical_path = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_path.starts_with(canonical_directory) {
        return Err("Снимок находится вне разрешённой папки.".into());
    }
    Ok(canonical_path)
}

fn is_backup_file(path: &Path) -> bool {
    path.is_file()
        && path.extension() == Some(OsStr::new("db"))
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("textpilot-"))
}

fn resolve_backup_path(database_path: &Path, file_name: &str) -> Result<PathBuf, String> {
    let requested = Path::new(file_name);
    if requested.file_name() != Some(OsStr::new(file_name))
        || requested.extension() != Some(OsStr::new("db"))
        || !file_name.starts_with("textpilot-")
    {
        return Err("Некорректное имя резервной копии.".into());
    }

    let directory = backup_directory(database_path)?;
    let path = directory.join(requested);
    if !path.is_file() {
        return Err("Резервная копия не найдена.".into());
    }

    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical_path = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_path.starts_with(canonical_directory) {
        return Err("Резервная копия находится вне разрешённой папки.".into());
    }

    Ok(canonical_path)
}

fn validate_backup(path: &Path) -> Result<(), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("Не удалось открыть резервную копию: {error}"))?;
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("Не удалось проверить резервную копию: {error}"))?;
    if quick_check != "ok" {
        return Err(format!("Резервная копия повреждена: {quick_check}"));
    }
    let has_profiles: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'profiles'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !has_profiles {
        return Err("Файл не является резервной копией TextPilot.".into());
    }

    Ok(())
}

fn restore_connection(connection: &mut Connection, path: &Path) -> Result<(), String> {
    connection
        .restore(MAIN_DB, path, None::<fn(rusqlite::backup::Progress)>)
        .map_err(|error| error.to_string())?;
    initialize_connection(connection).map_err(|error| error.to_string())
}

fn ensure_profile_exists(connection: &Connection, profile_id: i64) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id = ?1)",
            [profile_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        Ok(())
    } else {
        Err("Профиль не найден.".into())
    }
}

fn find_profile_id(connection: &Connection, name: &str) -> Result<Option<i64>, String> {
    connection
        .query_row(
            "SELECT id FROM profiles WHERE lower(name) = lower(?1)",
            [name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn single_profile_target(
    connection: &Connection,
    configuration: &ConfigurationFile,
) -> Result<Option<(i64, String)>, String> {
    if configuration.profiles.len() != 1 {
        return Ok(None);
    }

    connection
        .query_row(
            "SELECT id, name
             FROM profiles
             ORDER BY is_active DESC, id ASC
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map(Some)
        .map_err(|error| error.to_string())
}

fn normalize_category_path(path: &str) -> Result<String, String> {
    let parts = path.split('/').map(str::trim).collect::<Vec<_>>();
    if parts.is_empty()
        || parts
            .iter()
            .any(|part| validate_category_name(part).is_err())
    {
        return Err(format!("Некорректный путь категории: «{path}»."));
    }
    Ok(parts.join("/"))
}

fn normalize_optional_path(path: Option<String>) -> Result<Option<String>, String> {
    path.filter(|path| !path.trim().is_empty())
        .map(|path| normalize_category_path(&path))
        .transpose()
}

fn normalize_snippet(snippet: &str) -> String {
    snippet
        .trim()
        .strip_prefix('/')
        .unwrap_or(snippet.trim())
        .to_owned()
}

fn add_path_with_parents(paths: &mut BTreeSet<String>, path: &str) {
    let mut current = String::new();
    for part in path.split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        paths.insert(current.clone());
    }
}

fn configuration_counts(configuration: &ConfigurationFile) -> ImportPreview {
    ImportPreview {
        version: configuration.version,
        target_profile_name: None,
        profile_count: configuration.profiles.len() as u32,
        category_count: configuration
            .profiles
            .iter()
            .map(|profile| profile.categories.len() as u32)
            .sum(),
        phrase_count: configuration
            .profiles
            .iter()
            .map(|profile| profile.phrases.len() as u32)
            .sum(),
        word_count: configuration
            .profiles
            .iter()
            .map(|profile| profile.words.len() as u32)
            .sum(),
        snippet_conflicts: 0,
        word_conflicts: 0,
    }
}

fn file_slug(value: &str) -> String {
    let slug = value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "profile".into()
    } else {
        slug
    }
}

fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("textpilot");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 2.. {
        let file_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{ImportConflictStrategy, parse_configuration};
    use crate::storage::{CreatePhraseInput, Database, DictionaryWordInput};

    fn temp_directory(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    fn configuration(snippet: &str, body: &str, word_priority: i32) -> String {
        format!(
            r#"{{
              "version": 1,
              "exported_at": "2026-06-07T12:00:00Z",
              "profiles": [{{
                "name": "Основной",
                "categories": [{{"path": "Клиенты/Сроки"}}],
                "phrases": [{{
                  "title": "Срок",
                  "snippet": "{snippet}",
                  "category_path": "Клиенты/Сроки",
                  "content": "{body}"
                }}],
                "words": [{{
                  "word": "согласование",
                  "category_path": "Клиенты",
                  "priority": {word_priority},
                  "allow_autocomplete": true,
                  "allow_autocorrect": true
                }}]
              }}]
            }}"#
        )
    }

    #[test]
    fn previews_and_imports_configuration() {
        let database = Database::open(":memory:").unwrap();
        let json = configuration("/сроки", "Три дня", 8);

        let preview = database.preview_import(&json).unwrap();
        assert_eq!(preview.profile_count, 1);
        assert_eq!(preview.category_count, 2);
        assert_eq!(preview.phrase_count, 1);
        assert_eq!(preview.word_count, 1);
        assert_eq!(preview.snippet_conflicts, 0);

        let result = database
            .import_configuration(&json, ImportConflictStrategy::Overwrite)
            .unwrap();
        assert_eq!(result.category_count, 2);
        assert_eq!(result.phrase_count, 1);
        assert_eq!(result.word_count, 1);

        let dashboard = database.dashboard_data().unwrap();
        assert_eq!(dashboard.phrases[0].snippet, "сроки");
        assert_eq!(dashboard.phrases[0].body, "Три дня");
        assert_eq!(dashboard.dictionary_words[0].priority, 8);
    }

    #[test]
    fn skips_or_overwrites_conflicts() {
        let database = Database::open(":memory:").unwrap();
        database
            .create_phrase(CreatePhraseInput {
                title: "Старый срок".into(),
                snippet: "сроки".into(),
                body: "Старый текст".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();
        database
            .save_dictionary_word(DictionaryWordInput {
                id: None,
                word: "согласование".into(),
                category_id: None,
                priority: 1,
                is_enabled: true,
                autocomplete_enabled: true,
                autocorrect_enabled: true,
            })
            .unwrap();

        let json = configuration("сроки", "Новый текст", 50);
        let preview = database.preview_import(&json).unwrap();
        assert_eq!(preview.snippet_conflicts, 1);
        assert_eq!(preview.word_conflicts, 1);

        let skipped = database
            .import_configuration(&json, ImportConflictStrategy::Skip)
            .unwrap();
        assert_eq!(skipped.skipped_conflicts, 2);
        let dashboard = database.dashboard_data().unwrap();
        assert_eq!(dashboard.phrases[0].body, "Старый текст");
        assert_eq!(dashboard.dictionary_words[0].priority, 1);

        database
            .import_configuration(&json, ImportConflictStrategy::Overwrite)
            .unwrap();
        let dashboard = database.dashboard_data().unwrap();
        assert_eq!(dashboard.phrases[0].body, "Новый текст");
        assert_eq!(dashboard.dictionary_words[0].priority, 50);
    }

    #[test]
    fn single_profile_import_uses_the_current_active_profile() {
        let database = Database::open(":memory:").unwrap();
        database
            .create_phrase(CreatePhraseInput {
                title: "Фраза первого профиля".into(),
                snippet: "сроки".into(),
                body: "Первый профиль".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();
        database.create_profile("Второй").unwrap();

        let json = configuration("сроки", "Второй профиль", 10);
        let preview = database.preview_import(&json).unwrap();
        assert_eq!(preview.target_profile_name.as_deref(), Some("Второй"));
        assert_eq!(preview.snippet_conflicts, 0);

        database
            .import_configuration(&json, ImportConflictStrategy::Overwrite)
            .unwrap();
        let second_dashboard = database.dashboard_data().unwrap();
        assert_eq!(second_dashboard.phrases.len(), 1);
        assert_eq!(second_dashboard.phrases[0].body, "Второй профиль");

        let first_profile_id = second_dashboard
            .profiles
            .iter()
            .find(|profile| profile.name == "Основной")
            .unwrap()
            .id;
        database.set_active_profile(first_profile_id).unwrap();
        let first_dashboard = database.dashboard_data().unwrap();
        assert_eq!(first_dashboard.phrases.len(), 1);
        assert_eq!(first_dashboard.phrases[0].body, "Первый профиль");
    }

    #[test]
    fn rejects_duplicate_items_inside_file() {
        let json = r#"{
          "version": 1,
          "exported_at": "2026-06-07T12:00:00Z",
          "profiles": [{
            "name": "Основной",
            "phrases": [
              {"title": "А", "snippet": "кп", "content": "Один"},
              {"title": "Б", "snippet": "КП", "content": "Два"}
            ]
          }]
        }"#;

        assert!(parse_configuration(json).is_err());
    }

    #[test]
    fn creates_file_backup_before_import() {
        let directory = temp_directory("textpilot-transfer-test");
        fs::create_dir_all(&directory).unwrap();
        let database_path = directory.join("textpilot.db");
        let database = Database::open(&database_path).unwrap();

        let result = database
            .import_configuration(
                &configuration("сроки", "Три дня", 8),
                ImportConflictStrategy::Overwrite,
            )
            .unwrap();
        let backup_path = result.backup_path.expect("backup path should be returned");
        assert!(std::path::Path::new(&backup_path).is_file());
        assert!(fs::metadata(&backup_path).unwrap().len() > 0);

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restores_whole_database_from_backup() {
        let directory = temp_directory("textpilot-restore-test");
        fs::create_dir_all(&directory).unwrap();
        let database_path = directory.join("textpilot.db");
        let database = Database::open(&database_path).unwrap();
        database
            .create_phrase(CreatePhraseInput {
                title: "Исходная".into(),
                snippet: "старт".into(),
                body: "Старый текст".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();

        database
            .import_configuration(
                &configuration("после", "Новый текст", 10),
                ImportConflictStrategy::Overwrite,
            )
            .unwrap();
        let backup = database
            .list_backups()
            .unwrap()
            .into_iter()
            .find(|backup| backup.file_name.contains("before-import"))
            .unwrap();
        assert_eq!(database.dashboard_data().unwrap().phrases.len(), 2);

        let result = database.restore_backup(&backup.file_name).unwrap();
        let dashboard = database.dashboard_data().unwrap();

        assert!(std::path::Path::new(&result.safety_backup_path).is_file());
        assert_eq!(dashboard.phrases.len(), 1);
        assert_eq!(dashboard.phrases[0].snippet, "старт");
        assert_eq!(dashboard.phrases[0].body, "Старый текст");
        assert!(
            database
                .list_backups()
                .unwrap()
                .iter()
                .any(|backup| backup.file_name.contains("before-restore"))
        );

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restores_only_selected_profile_from_snapshot() {
        let directory = temp_directory("textpilot-profile-restore-test");
        fs::create_dir_all(&directory).unwrap();
        let database = Database::open(directory.join("textpilot.db")).unwrap();
        let first_profile_id = database.dashboard_data().unwrap().active_profile_id;
        database
            .create_phrase(CreatePhraseInput {
                title: "Первая версия".into(),
                snippet: "первая".into(),
                body: "Исходный текст".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();
        let snapshot = database.create_profile_snapshot(first_profile_id).unwrap();

        database.create_profile("Второй").unwrap();
        let second_profile_id = database.dashboard_data().unwrap().active_profile_id;
        database
            .create_phrase(CreatePhraseInput {
                title: "Второй профиль".into(),
                snippet: "второй".into(),
                body: "Не менять".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();

        database.set_active_profile(first_profile_id).unwrap();
        database
            .rename_profile(first_profile_id, "Переименованный")
            .unwrap();
        database
            .create_phrase(CreatePhraseInput {
                title: "Лишняя версия".into(),
                snippet: "лишняя".into(),
                body: "Удалить при восстановлении".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();
        let result = database
            .restore_profile_snapshot(first_profile_id, &snapshot.file_name)
            .unwrap();

        let first_dashboard = database.dashboard_data().unwrap();
        assert_eq!(first_dashboard.active_profile_id, first_profile_id);
        assert_eq!(
            first_dashboard
                .profiles
                .iter()
                .find(|profile| profile.id == first_profile_id)
                .unwrap()
                .name,
            "Переименованный"
        );
        assert_eq!(first_dashboard.phrases.len(), 1);
        assert_eq!(first_dashboard.phrases[0].snippet, "первая");
        assert!(std::path::Path::new(&result.safety_snapshot_path).is_file());

        database.set_active_profile(second_profile_id).unwrap();
        let second_dashboard = database.dashboard_data().unwrap();
        assert_eq!(second_dashboard.phrases.len(), 1);
        assert_eq!(second_dashboard.phrases[0].snippet, "второй");
        assert_eq!(second_dashboard.phrases[0].body, "Не менять");
        assert!(
            database
                .list_profile_snapshots(second_profile_id)
                .unwrap()
                .is_empty()
        );

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exports_to_the_selected_path() {
        let directory = temp_directory("textpilot-export-test");
        fs::create_dir_all(&directory).unwrap();
        let database = Database::open(directory.join("textpilot.db")).unwrap();
        let selected_path = directory.join("my-textpilot-backup.json");

        let result = database.export_configuration(None, &selected_path).unwrap();

        assert_eq!(result.path, selected_path.display().to_string());
        assert!(selected_path.is_file());
        assert!(
            fs::read_to_string(&selected_path)
                .unwrap()
                .contains("\"version\": 1")
        );

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
