//! Translation: the language comes from the system (LANGUAGE, LC_ALL, LC_MESSAGES, LANG); English otherwise.

use std::sync::OnceLock;

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
];

fn language() -> usize {
    static LANG: OnceLock<usize> = OnceLock::new();
    *LANG.get_or_init(|| {
        let wanted = ["TETHER_LANG", "LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter()
            .filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty()).unwrap_or_default();
        match wanted.get(..2) { Some("it") => 1, Some("ru") => 2, _ => 0 }
    })
}

pub fn t(english: &str) -> String {
    let lang = language();
    if lang == 0 {
        return english.to_string();
    }
    MESSAGES.iter().find(|(en, _, _)| *en == english).map(|(_, it, ru)| if lang == 1 { *it } else { *ru })
        .unwrap_or(english).to_string()
}

pub fn tf(english: &str, values: &[(&str, &str)]) -> String {
    let mut text = t(english);
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

