//! translation: table here, rules @ `quezka_kit::i18n`; lang = `TETHER_LANG` | system

const MESSAGES: &[(&str, &str, &str)] = &[
    ("Your phone on this computer", "Il telefono su questo computer", "Телефон на этом компьютере"),
    ("Phone", "Telefono", "Телефон"),
    ("Notifications", "Notifiche", "Уведомления"),
    ("Messages", "Messaggi", "Сообщения"),
    ("Files", "File", "Файлы"),
    ("Connected to {name}", "Collegato a {name}", "Подключено к {name}"),
    ("No phone connected. Open Tether on the phone, on the same Wi-Fi.", "Nessun telefono collegato. Apri Tether sul telefono, sulla stessa Wi-Fi.", "Телефон не подключён. Открой Tether на телефоне в той же сети Wi-Fi."),
    ("Pair a phone", "Abbina un telefono", "Связать телефон"),
    ("On the phone, pick this computer and type this code:", "Sul telefono scegli questo computer e scrivi questo codice:", "На телефоне выбери этот компьютер и введи код:"),
    ("Paired phones", "Telefoni abbinati", "Связанные телефоны"),
    ("Forget", "Dimentica", "Забыть"),
    ("Battery {n}%", "Batteria {n}%", "Батарея {n}%"),
    ("{name} is calling", "{name} sta chiamando", "{name} звонит"),
    ("Answer", "Rispondi", "Ответить"),
    ("Reject", "Rifiuta", "Отклонить"),
    ("On a call with {name}", "In chiamata con {name}", "Разговор с {name}"),
    ("Hang up", "Riaggancia", "Положить трубку"),
    ("Nothing new on the phone.", "Niente di nuovo sul telefono.", "На телефоне ничего нового."),
    ("Reply…", "Rispondi…", "Ответить…"),
    ("Send", "Invia", "Отправить"),
    ("Dismiss", "Chiudi", "Скрыть"),
    ("Load messages", "Carica i messaggi", "Загрузить сообщения"),
    ("Write a message…", "Scrivi un messaggio…", "Напиши сообщение…"),
    ("Send a file to the phone", "Invia un file al telefono", "Отправить файл на телефон"),
    ("Send my clipboard", "Invia i miei appunti", "Отправить буфер обмена"),
    ("From the phone's clipboard:", "Dagli appunti del telefono:", "Из буфера телефона:"),
    ("Copy", "Copia", "Копировать"),
    ("Saved to {path}", "Salvato in {path}", "Сохранено в {path}"),
    ("Files the phone shares with Tether land in your Downloads folder.", "I file che il telefono condivide con Tether finiscono nella cartella Scaricati.", "Файлы, которыми телефон делится через Tether, попадают в папку «Загрузки»."),
    ("The phone isn't connected.", "Il telefono non è collegato.", "Телефон не подключён."),
    ("Connected", "Collegato", "Подключено"),
    ("charging", "in carica", "заряжается"),
    ("Waiting for the phone…", "In attesa del telefono…", "Ждём телефон…"),
    ("Your phone", "Il tuo telefono", "Твой телефон"),
    ("Quit Tether", "Esci da Tether", "Выйти из Tether"),
    ("Start with the computer", "Avvia con il computer", "Запускать вместе с компьютером"),
    ("Closing the window keeps Tether running.", "Chiudendo la finestra Tether resta attivo.", "Если закрыть окно, Tether продолжит работать."),
    ("OK", "OK", "ОК"),
    ("Sent {name} to the phone", "{name} inviato al telefono", "{name} отправлен на телефон"),
    ("Pick a conversation.", "Scegli una conversazione.", "Выбери переписку."),
    ("Write a message… (Enter sends)", "Scrivi un messaggio… (Invio per inviare)", "Напиши сообщение… (Enter — отправить)"),
    ("All conversations", "Tutte le conversazioni", "Все переписки"),
    ("Or drop files anywhere on this window.", "Oppure trascina i file in questa finestra.", "Или перетащи файлы в это окно."),
    ("Clipboard", "Appunti", "Буфер обмена"),
    ("Couldn't change starting with the computer.", "Impossibile cambiare l'avvio con il computer.", "Не удалось изменить автозапуск."),
];

quezka_kit::translations!(MESSAGES);

#[cfg(test)]
mod tests {
    #[test]
    fn rows_are_complete() {
        quezka_kit::i18n::check_messages(super::MESSAGES).unwrap();
    }
}
