//! Translation: the language comes from the system (LANGUAGE, LC_ALL, LC_MESSAGES, LANG); English otherwise.

use std::sync::OnceLock;

const MESSAGES: &[(&str, &str, &str)] = &[
    ("Camera and second screen over your home network", "Fotocamera e secondo schermo sulla rete di casa", "Камера и второй экран по домашней сети"),
    ("Send this camera", "Invia questa fotocamera", "Отправить эту камеру"),
    ("This computer's camera shows up on the other one as a normal webcam.", "La fotocamera di questo computer compare sull'altro come una webcam normale.", "Камера этого компьютера появится на другом как обычная веб-камера."),
    ("This computer has no camera.", "Questo computer non ha una fotocamera.", "У этого компьютера нет камеры."),
    ("No other computer with Loupe found yet. Open Loupe on it too.", "Nessun altro computer con Loupe trovato. Apri Loupe anche lì.", "Других компьютеров с Loupe пока нет. Открой Loupe и на нём."),
    ("Send", "Invia", "Отправить"),
    ("Stop", "Ferma", "Остановить"),
    ("Sending to {name}", "In invio a {name}", "Отправляется на {name}"),
    ("Receive a camera", "Ricevi una fotocamera", "Принимать камеру"),
    ("Show other computers' cameras here as a webcam", "Mostra qui le fotocamere degli altri computer come webcam", "Показывать камеры других компьютеров здесь как веб-камеру"),
    ("Apps see it as \"Loupe camera\". The first time asks for your password (to add the virtual webcam).", "Le app la vedono come \"Loupe camera\". La prima volta chiede la password (per aggiungere la webcam virtuale).", "Приложения видят её как «Loupe camera». В первый раз спросит пароль (чтобы добавить виртуальную камеру)."),
    ("Second screen", "Secondo schermo", "Второй экран"),
    ("Let a laptop extend this screen", "Lascia che un portatile estenda questo schermo", "Разрешить ноутбуку расширить этот экран"),
    ("On the laptop, open Loupe, pick this computer and type this code:", "Sul portatile apri Loupe, scegli questo computer e scrivi questo codice:", "На ноутбуке открой Loupe, выбери этот компьютер и введи код:"),
    ("Be the second screen of", "Fai da secondo schermo a", "Стать вторым экраном для"),
    ("Code", "Codice", "Код"),
    ("Connect", "Collega", "Подключить"),
    ("Showing the other screen. Leave full screen with Ctrl+Alt+Enter.", "Stai mostrando l'altro schermo. Esci dallo schermo intero con Ctrl+Alt+Invio.", "Показывается другой экран. Выйти из полноэкранного режима: Ctrl+Alt+Enter."),
    ("No computer is sharing its screen yet.", "Nessun computer condivide ancora lo schermo.", "Пока никто не делится экраном."),
];

fn language() -> usize {
    static LANG: OnceLock<usize> = OnceLock::new();
    *LANG.get_or_init(|| {
        let wanted = ["LOUPE_LANG", "LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter()
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
