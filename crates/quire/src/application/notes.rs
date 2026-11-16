//! Markdown notes, notebooks and pictures (port of `services/notes.py`; Ligature diagrams come with the desktop shell).

use std::rc::Rc;

use chrono::NaiveDate;

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, CourseLookup, CourseRepository, ImageRepository, NoteRepository, NotebookRepository};
use crate::domain::model::{image_markdown, image_uids, normalize_topic, note_snippet, Course, Note, NoteImage, Notebook};
use crate::domain::ValidationError;

pub const SNIPPET: usize = 90; // same as note_snippet(body) in Python

pub fn class_note_title(course: &str, day: NaiveDate) -> String {
    format!("{course}: {} {} {}", day.format("%A"), day.format("%-d"), day.format("%B %Y"))
}

#[derive(Clone, Debug)]
pub struct NoteSummary {
    pub id: i64,
    pub title: String,
    pub pinned: bool,
    pub updated: Option<chrono::NaiveDateTime>,
    pub course: Option<Course>,
    pub topic: String,
    pub snippet: String,
    pub notebook: Option<Notebook>,
}

#[derive(Clone, Debug)]
pub struct NoteGroup {
    pub course: Option<Course>,
    pub topic: String,
    pub notes: Vec<NoteSummary>,
    pub notebook: Option<Notebook>,
}

pub struct NoteService {
    pub notes: Rc<dyn NoteRepository>,
    pub courses: Rc<dyn CourseRepository>,
    pub lookup: Rc<dyn CourseLookup>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    pub images: Rc<dyn ImageRepository>,
    pub notebooks: Rc<dyn NotebookRepository>,
}

impl NoteService {
    /// Pinned notes first, then most recently edited.
    pub fn search(&self, text: &str, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<NoteSummary>> {
        let courses = self.courses.list()?;
        let books = self.notebooks.list()?;
        Ok(self.notes.search(text.trim(), course_id, notebook_id)?.into_iter().map(|n| NoteSummary {
            id: n.id.unwrap_or_default(), title: n.title(), pinned: n.pinned, updated: n.updated,
            course: courses.iter().find(|c| n.course_id.is_some() && c.id == n.course_id).cloned(),
            topic: n.topic.clone(), snippet: note_snippet(&n.body, SNIPPET),
            notebook: books.iter().find(|b| n.notebook_id.is_some() && b.id == n.notebook_id).cloned(),
        }).collect())
    }

    /// Grouped by class, then notebook, then neither (each alphabetical); within, topics with no topic last.
    pub fn grouped(&self, text: &str, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<NoteGroup>> {
        let mut groups: Vec<((u8, String, i64, bool, String), NoteGroup)> = Vec::new();
        for s in self.search(text, course_id, notebook_id)? {
            let home = match (&s.course, &s.notebook) {
                (Some(c), _) => (0u8, c.name.to_lowercase(), c.id.unwrap_or_default()),
                (None, Some(b)) => (1, b.name.to_lowercase(), b.id.unwrap_or_default()),
                _ => (2, String::new(), 0),
            };
            let key = (home.0, home.1, home.2, s.topic.is_empty(), s.topic.to_lowercase());
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, g)) => g.notes.push(s),
                None => groups.push((key, NoteGroup { course: s.course.clone(), topic: s.topic.clone(),
                                                      notebook: s.notebook.clone(), notes: vec![s] })),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(groups.into_iter().map(|(_, g)| g).collect())
    }

    pub fn topics(&self, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<String>> {
        let mut t = self.notes.topics(course_id, notebook_id)?;
        t.sort_by_key(|x| x.to_lowercase());
        Ok(t)
    }

    pub fn rename_topic(&self, course_id: Option<i64>, old: &str, new: &str, notebook_id: Option<i64>) -> AppResult<usize> {
        let (old, new) = (normalize_topic(old), normalize_topic(new));
        if old.is_empty() || old == new {
            return Ok(0);
        }
        let n = self.notes.rename_topic(course_id, &old, &new, notebook_id)?;
        self.bus.publish(Topic::Notes);
        Ok(n)
    }

    fn get(&self, id: i64) -> AppResult<Note> {
        self.notes.get(id)?.ok_or_else(|| AppError::NotFound(format!("Note {id} does not exist.")))
    }

    pub fn note(&self, id: i64) -> AppResult<Note> {
        self.get(id)
    }

    /// Reuse an existing topic's spelling when only the letter case differs.
    fn canonical_topic(&self, course_id: Option<i64>, topic: &str, notebook_id: Option<i64>) -> AppResult<String> {
        let topic = normalize_topic(topic);
        Ok(self.notes.topics(course_id, notebook_id)?.into_iter().find(|t| t.to_lowercase() == topic.to_lowercase()).unwrap_or(topic))
    }

    fn check_home(&self, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult {
        if course_id.is_some() && notebook_id.is_some() {
            return Err(ValidationError("A note goes in a class or in a notebook, not both.").into());
        }
        if let Some(b) = notebook_id {
            if self.notebooks.get(b)?.is_none() {
                return Err(AppError::NotFound(format!("Notebook {b} does not exist.")));
            }
        }
        Ok(())
    }

    pub fn create(&self, body: &str, course_id: Option<i64>, topic: &str, notebook_id: Option<i64>) -> AppResult<Note> {
        self.check_home(course_id, notebook_id)?;
        let mut note = Note { body: body.into(), course_id, updated: Some(self.clock.now()),
                              topic: self.canonical_topic(course_id, topic, notebook_id)?, notebook_id, ..Note::default() };
        note.id = Some(self.notes.add(&note)?);
        self.bus.publish(Topic::Notes);
        Ok(note)
    }

    /// Save an edit; returns the note as stored (with the topic's canonical spelling).
    pub fn update(&self, id: i64, body: &str, course_id: Option<i64>, pinned: bool, topic: &str, notebook_id: Option<i64>) -> AppResult<Note> {
        let mut note = self.get(id)?;
        self.check_home(course_id, notebook_id)?;
        note.body = body.into();
        note.course_id = course_id;
        note.pinned = pinned;
        note.notebook_id = notebook_id;
        note.topic = self.canonical_topic(course_id, topic, notebook_id)?;
        note.updated = Some(self.clock.now());
        self.notes.update(&note)?;
        self.bus.publish(Topic::Notes);
        Ok(note)
    }

    pub fn delete(&self, id: i64) -> AppResult {
        self.notes.delete(id)?;
        self.bus.publish(Topic::Notes);
        Ok(())
    }

    /// The note for one lesson of a course, created on first use.
    pub fn class_note(&self, course_id: i64, day: NaiveDate) -> AppResult<Note> {
        let course = self.lookup.get(course_id)?.ok_or_else(|| AppError::NotFound(format!("Course {course_id} does not exist.")))?;
        let title = class_note_title(&course.name, day);
        match self.notes.find_by_title(&title, Some(course_id))? {
            Some(n) => Ok(n),
            None => self.create(&format!("# {title}\n\n"), Some(course_id), "", None),
        }
    }

    // ---- notebooks ----

    /// Each notebook with how many notes it holds.
    pub fn notebooks(&self) -> AppResult<Vec<(Notebook, usize)>> {
        self.notebooks.list()?.into_iter().map(|b| { let n = self.notebooks.note_count(b.id.unwrap_or_default())?; Ok((b, n)) }).collect()
    }

    pub fn create_notebook(&self, name: &str, color: &str) -> AppResult<Notebook> {
        let mut book = Notebook { name: name.trim().into(), color: color.into(), id: None };
        book.validate()?;
        book.id = Some(self.notebooks.add(&book)?);
        self.bus.publish(Topic::Notes);
        Ok(book)
    }

    pub fn update_notebook(&self, id: i64, name: &str, color: &str) -> AppResult<Notebook> {
        let mut book = self.notebooks.get(id)?.ok_or_else(|| AppError::NotFound(format!("Notebook {id} does not exist.")))?;
        book.name = name.trim().into();
        book.color = color.into();
        book.validate()?;
        self.notebooks.update(&book)?;
        self.bus.publish(Topic::Notes);
        Ok(book)
    }

    /// Delete a notebook; its notes stay, filed nowhere.
    pub fn delete_notebook(&self, id: i64) -> AppResult {
        self.notebooks.delete(id)?;
        self.bus.publish(Topic::Notes);
        Ok(())
    }

    // ---- pictures ----

    /// Keep a picture for notes; returns the Markdown that shows it.
    pub fn add_image(&self, data: &[u8], mime: &str, alt: &str) -> AppResult<String> {
        let image = NoteImage { uid: self.images.new_uid(), mime: mime.into(), data: data.to_vec(), id: None };
        image.check()?;
        self.images.add(&image)?;
        Ok(image_markdown(&image.uid, alt))
    }

    pub fn image(&self, uid: &str) -> AppResult<Option<NoteImage>> {
        self.images.get(uid)
    }

    /// The pictures a note's text shows (those that have arrived on this device).
    pub fn images_in(&self, body: &str) -> AppResult<Vec<NoteImage>> {
        let mut out = Vec::new();
        for uid in image_uids(body) {
            if let Some(i) = self.images.get(&uid)? {
                out.push(i);
            }
        }
        Ok(out)
    }
}
