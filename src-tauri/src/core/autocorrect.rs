use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub original: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Default)]
pub struct AutocorrectIndex {
    exact_words: HashSet<String>,
    words_by_length: HashMap<usize, Vec<IndexedWord>>,
}

#[derive(Debug, Clone)]
struct IndexedWord {
    word: String,
    characters: Vec<char>,
}

impl AutocorrectIndex {
    pub fn new(words: Vec<String>) -> Self {
        let mut index = Self::default();
        index.replace(words);
        index
    }

    pub fn replace(&mut self, mut words: Vec<String>) {
        words.sort_by_key(|word| word.to_lowercase());
        words.dedup_by(|left, right| left.to_lowercase() == right.to_lowercase());

        let mut exact_words = HashSet::with_capacity(words.len());
        let mut words_by_length = HashMap::<usize, Vec<IndexedWord>>::new();
        for word in words {
            let normalized = word.to_lowercase();
            let characters = normalized.chars().collect::<Vec<_>>();
            exact_words.insert(normalized);
            words_by_length
                .entry(characters.len())
                .or_default()
                .push(IndexedWord { word, characters });
        }

        self.exact_words = exact_words;
        self.words_by_length = words_by_length;
    }

    pub fn correction_for(&self, word: &str) -> Option<Correction> {
        let normalized = word.to_lowercase();
        let length = normalized.chars().count();
        if length < 5 || !is_russian_word(&normalized) {
            return None;
        }

        if self.exact_words.contains(&normalized) {
            return None;
        }

        let max_distance = if length <= 7 { 1 } else { 2 };
        let mut nearest: Option<(&str, usize)> = None;
        let mut ambiguous = false;
        let characters = normalized.chars().collect::<Vec<_>>();
        let largest_candidate_length = length.saturating_add(max_distance);
        let columns = largest_candidate_length + 1;
        let mut distances = vec![0usize; (length + 1) * columns];

        for candidate_length in length.saturating_sub(max_distance)..=largest_candidate_length {
            let Some(candidates) = self.words_by_length.get(&candidate_length) else {
                continue;
            };

            for candidate in candidates {
                let distance = bounded_damerau_levenshtein(
                    &characters,
                    &candidate.characters,
                    max_distance,
                    columns,
                    &mut distances,
                );
                if distance > max_distance {
                    continue;
                }

                match nearest {
                    None => {
                        nearest = Some((&candidate.word, distance));
                        ambiguous = false;
                    }
                    Some((_, nearest_distance)) if distance < nearest_distance => {
                        nearest = Some((&candidate.word, distance));
                        ambiguous = false;
                    }
                    Some((_, nearest_distance)) if distance == nearest_distance => {
                        ambiguous = true;
                    }
                    _ => {}
                }
            }
        }

        let (candidate, _) = nearest?;
        if ambiguous {
            return None;
        }

        Some(Correction {
            original: word.to_owned(),
            replacement: match_case(word, candidate),
        })
    }
}

fn is_russian_word(word: &str) -> bool {
    let mut has_letter = false;
    word.chars().all(|character| {
        if character == '-' {
            return true;
        }

        let is_cyrillic = matches!(character, '\u{0400}'..='\u{04ff}');
        has_letter |= is_cyrillic;
        is_cyrillic
    }) && has_letter
}

fn match_case(original: &str, replacement: &str) -> String {
    let letters = original
        .chars()
        .filter(|character| character.is_alphabetic());
    if letters.clone().all(|character| character.is_uppercase()) {
        return replacement.to_uppercase();
    }

    if letters
        .take(1)
        .next()
        .is_some_and(|character| character.is_uppercase())
    {
        let mut characters = replacement.chars();
        if let Some(first) = characters.next() {
            return first.to_uppercase().chain(characters).collect::<String>();
        }
    }

    replacement.to_owned()
}

fn bounded_damerau_levenshtein(
    left: &[char],
    right: &[char],
    max_distance: usize,
    columns: usize,
    distances: &mut [usize],
) -> usize {
    let unreachable = max_distance + 1;
    distances.fill(unreachable);
    distances[0] = 0;

    for row in 1..=left.len().min(max_distance) {
        distances[row * columns] = row;
    }
    for column in 1..=right.len().min(max_distance) {
        distances[column] = column;
    }

    for row in 1..=left.len() {
        let first_column = row.saturating_sub(max_distance).max(1);
        let last_column = right.len().min(row + max_distance);
        for column in first_column..=last_column {
            let substitution_cost = usize::from(left[row - 1] != right[column - 1]);
            let mut distance = (distances[(row - 1) * columns + column] + 1)
                .min(distances[row * columns + column - 1] + 1)
                .min(distances[(row - 1) * columns + column - 1] + substitution_cost);

            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                distance = distance.min(distances[(row - 2) * columns + column - 2] + 1);
            }

            distances[row * columns + column] = distance.min(unreachable);
        }
    }

    distances[left.len() * columns + right.len()]
}

#[cfg(test)]
mod tests {
    use super::AutocorrectIndex;

    #[test]
    fn corrects_typo_and_transposition() {
        let index = AutocorrectIndex::new(vec!["согласование".into(), "макет".into()]);

        assert_eq!(
            index
                .correction_for("саглосование")
                .map(|correction| correction.replacement),
            Some("согласование".into())
        );
        assert_eq!(
            index
                .correction_for("макте")
                .map(|correction| correction.replacement),
            Some("макет".into())
        );
    }

    #[test]
    fn avoids_short_exact_and_ambiguous_words() {
        let index =
            AutocorrectIndex::new(vec!["котик".into(), "ротик".into(), "согласование".into()]);

        assert!(index.correction_for("кот").is_none());
        assert!(index.correction_for("согласование").is_none());
        assert!(index.correction_for("мотик").is_none());
    }

    #[test]
    fn corrects_insertion_deletion_and_two_long_word_errors() {
        let index = AutocorrectIndex::new(vec!["привет".into(), "производительность".into()]);

        assert_eq!(
            index
                .correction_for("привт")
                .map(|correction| correction.replacement),
            Some("привет".into())
        );
        assert_eq!(
            index
                .correction_for("привеет")
                .map(|correction| correction.replacement),
            Some("привет".into())
        );
        assert_eq!(
            index
                .correction_for("прозводительност")
                .map(|correction| correction.replacement),
            Some("производительность".into())
        );
    }
}
