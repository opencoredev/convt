//! Which targets right-click menus offer, by kind of file. People can pick
//! their own list for each [`Category`]; a kind they never changed keeps
//! [`default_menu`]. The choice lives in `menu.toml` in the convt config
//! directory, so the app, the CLI and every file manager menu agree:
//!
//! ```toml
//! audio = ["flac", "mp3"]
//! video = ["webm", "mp4", "gif"]
//! ```
//!
//! Keys are category names (`image`, `vector`, `video`, `audio`, `pdf`,
//! `document`, `presentation`, `spreadsheet`). An unknown key or format id is
//! skipped, so an older convt still reads a newer file.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::{Category, Error, Format, Result, format_by_id};

/// Every kind of file, in the order menus settings list them.
pub const MENU_KINDS: [Category; 8] = [
    Category::Image,
    Category::Vector,
    Category::Video,
    Category::Audio,
    Category::Pdf,
    Category::Document,
    Category::Spreadsheet,
    Category::Presentation,
];

/// The targets a right-click menu offers for a kind of file nobody
/// customized, most wanted first. GIF and the text formats (Markdown, JSON,
/// YAML, subtitles, TSV) have their own lists (see
/// [`crate::Registry::menu_targets`]).
pub fn default_menu(category: Category) -> &'static [&'static str] {
    match category {
        Category::Image => &["jpeg", "png", "webp", "pdf"],
        // A PDF made from an SVG holds a picture of it, not its paths, so it
        // stays in Quick convert.
        Category::Vector => &["png", "jpeg"],
        Category::Video => &["mp4", "mov", "gif", "mp3"],
        Category::Audio => &["mp3", "m4a", "wav"],
        Category::Pdf => &["png", "jpeg", "docx", "txt"],
        Category::Document => &["pdf", "docx", "txt"],
        Category::Spreadsheet => &["pdf", "xlsx", "csv"],
        Category::Presentation => &["pdf", "pptx"],
    }
}

/// The key a category has in `menu.toml`.
pub fn category_key(category: Category) -> &'static str {
    match category {
        Category::Image => "image",
        Category::Vector => "vector",
        Category::Video => "video",
        Category::Audio => "audio",
        Category::Pdf => "pdf",
        Category::Document => "document",
        Category::Presentation => "presentation",
        Category::Spreadsheet => "spreadsheet",
    }
}

/// The category named by a `menu.toml` key.
pub fn category_by_key(key: &str) -> Option<Category> {
    MENU_KINDS.into_iter().find(|c| category_key(*c) == key)
}

/// The menu lists someone picked, by kind of file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuChoices {
    lists: BTreeMap<&'static str, Vec<&'static Format>>,
}

impl MenuChoices {
    pub fn parse(toml_text: &str) -> Result<Self> {
        let table: toml::Table =
            toml::from_str(toml_text).map_err(|e| Error::InvalidOption(e.message().into()))?;
        let mut choices = Self::default();
        for (key, value) in table {
            let Some(category) = category_by_key(&key) else {
                continue;
            };
            let toml::Value::Array(items) = value else {
                return Err(Error::InvalidOption(format!(
                    "`{key}` must be a list of format ids"
                )));
            };
            let mut formats = Vec::new();
            for item in items {
                let toml::Value::String(id) = item else {
                    return Err(Error::InvalidOption(format!(
                        "`{key}` must be a list of format ids"
                    )));
                };
                if let Some(f) = format_by_id(&id) {
                    formats.push(f);
                }
            }
            choices.set(category, formats);
        }
        Ok(choices)
    }

    /// Serializes in the format [`MenuChoices::parse`] reads.
    pub fn to_toml(&self) -> String {
        let mut table = toml::Table::new();
        for (key, list) in &self.lists {
            let ids = list
                .iter()
                .map(|f| toml::Value::String(f.id.into()))
                .collect();
            table.insert((*key).into(), toml::Value::Array(ids));
        }
        toml::to_string(&table).expect("a TOML table always serializes")
    }

    /// Reads `path`. A missing file means nothing was customized; errors
    /// name the file.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| {
                let why = match e {
                    Error::InvalidOption(msg) => msg,
                    e => e.to_string(),
                };
                Error::InvalidOption(format!("{}: {why}", path.display()))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes atomically, so a menu reading the file never sees half of it.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(self.to_toml().as_bytes())?;
        tmp.persist(path).map_err(|e| Error::Io(e.error))?;
        Ok(())
    }

    /// The list picked for `category`, or `None` if it keeps the default.
    pub fn get(&self, category: Category) -> Option<&[&'static Format]> {
        self.lists.get(category_key(category)).map(Vec::as_slice)
    }

    /// Picks `formats` for `category`, in menu order. Duplicates are
    /// dropped; an empty list goes back to the default.
    pub fn set(&mut self, category: Category, formats: Vec<&'static Format>) {
        let mut unique: Vec<&'static Format> = Vec::with_capacity(formats.len());
        for f in formats {
            if !unique.contains(&f) {
                unique.push(f);
            }
        }
        if unique.is_empty() {
            self.reset(category);
        } else {
            self.lists.insert(category_key(category), unique);
        }
    }

    /// Goes back to the default list for `category`.
    pub fn reset(&mut self, category: Category) {
        self.lists.remove(category_key(category));
    }

    /// What the menu lists for `category`, ignoring what a given input can
    /// reach: the picked list, else the default.
    pub fn effective(&self, category: Category) -> Vec<&'static Format> {
        match self.get(category) {
            Some(list) => list.to_vec(),
            None => default_menu(category)
                .iter()
                .filter_map(|id| format_by_id(id))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str) -> &'static Format {
        format_by_id(id).unwrap()
    }

    #[test]
    fn round_trips_and_skips_what_it_does_not_know() {
        let mut c = MenuChoices::default();
        c.set(Category::Audio, vec![f("flac"), f("mp3"), f("flac")]);
        c.set(Category::Video, vec![f("webm")]);
        assert_eq!(c.get(Category::Audio).unwrap(), [f("flac"), f("mp3")]);
        assert_eq!(MenuChoices::parse(&c.to_toml()).unwrap(), c);

        let c = MenuChoices::parse("audio = [\"wav\", \"nope\"]\nfuture = [\"png\"]\n").unwrap();
        assert_eq!(c.get(Category::Audio).unwrap(), [f("wav")]);
        assert_eq!(c.get(Category::Image), None);
        assert!(MenuChoices::parse("audio = \"wav\"").is_err());
        assert!(MenuChoices::parse("audio = [1]").is_err());
    }

    #[test]
    fn an_empty_list_means_the_default() {
        let mut c = MenuChoices::parse("audio = [\"nope\"]").unwrap();
        assert_eq!(c.get(Category::Audio), None);
        c.set(Category::Audio, vec![f("opus")]);
        c.set(Category::Audio, vec![]);
        assert_eq!(c.get(Category::Audio), None);
        assert_eq!(c.effective(Category::Audio), [f("mp3"), f("m4a"), f("wav")]);
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/menu.toml");
        assert_eq!(MenuChoices::load(&path).unwrap(), MenuChoices::default());
        let mut c = MenuChoices::default();
        c.set(Category::Document, vec![f("odt"), f("pdf")]);
        c.save(&path).unwrap();
        assert_eq!(MenuChoices::load(&path).unwrap(), c);
        std::fs::write(&path, "document = 3").unwrap();
        let err = MenuChoices::load(&path).unwrap_err().to_string();
        assert!(err.contains("menu.toml"), "{err}");
    }

    #[test]
    fn every_default_names_a_real_format() {
        for c in MENU_KINDS {
            for id in default_menu(c) {
                assert!(format_by_id(id).is_some(), "{id}");
            }
            assert_eq!(category_by_key(category_key(c)), Some(c));
        }
    }
}
