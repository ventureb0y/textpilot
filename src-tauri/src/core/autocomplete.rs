#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteEntry {
    pub word: String,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub word: String,
    pub suffix: String,
}

#[derive(Debug, Clone, Default)]
pub struct AutocompleteIndex {
    entries: Vec<AutocompleteEntry>,
}

impl AutocompleteIndex {
    pub fn new(entries: Vec<AutocompleteEntry>) -> Self {
        let mut index = Self::default();
        index.replace(entries);
        index
    }

    pub fn replace(&mut self, mut entries: Vec<AutocompleteEntry>) {
        entries.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.word.chars().count().cmp(&right.word.chars().count()))
                .then_with(|| left.word.to_lowercase().cmp(&right.word.to_lowercase()))
        });
        self.entries = entries;
    }

    pub fn completions_for(&self, prefix: &str, limit: usize) -> Vec<Completion> {
        if prefix.chars().count() < 2 || !prefix.chars().all(is_word_character) {
            return Vec::new();
        }

        let normalized_prefix = prefix.to_lowercase();
        self.entries
            .iter()
            .filter_map(|entry| {
                let normalized_word = entry.word.to_lowercase();
                if normalized_word == normalized_prefix
                    || !normalized_word.starts_with(&normalized_prefix)
                {
                    return None;
                }

                let suffix = entry
                    .word
                    .chars()
                    .skip(prefix.chars().count())
                    .collect::<String>();
                Some(Completion {
                    word: entry.word.clone(),
                    suffix,
                })
            })
            .take(limit)
            .collect()
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphabetic() || character == '-'
}

#[cfg(test)]
mod tests {
    use super::{AutocompleteEntry, AutocompleteIndex};

    #[test]
    fn prefers_priority_then_shorter_word() {
        let index = AutocompleteIndex::new(vec![
            AutocompleteEntry {
                word: "коммерческий".into(),
                priority: 5,
            },
            AutocompleteEntry {
                word: "коммерческое".into(),
                priority: 10,
            },
        ]);

        assert_eq!(
            index
                .completions_for("комм", 6)
                .first()
                .map(|item| item.word.as_str()),
            Some("коммерческое")
        );
    }

    #[test]
    fn ignores_short_and_exact_prefixes() {
        let index = AutocompleteIndex::new(vec![AutocompleteEntry {
            word: "макет".into(),
            priority: 0,
        }]);

        assert!(index.completions_for("м", 6).is_empty());
        assert!(index.completions_for("макет", 6).is_empty());
    }
}
