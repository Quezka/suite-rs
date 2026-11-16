//! startup: bg on close default on; login entry in temp dir (never real `~/.config`)

use std::rc::Rc;

use quezka_kit::autostart::LoginItem as KitLoginItem;
use quire::application::startup::StartupService;
use quire::infrastructure::login_item::SystemLoginItem;
use quire::infrastructure::repositories::SqliteSettings;
use quire::infrastructure::sqlite::Database;

#[test]
fn background_and_login_entry() {
    let dir = std::env::temp_dir().join(format!("quire-login-{}", std::process::id()));
    let item = KitLoginItem { app_id: "io.github.quezka.Quire".into(), name: "Quire".into(), comment: "Start Quire in the background".into(),
        command: vec!["/opt/quire/quire".into(), "--background".into()], config_dir: Some(dir.clone()) };
    let s = StartupService { login_item: Rc::new(SystemLoginItem(item.clone())), settings: Rc::new(SqliteSettings(Rc::new(Database::in_memory().unwrap()))) };
    assert!(s.background_on_close());
    s.set_background_on_close(false).unwrap();
    assert!(!s.background_on_close());
    assert!(!s.start_on_login());
    s.set_start_on_login(true).unwrap();
    assert!(s.start_on_login());
    std::fs::write(item.desktop_file(), "old").unwrap();
    s.refresh_login_item();
    assert!(std::fs::read_to_string(item.desktop_file()).unwrap().contains("Exec=/opt/quire/quire --background"));
    s.set_start_on_login(false).unwrap();
    assert!(!s.start_on_login());
    let _ = std::fs::remove_dir_all(dir);
}
