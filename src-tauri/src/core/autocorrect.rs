#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub original: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Default)]
pub struct AutocorrectIndex {
    words: Vec<String>,
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
        self.words = words;
    }

    pub fn correction_for(&self, word: &str) -> Option<Correction> {
        let normalized = word.to_lowercase();
        let length = normalized.chars().count();
        if length < 5 || !is_russian_word(&normalized) {
            return None;
        }

        if self
            .words
            .iter()
            .any(|candidate| candidate.to_lowercase() == normalized)
        {
            return None;
        }

        let max_distance = if length <= 7 { 1 } else { 2 };
        let mut nearest: Option<(&str, usize)> = None;
        let mut ambiguous = false;

        for candidate in &self.words {
            let candidate_normalized = candidate.to_lowercase();
            if candidate_normalized.chars().count().abs_diff(length) > max_distance {
                continue;
            }

            let distance = damerau_levenshtein(&normalized, &candidate_normalized);
            if distance > max_distance {
                continue;
            }

            match nearest {
                None => {
                    nearest = Some((candidate, distance));
                    ambiguous = false;
                }
                Some((_, nearest_distance)) if distance < nearest_distance => {
                    nearest = Some((candidate, distance));
                    ambiguous = false;
                }
                Some((_, nearest_distance)) if distance == nearest_distance => {
                    ambiguous = true;
                }
                _ => {}
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

fn damerau_levenshtein(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let columns = right.len() + 1;
    let mut distances = vec![0usize; (left.len() + 1) * columns];

    for row in 0..=left.len() {
        distances[row * columns] = row;
    }
    for column in 0..=right.len() {
        distances[column] = column;
    }

    for row in 1..=left.len() {
        for column in 1..=right.len() {
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

            distances[row * columns + column] = distance;
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
}
