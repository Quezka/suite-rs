//! evolution source + file write, in temp dirs (never real `~/.config`)

use quire::application::ports::SystemCalendar;
use quire::infrastructure::system_calendar::EvolutionCalendar;

#[test]
fn evolution_source_follows_the_file() {
    let root = std::env::temp_dir().join(format!("quire-cal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cal = EvolutionCalendar::new(Some(root.join("data")), Some(root.join("sources")));
    cal.publish("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").unwrap();
    assert_eq!(std::fs::read(&cal.file).unwrap(), b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n");
    let first = std::fs::metadata(&cal.file).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    cal.publish("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").unwrap();
    assert_eq!(std::fs::metadata(&cal.file).unwrap().modified().unwrap(), first, "same text => no rewrite");
    cal.connect().unwrap();
    let source = std::fs::read_to_string(root.join("sources").join(EvolutionCalendar::SOURCE)).unwrap();
    assert!(source.contains(&format!("CustomFile=file://{}\nWritable=false", cal.file.display())));
    cal.disconnect().unwrap();
    cal.disconnect().unwrap();
    assert!(!root.join("sources").join(EvolutionCalendar::SOURCE).exists());
    std::fs::remove_dir_all(&root).unwrap();
}
