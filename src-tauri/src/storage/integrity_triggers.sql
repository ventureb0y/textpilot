CREATE TRIGGER IF NOT EXISTS categories_parent_profile_insert
BEFORE INSERT ON categories
WHEN NEW.parent_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.parent_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'category parent belongs to another profile');
END;

CREATE TRIGGER IF NOT EXISTS categories_parent_profile_update
BEFORE UPDATE OF parent_id, profile_id ON categories
WHEN NEW.parent_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.parent_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'category parent belongs to another profile');
END;

CREATE TRIGGER IF NOT EXISTS phrases_category_profile_insert
BEFORE INSERT ON phrases
WHEN NEW.category_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.category_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'phrase category belongs to another profile');
END;

CREATE TRIGGER IF NOT EXISTS phrases_category_profile_update
BEFORE UPDATE OF category_id, profile_id ON phrases
WHEN NEW.category_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.category_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'phrase category belongs to another profile');
END;

CREATE TRIGGER IF NOT EXISTS phrases_enabled_key_insert
BEFORE INSERT ON phrases
WHEN NEW.is_enabled = 1 AND NEW.snippet_key IS NULL
BEGIN
    SELECT RAISE(ABORT, 'enabled phrase requires a normalized snippet');
END;

CREATE TRIGGER IF NOT EXISTS phrases_enabled_key_update
BEFORE UPDATE OF is_enabled, snippet_key ON phrases
WHEN NEW.is_enabled = 1 AND NEW.snippet_key IS NULL
BEGIN
    SELECT RAISE(ABORT, 'enabled phrase requires a normalized snippet');
END;

CREATE TRIGGER IF NOT EXISTS dictionary_category_profile_insert
BEFORE INSERT ON dictionary_words
WHEN NEW.category_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.category_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'dictionary category belongs to another profile');
END;

CREATE TRIGGER IF NOT EXISTS dictionary_enabled_key_insert
BEFORE INSERT ON dictionary_words
WHEN NEW.is_enabled = 1 AND NEW.word_key IS NULL
BEGIN
    SELECT RAISE(ABORT, 'enabled dictionary word requires a normalized key');
END;

CREATE TRIGGER IF NOT EXISTS dictionary_enabled_key_update
BEFORE UPDATE OF is_enabled, word_key ON dictionary_words
WHEN NEW.is_enabled = 1 AND NEW.word_key IS NULL
BEGIN
    SELECT RAISE(ABORT, 'enabled dictionary word requires a normalized key');
END;

CREATE TRIGGER IF NOT EXISTS dictionary_category_profile_update
BEFORE UPDATE OF category_id, profile_id ON dictionary_words
WHEN NEW.category_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM categories
    WHERE id = NEW.category_id AND profile_id = NEW.profile_id
 )
BEGIN
    SELECT RAISE(ABORT, 'dictionary category belongs to another profile');
END;
