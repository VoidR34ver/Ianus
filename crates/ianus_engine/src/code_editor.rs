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

/// State of the code editor window.
#[derive(Resource, Debug, Default)]
pub struct CodeEditor {
    pub open: bool,
    docs: Vec<Doc>,
    active: usize,
    new_name: String,
    /// Last problem or result, shown under the text.
    message: String,
}

impl CodeEditor {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Open a file (or switch to it if it is already open).
    pub fn open_file(&mut self, path: &Path) {
        if let Some(i) = self.docs.iter().position(|d| d.path == path) {
            self.active = i;
            return;
        }
        match fs::read_to_string(path) {
            Ok(text) => {
                self.docs.push(Doc {
                    path: path.to_path_buf(),
                    saved: text.clone(),
                    text,
                });
                self.active = self.docs.len() - 1;
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

    fn save_active(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        match fs::write(&doc.path, &doc.text) {
            Ok(()) => {
                doc.saved = doc.text.clone();
                self.message = format!("Saved {}", doc.path.display());
            }
            Err(e) => self.message = format!("Could not save {}: {e}", doc.path.display()),
        }
    }

    fn close_active(&mut self) {
        if self.active < self.docs.len() {
            self.docs.remove(self.active);
            self.active = self.active.saturating_sub(1);
        }
    }

    /// Draw the window. Call every frame from the GUI system.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Code")
            .open(&mut open)
            .default_size([560.0, 420.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.contents(ui));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        // Ctrl+S saves while the editor has focus.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_active();
        }

        ui.horizontal_wrapped(|ui| {
            for i in 0..self.docs.len() {
                let title = self.docs[i].title();
                if ui.selectable_label(self.active == i, title).clicked() {
                    self.active = i;
                }
            }
            ui.menu_button("Open...", |ui| {
                let files = list_scripts();
                if files.is_empty() {
                    ui.weak(format!("No files in {SCRIPTS_DIR}/ yet."));
                }
                for path in files {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if ui.button(name).clicked() {
                        self.open_file(&path);
                        ui.close();
                    }
                }
            });
            ui.menu_button("New...", |ui| {
                ui.label("File name (.rs is added)");
                let r = ui.text_edit_singleline(&mut self.new_name);
                let go = ui.button("Create").clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.new_name.is_empty() {
                    let name = std::mem::take(&mut self.new_name);
                    self.create(&name);
                    ui.close();
                }
            });
        });
        ui.horizontal(|ui| {
            let has_doc = self.active < self.docs.len();
            if ui
                .add_enabled(has_doc, egui::Button::new("Save  (Ctrl+S)"))
                .clicked()
            {
                self.save_active();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Close"))
                .clicked()
            {
                self.close_active();
            }
            if let Some(doc) = self.docs.get(self.active) {
                ui.weak(doc.path.display().to_string());
            }
        });
        ui.separator();

        let message = self.message.clone();
        let Some(doc) = self.docs.get_mut(self.active) else {
            ui.weak("No file open. Use Open... or New... above.");
            return;
        };

        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = highlight_rust(text.as_str(), ui.visuals().dark_mode);
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        // Leave a line for the message under the text.
        let height = (ui.available_height() - 24.0).max(60.0);
        egui::ScrollArea::both()
            .id_salt("code_scroll")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut doc.text)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .layouter(&mut layouter),
                );
            });
        if !message.is_empty() {
            ui.weak(message);
        }
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
