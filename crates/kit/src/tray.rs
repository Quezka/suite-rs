//! Tray icon (linux: StatusNotifierItem via `ksni`, gnome needs appindicator ext = ubuntu default).
//! App polls `Tray::events` each frame (also while hidden: runner ticks unseen) -> show window | quit.
//! Other OS: `spawn` -> None (todo windows `tray-icon`).

use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    Quit,
}

pub struct Tray {
    rx: Receiver<TrayEvent>,
    #[cfg(target_os = "linux")]
    handle: ksni::blocking::Handle<imp::Item>,
}

impl Tray {
    /// Clicks since last call.
    pub fn events(&self) -> Vec<TrayEvent> {
        self.rx.try_iter().collect()
    }

    /// Tooltip / status line under the title (e.g. "Connected to Redmi").
    pub fn set_status(&self, status: &str) {
        #[cfg(target_os = "linux")]
        {
            let s = status.to_string();
            self.handle.update(move |t| t.status = s);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = status;
    }
}

/// `icon`: RGBA pixels + side. Labels already translated. None = no tray here (or no D-Bus).
pub fn spawn(id: &str, title: &str, icon: Option<(Vec<u8>, u32)>, show: &str, quit: &str) -> Option<Tray> {
    let (tx, rx) = channel();
    #[cfg(target_os = "linux")]
    {
        use ksni::blocking::TrayMethods;
        let item = imp::Item {
            id: id.into(),
            title: title.into(),
            status: String::new(),
            icon: icon.map(|(rgba, side)| imp::argb(&rgba, side)),
            show: show.into(),
            quit: quit.into(),
            tx,
        };
        let handle = item.spawn().ok()?;
        Some(Tray { rx, handle })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (id, title, icon, show, quit, tx, rx);
        None
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::TrayEvent;
    use std::sync::mpsc::Sender;

    pub struct Item {
        pub id: String,
        pub title: String,
        pub status: String,
        pub icon: Option<ksni::Icon>,
        pub show: String,
        pub quit: String,
        pub tx: Sender<TrayEvent>,
    }

    /// RGBA -> ARGB32 network order (SNI wants it).
    pub fn argb(rgba: &[u8], side: u32) -> ksni::Icon {
        let data = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
        ksni::Icon { width: side as i32, height: side as i32, data }
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            self.id.clone()
        }
        fn title(&self) -> String {
            self.title.clone()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.iter().map(|i| ksni::Icon { width: i.width, height: i.height, data: i.data.clone() }).collect()
        }
        fn icon_name(&self) -> String {
            if self.icon.is_some() { String::new() } else { self.id.clone() }
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: self.title.clone(), description: self.status.clone(), ..Default::default() }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.send(TrayEvent::Show);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            let mut items: Vec<ksni::MenuItem<Self>> = Vec::new();
            if !self.status.is_empty() {
                items.push(StandardItem { label: self.status.clone(), enabled: false, ..Default::default() }.into());
                items.push(ksni::MenuItem::Separator);
            }
            items.push(StandardItem { label: self.show.clone(), activate: Box::new(|t: &mut Self| { let _ = t.tx.send(TrayEvent::Show); }), ..Default::default() }.into());
            items.push(StandardItem { label: self.quit.clone(), activate: Box::new(|t: &mut Self| { let _ = t.tx.send(TrayEvent::Quit); }), ..Default::default() }.into());
            items
        }
    }
}
