//! A floating code editor window: open, edit and save text files (Rust
//! scripts) from inside the engine.
//!
//! This is only an editor. Nothing here compiles or runs the scripts; that
//! needs the scripting design, which is still an open decision (see
//! `HANDOFF.md`). Files live in a `scripts/` folder next to where the engine
//! is started.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

use crate::{Action, Actions};
use bevy_egui::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

/// Folder the editor lists and saves into.
pub const SCRIPTS_DIR: &str = "scripts";

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while",
];

/// One open file.
#[derive(Debug, Clone)]
pub struct Doc {
    pub path: PathBuf,
    pub text: String,
    /// What is on disk, to tell whether there are unsaved changes.
    saved: String,
}

impl Doc {
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    fn title(&self) -> String {
        let name = self
            .path
            .file_name()
            .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned());
        if self.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }
}

/// State of the code editor: a Resources window (like GameMaker's resource
/// tree) and one floating code window per open script.
#[derive(Resource, Debug, Default)]
pub struct CodeEditor {
    /// Left panel shows the Assets tree instead of the Scene list.
    pub open: bool,
    docs: Vec<Doc>,
    new_name: String,
    /// Last problem or result, shown in the Resources window.
    message: String,
}

impl CodeEditor {
    /// Window starts open and opens the file named by `IANUS_OPEN_CODE` (for automated screenshots).
    pub fn from_env() -> Self {
        let mut me = Self::default();
        if let Ok(file) = std::env::var("IANUS_OPEN_CODE") {
            me.open = true;
            me.open_file(Path::new(&file));
        }
        me
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Open a file in its own window (nothing happens if it is already open).
    pub fn open_file(&mut self, path: &Path) {
        if self.docs.iter().any(|d| d.path == path) {
            return;
        }
        match fs::read_to_string(path) {
            Ok(text) => {
                self.docs.push(Doc {
                    path: path.to_path_buf(),
                    saved: text.clone(),
                    text,
                });
                self.message.clear();
            }
            Err(e) => self.message = format!("Could not open {}: {e}", path.display()),
        }
    }

    /// Create a new empty script in the scripts folder and open it.
    fn create(&mut self, name: &str) {
        let Some(file) = script_file_name(name) else {
            self.message = "Use letters, digits, '_' or '-' for the name.".into();
            return;
        };
        let path = Path::new(SCRIPTS_DIR).join(file);
        if path.exists() {
            self.open_file(&path);
            return;
        }
        if let Err(e) = fs::create_dir_all(SCRIPTS_DIR).and_then(|()| fs::write(&path, "")) {
            self.message = format!("Could not create {}: {e}", path.display());
            return;
        }
        self.open_file(&path);
    }

    /// Draw the windows. Call every frame from the GUI system.
    pub fn show(&mut self, ctx: &egui::Context) {
        let mut closed = None;
        let mut message = None;
        for (i, doc) in self.docs.iter_mut().enumerate() {
            let mut open = true;
            egui::Window::new(doc.title())
                .id(egui::Id::new(("code_doc", &doc.path)))
                .open(&mut open)
                .default_size([520.0, 400.0])
                .default_pos([460.0 + 24.0 * i as f32, 90.0 + 24.0 * i as f32])
                .show(ctx, |ui| {
                    if let Some(m) = doc.contents(ui) {
                        message = Some(m);
                    }
                });
            if !open {
                closed = Some(i);
            }
        }
        if let Some(i) = closed {
            self.docs.remove(i);
        }
        if let Some(m) = message {
            self.message = m;
        }
    }

    /// The resource tree: a Scripts folder; double-click opens a script.
    /// The Assets view, docked in the left panel.
    pub(crate) fn resources(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.heading("Assets");
        egui::CollapsingHeader::new("Scripts")
            .default_open(true)
            .show(ui, |ui| {
                let files = list_scripts();
                if files.is_empty() {
                    ui.weak(format!("No files in {SCRIPTS_DIR}/ yet."));
                }
                for path in files {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if ui
                        .selectable_label(false, name)
                        .on_hover_text("Double-click to open")
                        .double_clicked()
                    {
                        self.open_file(&path);
                    }
                }
            });
        egui::CollapsingHeader::new("Rooms")
            .default_open(true)
            .show(ui, |ui| {
                for path in crate::rooms::list_rooms() {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if ui
                        .selectable_label(false, name)
                        .on_hover_text("Double-click to open this room")
                        .double_clicked()
                    {
                        actions.push(Action::OpenRoom(path));
                    }
                }
            });
        ui.separator();
        ui.label("New script (.rs is added)");
        let r = ui.text_edit_singleline(&mut self.new_name);
        let go = ui.button("Create").clicked()
            || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if go && !self.new_name.is_empty() {
            let name = std::mem::take(&mut self.new_name);
            self.create(&name);
        }
        if !self.message.is_empty() {
            ui.weak(&self.message);
        }
    }
}

impl Doc {
    fn save(&mut self) -> String {
        match fs::write(&self.path, &self.text) {
            Ok(()) => {
                self.saved = self.text.clone();
                format!("Saved {}", self.path.display())
            }
            Err(e) => format!("Could not save {}: {e}", self.path.display()),
        }
    }

    /// Draw one script window; returns a message when something happened.
    fn contents(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut message = None;
        ui.horizontal(|ui| {
            if ui.button("Save  (Ctrl+S)").clicked() {
                message = Some(self.save());
            }
            ui.weak(self.path.display().to_string());
        });
        ui.separator();

        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = highlight_rust(text.as_str(), ui.visuals().dark_mode);
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        egui::ScrollArea::both()
            .id_salt("code_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .layouter(&mut layouter),
                );
                // Ctrl+S saves the script you are typing in.
                if r.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
                {
                    message = Some(self.save());
                }
            });
        message
    }
}

/// `name` -> `name.rs`, or `None` if the name is empty or has odd characters.
/// Only plain names are allowed so the editor cannot write outside `scripts/`.
fn script_file_name(name: &str) -> Option<String> {
    let stem = name.trim().strip_suffix(".rs").unwrap_or(name.trim());
    let ok = !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then(|| format!("{stem}.rs"))
}

/// `.rs` files in the scripts folder, sorted by name.
fn list_scripts() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(SCRIPTS_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    files
}

/// Colour Rust source: keywords, comments, strings, numbers. A small
/// hand-written pass, not a parser, so odd code may be coloured a little
/// wrong. It never changes the text.
fn highlight_rust(text: &str, dark: bool) -> LayoutJob {
    let (plain, keyword, comment, string, number) = if dark {
        (
            Color32::from_gray(210),
            Color32::from_rgb(255, 140, 120),
            Color32::from_gray(120),
            Color32::from_rgb(150, 210, 120),
            Color32::from_rgb(130, 180, 255),
        )
    } else {
        (
            Color32::from_gray(40),
            Color32::from_rgb(170, 30, 30),
            Color32::from_gray(130),
            Color32::from_rgb(40, 120, 30),
            Color32::from_rgb(30, 70, 190),
        )
    };
    let font = FontId::monospace(14.0);
    let mut job = LayoutJob::default();
    let mut push = |s: &str, color: Color32| {
        if !s.is_empty() {
            job.append(s, 0.0, TextFormat::simple(font.clone(), color));
        }
    };

    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let end = if rest.starts_with("//") {
            let n = rest.find('\n').unwrap_or(rest.len());
            push(&rest[..n], comment);
            n
        } else if c == '"' {
            let n = string_len(rest);
            push(&rest[..n], string);
            n
        } else if c.is_ascii_digit() {
            let n = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .unwrap_or(rest.len());
            push(&rest[..n], number);
            n
        } else if c.is_alphabetic() || c == '_' {
            let n = rest
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .unwrap_or(rest.len());
            let word = &rest[..n];
            push(
                word,
                if KEYWORDS.contains(&word) {
                    keyword
                } else {
                    plain
                },
            );
            n
        } else {
            let n = c.len_utf8();
            push(&rest[..n], plain);
            n
        };
        rest = &rest[end..];
    }
    job
}

/// Length in bytes of the string literal at the start of `s` (which starts
/// with `"`), including the closing quote, or to the end if it is unclosed.
fn string_len(s: &str) -> usize {
    let mut escaped = false;
    for (i, ch) in s.char_indices().skip(1) {
        match ch {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return i + 1,
            _ => {}
        }
    }
    s.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_plain_and_get_an_extension() {
        assert_eq!(script_file_name("miner"), Some("miner.rs".into()));
        assert_eq!(script_file_name(" miner.rs "), Some("miner.rs".into()));
        assert_eq!(script_file_name(""), None);
        assert_eq!(script_file_name("../evil"), None);
        assert_eq!(script_file_name("a/b"), None);
    }

    #[test]
    fn string_length_handles_escapes_and_unclosed_strings() {
        assert_eq!(string_len(r#""ab" rest"#), 4);
        assert_eq!(string_len(r#""a\"b" x"#), 6);
        assert_eq!(string_len(r#""open"#), 5);
    }

    #[test]
    fn highlighting_never_changes_the_text() {
        let src = "fn main() {\n    let s = \"hi\\\"\"; // é comment\n    let n = 1_000.5;\n}\n\"unclosed";
        let job = highlight_rust(src, true);
        assert_eq!(job.text, src);
    }

    #[test]
    fn keywords_get_a_different_colour_than_names() {
        let job = highlight_rust("fn foo", true);
        let color_of = |word: &str| {
            job.sections
                .iter()
                .find(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0] == word)
                .map(|s| s.format.color)
        };
        assert_ne!(color_of("fn"), color_of("foo"));
    }
}
