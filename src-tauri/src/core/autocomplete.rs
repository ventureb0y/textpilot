use std::collections::HashMap;

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
struct TrieNode {
    children: HashMap<char, usize>,
    candidate_ids: Vec<usize>,
}

#[derive(Debug, Clone)]
struct IndexedEntry {
    word: String,
}

#[derive(Debug, Clone)]
pub struct AutocompleteIndex {
    entries: Vec<IndexedEntry>,
    nodes: Vec<TrieNode>,
}

impl Default for AutocompleteIndex {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            nodes: vec![TrieNode::default()],
        }
    }
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
        let mut nodes = vec![TrieNode::default()];
        let entries = entries
            .into_iter()
            .map(|entry| IndexedEntry { word: entry.word })
            .collect::<Vec<_>>();

        for (entry_id, entry) in entries.iter().enumerate() {
            let normalized = entry.word.to_lowercase();
            let characters = normalized.chars().collect::<Vec<_>>();
            let mut node_id = 0;

            for (offset, character) in characters.iter().copied().enumerate() {
                let next_id = nodes[node_id]
                    .children
                    .get(&character)
                    .copied()
                    .unwrap_or_else(|| {
                        let next_id = nodes.len();
                        nodes.push(TrieNode::default());
                        nodes[node_id].children.insert(character, next_id);
                        next_id
                    });
                node_id = next_id;

                let prefix_length = offset + 1;
                if prefix_length >= 2 && prefix_length < characters.len() {
                    nodes[node_id].candidate_ids.push(entry_id);
                }
            }
        }

        self.entries = entries;
        self.nodes = nodes;
    }

    pub fn completions_for(&self, prefix: &str, limit: usize) -> Vec<Completion> {
        if prefix.chars().count() < 2 || !prefix.chars().all(is_word_character) {
            return Vec::new();
        }

        let prefix_length = prefix.chars().count();
        let mut node_id = 0;
        for character in prefix.to_lowercase().chars() {
            let Some(next_id) = self.nodes[node_id].children.get(&character).copied() else {
                return Vec::new();
            };
            node_id = next_id;
        }

        self.nodes[node_id]
            .candidate_ids
            .iter()
            .take(limit)
            .map(|entry_id| {
                let word = &self.entries[*entry_id].word;
                Completion {
                    word: word.clone(),
                    suffix: word.chars().skip(prefix_length).collect(),
                }
            })
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

    #[test]
    fn trie_lookup_is_case_insensitive_and_respects_limit() {
        let index = AutocompleteIndex::new(vec![
            AutocompleteEntry {
                word: "Команда".into(),
                priority: 10,
            },
            AutocompleteEntry {
                word: "командир".into(),
                priority: 5,
            },
            AutocompleteEntry {
                word: "командировка".into(),
                priority: 1,
            },
        ]);

        let completions = index.completions_for("КОМ", 2);

        assert_eq!(completions.len(), 2);
        assert_eq!(completions[0].word, "Команда");
        assert_eq!(completions[0].suffix, "анда");
        assert_eq!(completions[1].word, "командир");
    }
}
