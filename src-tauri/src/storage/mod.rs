use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

use crate::core::{autocomplete::AutocompleteEntry, snippets::is_valid_trigger};

mod transfer;

pub use transfer::{
    BackupSummary, ExportResult, ImportConflictStrategy, ImportPreview, ImportResult,
    ProfileSnapshotSummary, RestoreBackupResult, RestoreProfileSnapshotResult,
};

const INITIAL_SCHEMA: &str = include_str!("schema.sql");
const INTEGRITY_TRIGGERS: &str = include_str!("integrity_triggers.sql");
const CURRENT_SCHEMA_VERSION: u32 = 5;

pub struct Database {
    connection: Mutex<Connection>,
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
    pub category_count: u32,
    pub phrase_count: u32,
    pub dictionary_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub phrase_count: u32,
    pub word_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhraseSummary {
    pub id: i64,
    pub category_id: Option<i64>,
    pub title: String,
    pub snippet: String,
    pub body: String,
    pub description: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchItem {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub shortcut: String,
    pub body: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryWordSummary {
    pub id: i64,
    pub category_id: Option<i64>,
    pub word: String,
    pub priority: i32,
    pub is_enabled: bool,
    pub autocomplete_enabled: bool,
    pub autocorrect_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub profiles: Vec<ProfileSummary>,
    pub active_profile_id: i64,
    pub categories: Vec<CategorySummary>,
    pub phrases: Vec<PhraseSummary>,
    pub dictionary_words: Vec<DictionaryWordSummary>,
    pub dictionary_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCategoryInput {
    pub name: String,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameCategoryInput {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderCategoryInput {
    pub category_id: i64,
    pub target_category_id: i64,
    pub place_after: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePhraseInput {
    pub title: String,
    pub snippet: String,
    pub body: String,
    pub description: String,
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePhraseInput {
    pub id: i64,
    pub title: String,
    pub snippet: String,
    pub body: String,
    pub description: String,
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryWordInput {
    pub id: Option<i64>,
    pub word: String,
    pub category_id: Option<i64>,
    pub priority: i32,
    pub is_enabled: bool,
    pub autocomplete_enabled: bool,
    pub autocorrect_enabled: bool,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let database_path = (path != Path::new(":memory:")).then(|| path.to_path_buf());
        let mut connection = Connection::open(path)?;
        initialize_connection(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
            path: database_path,
        })
    }

    pub fn profile_count(&self) -> Result<u32> {
        self.lock()?
            .query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))
    }

    pub fn dashboard_data(&self) -> Result<DashboardData> {
        let connection = self.lock()?;
        let active_profile_id = connection.query_row(
            "SELECT id FROM profiles ORDER BY is_active DESC, id ASC LIMIT 1",
            [],
            |row| row.get(0),
        )?;

        let profiles = {
            let mut statement = connection.prepare(
                "SELECT
                    profiles.id,
                    profiles.name,
                    profiles.is_active,
                    (SELECT COUNT(*) FROM categories WHERE profile_id = profiles.id),
                    (SELECT COUNT(*) FROM phrases WHERE profile_id = profiles.id),
                    (SELECT COUNT(*) FROM dictionary_words WHERE profile_id = profiles.id)
                 FROM profiles
                 ORDER BY is_active DESC, name",
            )?;
            statement
                .query_map([], |row| {
                    Ok(ProfileSummary {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        is_active: row.get::<_, i64>(2)? == 1,
                        category_count: row.get(3)?,
                        phrase_count: row.get(4)?,
                        dictionary_count: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        let categories = {
            let mut statement = connection.prepare(
                "WITH RECURSIVE category_tree (
                    id, parent_id, name, path, sort_path
                 ) AS (
                    SELECT
                        id,
                        parent_id,
                        name,
                        path,
                        printf('%020d-%020d', sort_order, id)
                    FROM categories
                    WHERE profile_id = ?1 AND parent_id IS NULL

                    UNION ALL

                    SELECT
                        child.id,
                        child.parent_id,
                        child.name,
                        child.path,
                        category_tree.sort_path || '/' ||
                            printf('%020d-%020d', child.sort_order, child.id)
                    FROM categories AS child
                    JOIN category_tree ON child.parent_id = category_tree.id
                    WHERE child.profile_id = ?1
                 )
                 SELECT
                    category_tree.id,
                    category_tree.parent_id,
                    category_tree.name,
                    category_tree.path,
                    (SELECT COUNT(*) FROM phrases WHERE category_id = category_tree.id),
                    (SELECT COUNT(*) FROM dictionary_words WHERE category_id = category_tree.id)
                 FROM category_tree
                 ORDER BY category_tree.sort_path",
            )?;
            statement
                .query_map([active_profile_id], |row| {
                    Ok(CategorySummary {
                        id: row.get(0)?,
                        parent_id: row.get(1)?,
                        name: row.get(2)?,
                        path: row.get(3)?,
                        phrase_count: row.get(4)?,
                        word_count: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        let phrases = {
            let mut statement = connection.prepare(
                "SELECT id, category_id, title, snippet, body, description, is_enabled
                 FROM phrases
                 WHERE profile_id = ?1
                 ORDER BY updated_at DESC, title",
            )?;
            statement
                .query_map([active_profile_id], |row| {
                    Ok(PhraseSummary {
                        id: row.get(0)?,
                        category_id: row.get(1)?,
                        title: row.get(2)?,
                        snippet: row.get(3)?,
                        body: row.get(4)?,
                        description: row.get(5)?,
                        is_enabled: row.get::<_, i64>(6)? == 1,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        let dictionary_count = connection.query_row(
            "SELECT COUNT(*) FROM dictionary_words WHERE profile_id = ?1",
            [active_profile_id],
            |row| row.get(0),
        )?;

        let dictionary_words = {
            let mut statement = connection.prepare(
                "SELECT
                    id,
                    category_id,
                    word,
                    priority,
                    is_enabled,
                    autocomplete_enabled,
                    autocorrect_enabled
                 FROM dictionary_words
                 WHERE profile_id = ?1
                 ORDER BY priority DESC, word COLLATE NOCASE",
            )?;
            statement
                .query_map([active_profile_id], |row| {
                    Ok(DictionaryWordSummary {
                        id: row.get(0)?,
                        category_id: row.get(1)?,
                        word: row.get(2)?,
                        priority: row.get(3)?,
                        is_enabled: row.get::<_, i64>(4)? == 1,
                        autocomplete_enabled: row.get::<_, i64>(5)? == 1,
                        autocorrect_enabled: row.get::<_, i64>(6)? == 1,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        Ok(DashboardData {
            profiles,
            active_profile_id,
            categories,
            phrases,
            dictionary_words,
            dictionary_count,
        })
    }

    pub fn create_profile(&self, name: &str) -> Result<()> {
        let name = validate_profile_name(name)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;

        transaction.execute(
            "UPDATE profiles
             SET is_active = 0, updated_at = CURRENT_TIMESTAMP
             WHERE is_active = 1",
            [],
        )?;
        transaction.execute(
            "INSERT INTO profiles (name, is_active) VALUES (?1, 1)",
            [name],
        )?;
        transaction.commit()
    }

    pub fn rename_profile(&self, profile_id: i64, name: &str) -> Result<()> {
        let name = validate_profile_name(name)?;
        let connection = self.lock()?;
        let updated = connection.execute(
            "UPDATE profiles
             SET name = ?1, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?2",
            params![name, profile_id],
        )?;

        ensure_profile_changed(updated)
    }

    pub fn set_active_profile(&self, profile_id: i64) -> Result<()> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id = ?1)",
            [profile_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        transaction.execute(
            "UPDATE profiles
             SET is_active = 0, updated_at = CURRENT_TIMESTAMP
             WHERE is_active = 1",
            [],
        )?;
        transaction.execute(
            "UPDATE profiles
             SET is_active = 1, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?1",
            [profile_id],
        )?;
        transaction.commit()
    }

    pub fn delete_profile(&self, profile_id: i64) -> Result<()> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let profile_count: u32 =
            transaction.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;
        if profile_count <= 1 {
            return Err(rusqlite::Error::InvalidParameterName(
                "cannot delete the last profile".into(),
            ));
        }

        let was_active: bool = transaction.query_row(
            "SELECT is_active FROM profiles WHERE id = ?1",
            [profile_id],
            |row| Ok(row.get::<_, i64>(0)? == 1),
        )?;
        transaction.execute("DELETE FROM profiles WHERE id = ?1", [profile_id])?;

        if was_active {
            transaction.execute(
                "UPDATE profiles
                 SET is_active = 1, updated_at = CURRENT_TIMESTAMP
                 WHERE id = (SELECT id FROM profiles ORDER BY name LIMIT 1)",
                [],
            )?;
        }

        transaction.commit()
    }

    pub fn create_category(&self, input: CreateCategoryInput) -> Result<()> {
        let name = validate_category_name(&input.name)?;

        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let path = match input.parent_id {
            Some(parent_id) => {
                let parent_path: String = connection.query_row(
                    "SELECT path FROM categories WHERE id = ?1 AND profile_id = ?2",
                    params![parent_id, active_profile_id],
                    |row| row.get(0),
                )?;
                format!("{parent_path}/{name}")
            }
            None => name.to_owned(),
        };

        connection.execute(
            "INSERT INTO categories (profile_id, parent_id, name, path, sort_order)
             VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                COALESCE((
                    SELECT MAX(sort_order) + 1
                    FROM categories
                    WHERE profile_id = ?1
                      AND parent_id IS ?2
                ), 0)
             )",
            params![active_profile_id, input.parent_id, name, path],
        )?;

        Ok(())
    }

    pub fn rename_category(&self, input: RenameCategoryInput) -> Result<()> {
        let name = validate_category_name(&input.name)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let active_profile_id = active_profile_id(&transaction)?;
        let (old_path, parent_id): (String, Option<i64>) = transaction.query_row(
            "SELECT path, parent_id
             FROM categories
             WHERE id = ?1 AND profile_id = ?2",
            params![input.id, active_profile_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let new_path = match parent_id {
            Some(parent_id) => {
                let parent_path: String = transaction.query_row(
                    "SELECT path
                     FROM categories
                     WHERE id = ?1 AND profile_id = ?2",
                    params![parent_id, active_profile_id],
                    |row| row.get(0),
                )?;
                format!("{parent_path}/{name}")
            }
            None => name.to_owned(),
        };

        transaction.execute(
            "UPDATE categories
             SET name = ?1, path = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?3 AND profile_id = ?4",
            params![name, new_path, input.id, active_profile_id],
        )?;
        transaction.execute(
            "UPDATE categories
             SET path = ?1 || substr(path, length(?2) + 1),
                 updated_at = CURRENT_TIMESTAMP
             WHERE profile_id = ?3 AND path LIKE ?2 || '/%'",
            params![new_path, old_path, active_profile_id],
        )?;
        transaction.commit()
    }

    pub fn delete_category(&self, category_id: i64) -> Result<()> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let deleted = connection.execute(
            "DELETE FROM categories WHERE id = ?1 AND profile_id = ?2",
            params![category_id, active_profile_id],
        )?;

        ensure_category_changed(deleted)
    }

    pub fn reorder_category(&self, input: ReorderCategoryInput) -> Result<()> {
        if input.category_id == input.target_category_id {
            return Ok(());
        }

        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let active_profile_id = active_profile_id(&transaction)?;
        let category_parent_id: Option<i64> = transaction.query_row(
            "SELECT parent_id
             FROM categories
             WHERE id = ?1 AND profile_id = ?2",
            params![input.category_id, active_profile_id],
            |row| row.get(0),
        )?;
        let target_parent_id: Option<i64> = transaction.query_row(
            "SELECT parent_id
             FROM categories
             WHERE id = ?1 AND profile_id = ?2",
            params![input.target_category_id, active_profile_id],
            |row| row.get(0),
        )?;

        if category_parent_id != target_parent_id {
            return Err(rusqlite::Error::InvalidParameterName(
                "categories can only be reordered within the same parent".into(),
            ));
        }

        let mut category_ids = {
            let mut statement = transaction.prepare(
                "SELECT id
                 FROM categories
                 WHERE profile_id = ?1 AND parent_id IS ?2
                 ORDER BY sort_order, id",
            )?;
            statement
                .query_map(params![active_profile_id, category_parent_id], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<Result<Vec<_>>>()?
        };
        let category_index = category_ids
            .iter()
            .position(|id| *id == input.category_id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        category_ids.remove(category_index);
        let target_index = category_ids
            .iter()
            .position(|id| *id == input.target_category_id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let insertion_index = target_index + usize::from(input.place_after);
        category_ids.insert(insertion_index, input.category_id);

        for (sort_order, category_id) in category_ids.into_iter().enumerate() {
            transaction.execute(
                "UPDATE categories
                 SET sort_order = ?1, updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?2 AND profile_id = ?3",
                params![sort_order as i64, category_id, active_profile_id],
            )?;
        }

        transaction.commit()
    }

    pub fn create_phrase(&self, input: CreatePhraseInput) -> Result<()> {
        validate_phrase(&input.title, &input.snippet, &input.body)?;

        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        ensure_category_in_profile(&connection, active_profile_id, input.category_id)?;
        let snippet = input.snippet.trim();
        connection.execute(
            "INSERT INTO phrases (
                profile_id, category_id, title, snippet, snippet_key, body, description
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                active_profile_id,
                input.category_id,
                input.title.trim(),
                snippet,
                normalize_case_key(snippet),
                input.body.trim(),
                input.description.trim()
            ],
        )?;

        Ok(())
    }

    pub fn update_phrase(&self, input: UpdatePhraseInput) -> Result<()> {
        validate_phrase(&input.title, &input.snippet, &input.body)?;

        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        ensure_category_in_profile(&connection, active_profile_id, input.category_id)?;
        let snippet = input.snippet.trim();
        let updated = connection.execute(
            "UPDATE phrases
             SET category_id = ?1,
                 title = ?2,
                 snippet = ?3,
                 snippet_key = ?4,
                 body = ?5,
                 description = ?6,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?7 AND profile_id = ?8",
            params![
                input.category_id,
                input.title.trim(),
                snippet,
                normalize_case_key(snippet),
                input.body.trim(),
                input.description.trim(),
                input.id,
                active_profile_id
            ],
        )?;

        ensure_phrase_changed(updated)
    }

    pub fn set_phrase_enabled(&self, phrase_id: i64, enabled: bool) -> Result<()> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let updated = connection.execute(
            "UPDATE phrases
             SET is_enabled = ?1, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?2 AND profile_id = ?3",
            params![i64::from(enabled), phrase_id, active_profile_id],
        )?;

        ensure_phrase_changed(updated)
    }

    pub fn delete_phrase(&self, phrase_id: i64) -> Result<()> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let deleted = connection.execute(
            "DELETE FROM phrases WHERE id = ?1 AND profile_id = ?2",
            params![phrase_id, active_profile_id],
        )?;

        ensure_phrase_changed(deleted)
    }

    pub fn save_dictionary_word(&self, input: DictionaryWordInput) -> Result<()> {
        let word = validate_dictionary_word(&input.word)?;
        if !(0..=100).contains(&input.priority) {
            return Err(rusqlite::Error::InvalidParameterName(
                "dictionary word priority must be between 0 and 100".into(),
            ));
        }

        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        ensure_category_in_profile(&connection, active_profile_id, input.category_id)?;
        let word_key = normalize_case_key(word);
        match input.id {
            Some(word_id) => {
                let updated = connection.execute(
                    "UPDATE dictionary_words
                     SET category_id = ?1,
                         word = ?2,
                         word_key = ?3,
                         priority = ?4,
                         is_enabled = ?5,
                         autocomplete_enabled = ?6,
                         autocorrect_enabled = ?7,
                         updated_at = CURRENT_TIMESTAMP
                     WHERE id = ?8 AND profile_id = ?9",
                    params![
                        input.category_id,
                        word,
                        word_key,
                        input.priority,
                        i64::from(input.is_enabled),
                        i64::from(input.autocomplete_enabled),
                        i64::from(input.autocorrect_enabled),
                        word_id,
                        active_profile_id
                    ],
                )?;
                ensure_dictionary_word_changed(updated)
            }
            None => {
                connection.execute(
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
                        active_profile_id,
                        input.category_id,
                        word,
                        word_key,
                        input.priority,
                        i64::from(input.is_enabled),
                        i64::from(input.autocomplete_enabled),
                        i64::from(input.autocorrect_enabled)
                    ],
                )?;
                Ok(())
            }
        }
    }

    pub fn delete_dictionary_word(&self, word_id: i64) -> Result<()> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let deleted = connection.execute(
            "DELETE FROM dictionary_words WHERE id = ?1 AND profile_id = ?2",
            params![word_id, active_profile_id],
        )?;

        ensure_dictionary_word_changed(deleted)
    }

    pub fn active_snippets(&self) -> Result<Vec<(String, String)>> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let mut statement = connection.prepare(
            "SELECT snippet, body
             FROM phrases
             WHERE profile_id = ?1 AND is_enabled = 1
             ORDER BY snippet",
        )?;

        statement
            .query_map([active_profile_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    }

    pub fn quick_search_items(&self) -> Result<Vec<QuickSearchItem>> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let mut items = Vec::new();

        let mut statement = connection.prepare(
            "SELECT
                phrases.id,
                phrases.title,
                phrases.snippet,
                phrases.body,
                COALESCE(categories.path, '')
             FROM phrases
             LEFT JOIN categories ON categories.id = phrases.category_id
             WHERE phrases.profile_id = ?1 AND phrases.is_enabled = 1
             ORDER BY phrases.updated_at DESC, phrases.title COLLATE NOCASE",
        )?;

        items.extend(
            statement
                .query_map([active_profile_id], |row| {
                    Ok(QuickSearchItem {
                        id: row.get(0)?,
                        kind: "phrase".into(),
                        title: row.get(1)?,
                        shortcut: row.get(2)?,
                        body: row.get(3)?,
                        category: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?,
        );

        let mut statement = connection.prepare(
            "SELECT
                dictionary_words.id,
                dictionary_words.word,
                COALESCE(categories.path, '')
             FROM dictionary_words
             LEFT JOIN categories ON categories.id = dictionary_words.category_id
             WHERE dictionary_words.profile_id = ?1
               AND dictionary_words.is_enabled = 1
             ORDER BY dictionary_words.priority DESC, dictionary_words.word COLLATE NOCASE",
        )?;
        items.extend(
            statement
                .query_map([active_profile_id], |row| {
                    let word: String = row.get(1)?;
                    Ok(QuickSearchItem {
                        id: row.get(0)?,
                        kind: "word".into(),
                        title: word.clone(),
                        shortcut: "Слово".into(),
                        body: word,
                        category: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?,
        );

        Ok(items)
    }

    pub fn quick_search_item_text(&self, kind: &str, item_id: i64) -> Result<String> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        match kind {
            "phrase" => connection.query_row(
                "SELECT body
                 FROM phrases
                 WHERE id = ?1 AND profile_id = ?2 AND is_enabled = 1",
                params![item_id, active_profile_id],
                |row| row.get(0),
            ),
            "word" => connection.query_row(
                "SELECT word
                 FROM dictionary_words
                 WHERE id = ?1 AND profile_id = ?2 AND is_enabled = 1",
                params![item_id, active_profile_id],
                |row| row.get(0),
            ),
            _ => Err(rusqlite::Error::InvalidParameterName(
                "unknown quick search item kind".into(),
            )),
        }
    }

    pub fn active_autocomplete_words(&self) -> Result<Vec<AutocompleteEntry>> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let mut statement = connection.prepare(
            "SELECT word, priority
             FROM dictionary_words
             WHERE profile_id = ?1
               AND is_enabled = 1
               AND autocomplete_enabled = 1
             ORDER BY priority DESC, length(word), word COLLATE NOCASE",
        )?;

        statement
            .query_map([active_profile_id], |row| {
                Ok(AutocompleteEntry {
                    word: row.get(0)?,
                    priority: row.get(1)?,
                })
            })?
            .collect()
    }

    pub fn active_autocorrect_words(&self) -> Result<Vec<String>> {
        let connection = self.lock()?;
        let active_profile_id = active_profile_id(&connection)?;
        let mut statement = connection.prepare(
            "SELECT word
             FROM dictionary_words
             WHERE profile_id = ?1
               AND is_enabled = 1
               AND autocorrect_enabled = 1
             ORDER BY word COLLATE NOCASE",
        )?;

        statement
            .query_map([active_profile_id], |row| row.get(0))?
            .collect()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| rusqlite::Error::InvalidQuery)
    }
}

fn initialize_connection(connection: &mut Connection) -> Result<()> {
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;

    let transaction = connection.transaction()?;
    transaction.execute_batch(INITIAL_SCHEMA)?;
    migrate_phrases_to_plain_triggers(&transaction)?;
    migrate_dictionary_word_enabled(&transaction)?;
    migrate_normalized_keys(&transaction)?;
    migrate_category_sort_order(&transaction)?;
    transaction.execute_batch(INTEGRITY_TRIGGERS)?;
    transaction.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;

    let profile_count: u32 =
        transaction.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;
    if profile_count == 0 {
        transaction.execute(
            "INSERT INTO profiles (name, is_active) VALUES (?1, 1)",
            ["Основной"],
        )?;
    }

    transaction.commit()
}

fn active_profile_id(connection: &Connection) -> Result<i64> {
    connection.query_row(
        "SELECT id FROM profiles ORDER BY is_active DESC, id ASC LIMIT 1",
        [],
        |row| row.get(0),
    )
}

fn ensure_category_in_profile(
    connection: &Connection,
    profile_id: i64,
    category_id: Option<i64>,
) -> Result<()> {
    let Some(category_id) = category_id else {
        return Ok(());
    };
    let exists: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM categories WHERE id = ?1 AND profile_id = ?2
         )",
        params![category_id, profile_id],
        |row| row.get(0),
    )?;

    if exists {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

fn normalize_case_key(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase).collect()
}

fn validate_profile_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        Err(rusqlite::Error::InvalidParameterName(
            "profile name cannot be empty".into(),
        ))
    } else {
        Ok(name)
    }
}

fn ensure_profile_changed(changed_rows: usize) -> Result<()> {
    if changed_rows == 1 {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

fn validate_category_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') {
        Err(rusqlite::Error::InvalidParameterName(
            "category name cannot be empty or contain '/'".into(),
        ))
    } else {
        Ok(name)
    }
}

fn ensure_category_changed(changed_rows: usize) -> Result<()> {
    if changed_rows == 1 {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

fn migrate_category_sort_order(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    if table_has_column(transaction, "categories", "sort_order")? {
        return Ok(());
    }

    transaction.execute(
        "ALTER TABLE categories
         ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    )?;
    let categories = {
        let mut statement = transaction.prepare(
            "SELECT id, profile_id, parent_id
             FROM categories
             ORDER BY profile_id, path COLLATE NOCASE, id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>>>()?
    };
    let mut sibling_positions: HashMap<(i64, Option<i64>), i64> = HashMap::new();

    for (category_id, profile_id, parent_id) in categories {
        let sort_order = sibling_positions
            .entry((profile_id, parent_id))
            .or_default();
        transaction.execute(
            "UPDATE categories SET sort_order = ?1 WHERE id = ?2",
            params![*sort_order, category_id],
        )?;
        *sort_order += 1;
    }

    Ok(())
}

fn validate_phrase(title: &str, snippet: &str, body: &str) -> Result<()> {
    if title.trim().is_empty() || body.trim().is_empty() || !is_valid_trigger(snippet.trim()) {
        return Err(rusqlite::Error::InvalidParameterName(
            "title, body, and a plain alphanumeric snippet are required".into(),
        ));
    }

    Ok(())
}

fn ensure_phrase_changed(changed_rows: usize) -> Result<()> {
    if changed_rows == 1 {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

fn validate_dictionary_word(word: &str) -> Result<&str> {
    let word = word.trim();
    let valid = !word.is_empty()
        && word.chars().count() <= 64
        && word
            .chars()
            .all(|character| character.is_alphabetic() || character == '-');
    if valid {
        Ok(word)
    } else {
        Err(rusqlite::Error::InvalidParameterName(
            "dictionary word must contain only letters and hyphens".into(),
        ))
    }
}

fn ensure_dictionary_word_changed(changed_rows: usize) -> Result<()> {
    if changed_rows == 1 {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

fn migrate_dictionary_word_enabled(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    let has_column: bool = transaction.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM pragma_table_info('dictionary_words')
            WHERE name = 'is_enabled'
        )",
        [],
        |row| row.get(0),
    )?;

    if !has_column {
        transaction.execute(
            "ALTER TABLE dictionary_words
             ADD COLUMN is_enabled INTEGER NOT NULL DEFAULT 1
             CHECK (is_enabled IN (0, 1))",
            [],
        )?;
    }

    Ok(())
}

fn migrate_normalized_keys(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    migrate_phrase_keys(transaction)?;
    migrate_dictionary_word_keys(transaction)?;

    transaction.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_phrases_profile_snippet_key
         ON phrases (profile_id, snippet_key)
         WHERE snippet_key IS NOT NULL;

         CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_profile_word_key
         ON dictionary_words (profile_id, word_key)
         WHERE word_key IS NOT NULL;",
    )
}

fn migrate_phrase_keys(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    if table_has_column(transaction, "phrases", "snippet_key")? {
        return Ok(());
    }

    transaction.execute("ALTER TABLE phrases ADD COLUMN snippet_key TEXT", [])?;
    let rows = {
        let mut statement =
            transaction.prepare("SELECT id, profile_id, snippet FROM phrases ORDER BY id")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>>>()?
    };
    let mut seen = HashSet::new();

    for (id, profile_id, snippet) in rows {
        let key = normalize_case_key(snippet.trim());
        if seen.insert((profile_id, key.clone())) {
            transaction.execute(
                "UPDATE phrases SET snippet_key = ?1 WHERE id = ?2",
                params![key, id],
            )?;
        } else {
            transaction.execute(
                "UPDATE phrases SET snippet_key = NULL, is_enabled = 0 WHERE id = ?1",
                [id],
            )?;
            tracing::warn!(
                phrase_id = id,
                %snippet,
                "disabled case-insensitive duplicate snippet during migration"
            );
        }
    }

    Ok(())
}

fn migrate_dictionary_word_keys(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    if table_has_column(transaction, "dictionary_words", "word_key")? {
        return Ok(());
    }

    transaction.execute("ALTER TABLE dictionary_words ADD COLUMN word_key TEXT", [])?;
    let rows = {
        let mut statement =
            transaction.prepare("SELECT id, profile_id, word FROM dictionary_words ORDER BY id")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>>>()?
    };
    let mut seen = HashSet::new();

    for (id, profile_id, word) in rows {
        let key = normalize_case_key(word.trim());
        if seen.insert((profile_id, key.clone())) {
            transaction.execute(
                "UPDATE dictionary_words SET word_key = ?1 WHERE id = ?2",
                params![key, id],
            )?;
        } else {
            transaction.execute(
                "UPDATE dictionary_words SET word_key = NULL, is_enabled = 0 WHERE id = ?1",
                [id],
            )?;
            tracing::warn!(
                word_id = id,
                %word,
                "disabled case-insensitive duplicate word during migration"
            );
        }
    }

    Ok(())
}

fn table_has_column(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
) -> Result<bool> {
    transaction.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2
         )",
        params![table, column],
        |row| row.get(0),
    )
}

fn migrate_phrases_to_plain_triggers(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    let phrases_sql: String = transaction.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'phrases'",
        [],
        |row| row.get(0),
    )?;

    if !phrases_sql.contains("snippet LIKE '/%'") {
        return Ok(());
    }

    transaction.execute_batch(
        "CREATE TABLE phrases_v2 (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id INTEGER NOT NULL,
            category_id INTEGER,
            title TEXT NOT NULL,
            snippet TEXT NOT NULL CHECK (length(trim(snippet)) > 0),
            body TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE SET NULL,
            UNIQUE (profile_id, snippet)
        );

        INSERT INTO phrases_v2 (
            id, profile_id, category_id, title, snippet, body, description,
            is_enabled, created_at, updated_at
        )
        SELECT
            id,
            profile_id,
            category_id,
            title,
            CASE WHEN snippet LIKE '/%' THEN substr(snippet, 2) ELSE snippet END,
            body,
            description,
            is_enabled,
            created_at,
            updated_at
        FROM phrases;

        DROP TABLE phrases;
        ALTER TABLE phrases_v2 RENAME TO phrases;",
    )
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, Result, params};

    use super::{
        CreateCategoryInput, CreatePhraseInput, Database, DictionaryWordInput,
        ReorderCategoryInput, migrate_category_sort_order, migrate_normalized_keys,
        migrate_phrases_to_plain_triggers,
    };

    #[test]
    fn creates_default_profile() {
        let database = Database::open(":memory:").expect("database should open");

        assert_eq!(database.profile_count().unwrap(), 1);
    }

    #[test]
    fn creates_category_and_phrase() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_category(CreateCategoryInput {
                name: "Продажи".into(),
                parent_id: None,
            })
            .unwrap();

        let category_id = database.dashboard_data().unwrap().categories[0].id;
        database
            .create_phrase(CreatePhraseInput {
                title: "Коммерческое предложение".into(),
                snippet: "кп".into(),
                body: "Здравствуйте! Отправляю коммерческое предложение.".into(),
                description: String::new(),
                category_id: Some(category_id),
            })
            .unwrap();

        let dashboard = database.dashboard_data().unwrap();
        assert_eq!(dashboard.phrases.len(), 1);
        assert_eq!(dashboard.categories[0].phrase_count, 1);
    }

    #[test]
    fn reorders_sibling_categories_with_their_subtrees() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_category(CreateCategoryInput {
                name: "Alpha".into(),
                parent_id: None,
            })
            .unwrap();
        let alpha_id = database.dashboard_data().unwrap().categories[0].id;
        database
            .create_category(CreateCategoryInput {
                name: "Alpha child".into(),
                parent_id: Some(alpha_id),
            })
            .unwrap();
        database
            .create_category(CreateCategoryInput {
                name: "Beta".into(),
                parent_id: None,
            })
            .unwrap();
        database
            .create_category(CreateCategoryInput {
                name: "Gamma".into(),
                parent_id: None,
            })
            .unwrap();

        let dashboard = database.dashboard_data().unwrap();
        let gamma_id = dashboard
            .categories
            .iter()
            .find(|category| category.name == "Gamma")
            .unwrap()
            .id;
        database
            .reorder_category(ReorderCategoryInput {
                category_id: gamma_id,
                target_category_id: alpha_id,
                place_after: false,
            })
            .unwrap();

        let names = database
            .dashboard_data()
            .unwrap()
            .categories
            .into_iter()
            .map(|category| category.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Gamma", "Alpha", "Alpha child", "Beta"]);
    }

    #[test]
    fn rejects_reordering_categories_from_different_levels() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_category(CreateCategoryInput {
                name: "Root".into(),
                parent_id: None,
            })
            .unwrap();
        let root_id = database.dashboard_data().unwrap().categories[0].id;
        database
            .create_category(CreateCategoryInput {
                name: "Child".into(),
                parent_id: Some(root_id),
            })
            .unwrap();
        let child_id = database
            .dashboard_data()
            .unwrap()
            .categories
            .into_iter()
            .find(|category| category.name == "Child")
            .unwrap()
            .id;

        let result = database.reorder_category(ReorderCategoryInput {
            category_id: child_id,
            target_category_id: root_id,
            place_after: false,
        });

        assert!(result.is_err());
    }

    #[test]
    fn category_sort_order_migration_preserves_alphabetical_sibling_order() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(
                "CREATE TABLE categories (
                    id INTEGER PRIMARY KEY,
                    profile_id INTEGER NOT NULL,
                    parent_id INTEGER,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL
                );
                INSERT INTO categories (id, profile_id, parent_id, name, path) VALUES
                    (1, 1, NULL, 'Zulu', 'Zulu'),
                    (2, 1, NULL, 'Alpha', 'Alpha'),
                    (3, 1, 2, 'Zulu child', 'Alpha/Zulu child'),
                    (4, 1, 2, 'Alpha child', 'Alpha/Alpha child');",
            )
            .expect("legacy categories should be created");

        let transaction = connection.transaction().expect("transaction should start");
        migrate_category_sort_order(&transaction).expect("migration should succeed");
        transaction.commit().expect("migration should commit");

        let positions = {
            let mut statement = connection
                .prepare("SELECT id, sort_order FROM categories ORDER BY id")
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(positions, [(1, 1), (2, 0), (3, 1), (4, 0)]);
    }

    #[test]
    fn rejects_slash_prefixed_snippet() {
        let database = Database::open(":memory:").expect("database should open");
        let result = database.create_phrase(CreatePhraseInput {
            title: "Фраза".into(),
            snippet: "/кп".into(),
            body: "Текст".into(),
            description: String::new(),
            category_id: None,
        });

        assert!(result.is_err());
    }

    #[test]
    fn rejects_case_insensitive_cyrillic_duplicates() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_phrase(CreatePhraseInput {
                title: "Первая".into(),
                snippet: "КП".into(),
                body: "Первый текст".into(),
                description: String::new(),
                category_id: None,
            })
            .unwrap();
        let duplicate_phrase = database.create_phrase(CreatePhraseInput {
            title: "Вторая".into(),
            snippet: "кп".into(),
            body: "Второй текст".into(),
            description: String::new(),
            category_id: None,
        });

        database
            .save_dictionary_word(DictionaryWordInput {
                id: None,
                word: "Согласование".into(),
                category_id: None,
                priority: 0,
                is_enabled: true,
                autocomplete_enabled: true,
                autocorrect_enabled: true,
            })
            .unwrap();
        let duplicate_word = database.save_dictionary_word(DictionaryWordInput {
            id: None,
            word: "согласование".into(),
            category_id: None,
            priority: 0,
            is_enabled: true,
            autocomplete_enabled: true,
            autocorrect_enabled: true,
        });

        assert!(duplicate_phrase.is_err());
        assert!(duplicate_word.is_err());
    }

    #[test]
    fn rejects_categories_from_another_profile() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_category(CreateCategoryInput {
                name: "Первый профиль".into(),
                parent_id: None,
            })
            .unwrap();
        let category_id = database.dashboard_data().unwrap().categories[0].id;
        database.create_profile("Второй").unwrap();

        let phrase_result = database.create_phrase(CreatePhraseInput {
            title: "Чужая категория".into(),
            snippet: "чужая".into(),
            body: "Текст".into(),
            description: String::new(),
            category_id: Some(category_id),
        });
        let word_result = database.save_dictionary_word(DictionaryWordInput {
            id: None,
            word: "Категория".into(),
            category_id: Some(category_id),
            priority: 0,
            is_enabled: true,
            autocomplete_enabled: true,
            autocorrect_enabled: true,
        });

        assert!(phrase_result.is_err());
        assert!(word_result.is_err());
    }

    #[test]
    fn database_triggers_reject_cross_profile_category_links() {
        let database = Database::open(":memory:").expect("database should open");
        database
            .create_category(CreateCategoryInput {
                name: "Первый профиль".into(),
                parent_id: None,
            })
            .unwrap();
        let first_dashboard = database.dashboard_data().unwrap();
        let first_profile_id = first_dashboard.active_profile_id;
        let category_id = first_dashboard.categories[0].id;
        database.create_profile("Второй").unwrap();
        let second_profile_id = database.dashboard_data().unwrap().active_profile_id;
        let connection = database.lock().unwrap();

        let result = connection.execute(
            "INSERT INTO phrases (
                profile_id, category_id, title, snippet, snippet_key, body
             ) VALUES (?1, ?2, 'Тест', 'тест', 'тест', 'Текст')",
            params![second_profile_id, category_id],
        );

        assert_ne!(first_profile_id, second_profile_id);
        assert!(result.is_err());
    }

    #[test]
    fn normalized_key_migration_disables_case_insensitive_duplicates() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(
                "CREATE TABLE phrases (
                    id INTEGER PRIMARY KEY,
                    profile_id INTEGER NOT NULL,
                    snippet TEXT NOT NULL,
                    is_enabled INTEGER NOT NULL DEFAULT 1
                );
                CREATE TABLE dictionary_words (
                    id INTEGER PRIMARY KEY,
                    profile_id INTEGER NOT NULL,
                    word TEXT NOT NULL,
                    is_enabled INTEGER NOT NULL DEFAULT 1
                );
                INSERT INTO phrases (id, profile_id, snippet) VALUES
                    (1, 1, 'КП'),
                    (2, 1, 'кп');
                INSERT INTO dictionary_words (id, profile_id, word) VALUES
                    (1, 1, 'Согласование'),
                    (2, 1, 'согласование');",
            )
            .expect("legacy data should be created");

        let transaction = connection.transaction().expect("transaction should start");
        migrate_normalized_keys(&transaction).expect("migration should succeed");
        transaction.commit().expect("migration should commit");

        let disabled_phrases: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM phrases
                 WHERE snippet_key IS NULL AND is_enabled = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let disabled_words: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM dictionary_words
                 WHERE word_key IS NULL AND is_enabled = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(disabled_phrases, 1);
        assert_eq!(disabled_words, 1);
    }

    #[test]
    fn migrates_slash_prefixed_snippets() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;

                CREATE TABLE profiles (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    is_active INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE categories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile_id INTEGER NOT NULL,
                    parent_id INTEGER,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL
                );

                CREATE TABLE phrases (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile_id INTEGER NOT NULL,
                    category_id INTEGER,
                    title TEXT NOT NULL,
                    snippet TEXT NOT NULL CHECK (snippet LIKE '/%'),
                    body TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    is_enabled INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                    FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE SET NULL,
                    UNIQUE (profile_id, snippet)
                );

                INSERT INTO profiles (id, name, is_active)
                VALUES (1, 'Основной', 1);

                INSERT INTO phrases (profile_id, title, snippet, body)
                VALUES (1, 'Приветствие', '/з', 'Здравствуйте!');",
            )
            .expect("legacy schema should be created");

        let transaction = connection.transaction().expect("transaction should start");
        migrate_phrases_to_plain_triggers(&transaction).expect("migration should succeed");
        transaction.commit().expect("migration should commit");

        let snippet: String = connection
            .query_row("SELECT snippet FROM phrases", [], |row| row.get(0))
            .expect("migrated snippet should exist");
        let body: String = connection
            .query_row("SELECT body FROM phrases", [], |row| row.get(0))
            .expect("migrated body should exist");

        assert_eq!(snippet, "з");
        assert_eq!(body, "Здравствуйте!");
    }
}
