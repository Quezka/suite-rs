//! translation: table @ `messages.rs`, rules @ `quezka_kit::i18n`; lang = `TIDINGS_LANG` | system
use super::messages::MESSAGES;

quezka_kit::translations!(MESSAGES);

#[cfg(test)]
mod tests {
    #[test]
    fn rows_are_complete() {
        quezka_kit::i18n::check_messages(super::MESSAGES).unwrap();
    }
}
