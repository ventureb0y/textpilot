const MAX_TRIGGER_LENGTH: usize = 64;

#[derive(Debug, Default)]
pub struct SnippetMatcher {
    buffer: String,
}

impl SnippetMatcher {
    pub fn typed(&mut self, character: char) {
        if is_trigger_character(character) {
            self.buffer.push(character);
            if self.buffer.chars().count() > MAX_TRIGGER_LENGTH {
                self.buffer.clear();
            }
        } else {
            self.buffer.clear();
        }
    }

    pub fn backspace(&mut self) {
        self.buffer.pop();
    }

    pub fn current(&self) -> &str {
        &self.buffer
    }

    pub fn take_trigger(&mut self) -> Option<String> {
        let trigger = std::mem::take(&mut self.buffer);
        (!trigger.is_empty()).then_some(trigger)
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

pub fn is_valid_trigger(trigger: &str) -> bool {
    !trigger.is_empty()
        && trigger.chars().count() <= MAX_TRIGGER_LENGTH
        && trigger.chars().all(is_trigger_character)
}

fn is_trigger_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::{SnippetMatcher, is_valid_trigger};

    #[test]
    fn captures_plain_cyrillic_trigger() {
        let mut matcher = SnippetMatcher::default();
        for character in "кп".chars() {
            matcher.typed(character);
        }

        assert_eq!(matcher.take_trigger().as_deref(), Some("кп"));
    }

    #[test]
    fn tracks_the_current_word() {
        let mut matcher = SnippetMatcher::default();
        for character in "текст сроки".chars() {
            matcher.typed(character);
        }

        assert_eq!(matcher.take_trigger().as_deref(), Some("сроки"));
    }

    #[test]
    fn backspace_updates_the_trigger() {
        let mut matcher = SnippetMatcher::default();
        for character in "кпи".chars() {
            matcher.typed(character);
        }
        matcher.backspace();

        assert_eq!(matcher.take_trigger().as_deref(), Some("кп"));
    }

    #[test]
    fn punctuation_starts_a_new_boundary() {
        let mut matcher = SnippetMatcher::default();
        for character in "текст.кп".chars() {
            matcher.typed(character);
        }

        assert_eq!(matcher.take_trigger().as_deref(), Some("кп"));
    }

    #[test]
    fn validates_plain_triggers() {
        assert!(is_valid_trigger("кп"));
        assert!(is_valid_trigger("сроки_2"));
        assert!(!is_valid_trigger(""));
        assert!(!is_valid_trigger("/кп"));
        assert!(!is_valid_trigger("два слова"));
    }
}
