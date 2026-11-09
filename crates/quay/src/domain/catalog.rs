//! The apps of the suite: where each is released and how each platform knows it. `key` is also the .deb package
//! and its command; `windows_guid` is the Inno Setup AppId, the name of its entry in Windows' installed programs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuiteApp {
    pub key: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    /// "owner/name" on GitHub.
    pub repo: &'static str,
    pub windows_guid: &'static str,
    pub windows_exe: &'static str,
}

const fn app(key: &'static str, name: &'static str, tagline: &'static str, repo: &'static str,
             windows_guid: &'static str, windows_exe: &'static str) -> SuiteApp {
    SuiteApp { key, name, tagline, repo, windows_guid, windows_exe }
}

pub const CATALOG: [SuiteApp; 10] = [
    app("quire", "Quire", "Notes, day planner and school timetable", "Quezka/quire", "A8A67700-9677-45B8-8C82-78A07D4DFD71", "Quire.exe"),
    app("gantry", "Gantry", "Gantt charts that open and save GanttProject files", "Quezka/gantry", "76971ED7-094F-4440-AF21-DE3524CFAEB4", "Gantry.exe"),
    app("ligature", "Ligature", "ER and UML class diagrams, with SQL generation", "Quezka/ligature", "3C1E7B52-6A0D-4F2B-9E4A-8D5F21C7A9B6", "Ligature.exe"),
    app("folio", "Folio", "Databases made easy: tables, forms and reports, and Access files", "Quezka/folio", "5E0B7A44-2C1D-4F6A-8B93-71D2E4A06C58", "Folio.exe"),
    app("bivouac", "Bivouac", "Base camp for CTF study: flashcards, drills and cheatsheets", "Quezka/bivouac", "8F4A2C61-3B7E-4D95-A0C8-5E1B9D7F24A3", "Bivouac.exe"),
    app("facet", "Facet", "A light 3D model viewer with precise measuring", "Quezka/facet", "8B4D2F61-1E7A-4C93-A5D0-6F3B9E21C8A7", "Facet.exe"),
    app("missive", "Missive", "A calm, private, read-only mail client", "Quezka/missive", "B7C3D9E2-4A15-4E68-9F20-3D8A6C1E5B74", "Missive.exe"),
    app("stagno", "Stagno", "The DuckNet browser", "Quezka/ducknet", "B7D4E2A1-5C93-4E68-A0F7-2D1B9C8E6A43", "Stagno.exe"),
    app("natter", "Natter", "WhatsApp Web in a light window of its own", "Quezka/natter", "8A41B7D0-13ED-40B1-A3DA-A5BCFA0C1DF7", "Natter.exe"),
    app("quay", "Quay", "This launcher: installs and updates the suite", "Quezka/quay", "C2F81A6D-93B4-4D07-A5E1-7B3C0D94F2A8", "Quay.exe"),
];

pub fn by_key(key: &str) -> Option<&'static SuiteApp> {
    CATALOG.iter().find(|a| a.key == key)
}
