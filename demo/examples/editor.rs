//! `editor` — a multi-line code editor with syntax highlighting.
//!
//! ```sh
//! cargo run -p kasumi-demo --example editor
//! ```
//!
//! This example puts the pieces of Kasumi's IDE-oriented text stack together:
//!
//! - `TextEditor` bound to `State<String>`: line numbers, current-line band,
//!   multi-line selection, two-axis scrolling that follows the caret, Undo/Redo,
//!   IME pre-edit, clipboard and a built-in context menu.
//! - `SyntaxHighlighter` (Rust / C / Python / JSON / Shell) or any custom
//!   `Highlighter` implementation, producing byte-range `TextSpan`s.
//! - `Label::spans` for cheap inline colouring without splitting a paragraph.
//! - `kasumi-text`'s `TextAnalysis` (UAX #29 grapheme/word boundaries, bracket
//!   matching, tab stops) and `TextLayout` (line-aware caret/hit testing).
//!
//! Try: Enter (auto-indent), Tab / Shift+Tab (indent), `(` (auto-pair),
//! Ctrl+/ (toggle comment), Ctrl+D (duplicate line), Alt+↑/↓ (move lines),
//! Ctrl+B (jump to matching bracket), Ctrl+Z / Ctrl+Shift+Z (undo/redo),
//! double-click a word, triple-click a line, or scroll with the wheel.

use kasumi::core::TextSpan;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

const RUST_SAMPLE: &str = r#"// Kasumi's text stack in one function.
use kasumi_text::{TextAnalysis, TextLayout};

#[derive(Debug, Clone)]
pub struct Document {
    pub name: String,
    pub lines: Vec<String>,
}

impl Document {
    pub fn new(name: &str, source: &str) -> Self {
        let lines = source.lines().map(str::to_owned).collect();
        Self { name: name.to_owned(), lines }
    }

    /// Returns the byte offset of a (line, column) position.
    pub fn offset(&self, line: usize, column: usize) -> usize {
        let layout = TextLayout::new(self.source());
        layout.byte_at_position(line, column)
    }

    pub fn word_at(&self, byte: usize) -> &str {
        let analysis = TextAnalysis::new(self.source());
        let range = analysis.word_range_at(self.source(), byte);
        &self.source()[range]
    }
}
"#;

struct EditorDemo;

impl App for EditorDemo {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let source = ui.state(|| RUST_SAMPLE.to_owned());
        let language = ui.state(|| 0usize);
        let show_whitespace = ui.state(|| false);
        let line_numbers = ui.state(|| true);

        shared::page(
            ui,
            "Code editor",
            "TextEditor · SyntaxHighlighter · TextSpan · TextAnalysis",
            "editor",
            |ui| {
                ui.column(|ui| {
                    ui.width_full().gap(12.0);

                    ui.card(Card::filled().padding(theme.spacing.lg).gap(8.0), |ui| {
                        ui.width_full();
                        heading(ui, "What this shows", "kasumi-text IDE primitives");
                        note(
                            ui,
                            "Syntax colours are byte-range TextSpans: the layout cache stays keyed \
                             by text and shaping attributes, so recolouring never reshapes. \
                             Glyphs carry their own byte range, which makes highlighting correct \
                             even after bidi reordering.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            for (index, label) in ["Rust", "C", "Python", "JSON", "Shell"]
                                .iter()
                                .enumerate()
                            {
                                let active = language.get() == index;
                                let button = Button::new(*label)
                                    .font_size(11.0)
                                    .padding_xy(10.0, 5.0);
                                let button = if active {
                                    button.filled()
                                } else {
                                    button.tonal()
                                };
                                if ui.add(button).clicked() {
                                    language.set(index);
                                }
                            }
                            ui.spacer();
                            if ui
                                .add(
                                    Button::new(if show_whitespace.get() {
                                        "Hide whitespace"
                                    } else {
                                        "Show whitespace"
                                    })
                                    .font_size(11.0)
                                    .subtle(),
                                )
                                .clicked()
                            {
                                show_whitespace.update(|value| *value = !*value);
                            }
                        });
                    });

                    ui.row(|ui| {
                        ui.width_full().gap(16.0).align_items(Align::Stretch);
                        ui.column(|ui| {
                            ui.grow(1.0).basis(0.0);
                            let colors = if theme.colors.background.r < 0.5 {
                                SyntaxColors::dark()
                            } else {
                                SyntaxColors::light()
                            };
                            let highlighter = match language.get() {
                                0 => SyntaxHighlighter::rust(colors),
                                1 => SyntaxHighlighter::new(Language::C, colors),
                                2 => SyntaxHighlighter::new(Language::Python, colors),
                                3 => SyntaxHighlighter::new(Language::Json, colors),
                                _ => SyntaxHighlighter::new(Language::Shell, colors),
                            };
                            let (response, status) = ui.add(
                                TextEditor::new(&source)
                                    .highlighter(highlighter)
                                    .height(420.0)
                                    .font_size(13.0)
                                    .line_height(20.0)
                                    .tab_width(4)
                                    .line_numbers(line_numbers.get())
                                    .show_whitespace(show_whitespace.get())
                                    .guide_column(80)
                                    .highlight_trailing_whitespace(true),
                            );
                            let _ = response;
                            ui.row(|ui| {
                                ui.width_full().gap(12.0).align_items(Align::Center);
                                micro(
                                    ui,
                                    format!(
                                        "Ln {}, Col {} · {} lines · {} selected{}",
                                        status.line,
                                        status.column,
                                        status.line_count,
                                        status.selection_len,
                                        if status.changed { " · edited" } else { "" }
                                    ),
                                );
                                ui.spacer();
                                if ui
                                    .add(Button::new("Line numbers").font_size(10.0).subtle())
                                    .clicked()
                                {
                                    line_numbers.update(|value| *value = !*value);
                                }
                            });
                        });

                        ui.column(|ui| {
                            ui.width(260.0).flex_shrink(0.0).gap(12.0);
                            ui.card(Card::outlined().padding(theme.spacing.md).gap(8.0), |ui| {
                                heading(ui, "Rich labels", "Label::spans");
                                note(
                                    ui,
                                    "A label can colour byte ranges without becoming several \
                                     layout nodes. Word wrapping and bidi still see one paragraph.",
                                );
                                let demo = "let value = 42; // highlighted in one pass";
                                ui.add(
                                    Label::new(demo)
                                        .font_size(11.0)
                                        .family(FontFamily::Monospace)
                                        .spans([
                                            TextSpan::new(0, 3, theme.colors.primary),
                                            TextSpan::new(4, 9, theme.colors.tertiary),
                                            TextSpan::new(12, 14, theme.colors.error),
                                            TextSpan::new(17, demo.len(), theme.colors.outline),
                                        ]),
                                );
                                ui.divider();
                                code(ui, "TextSpan { start, end, color }");
                            });
                            ui.card(Card::outlined().padding(theme.spacing.md).gap(8.0), |ui| {
                                heading(ui, "Editor commands", "TextEdit");
                                for line in [
                                    "Enter · newline + auto-indent",
                                    "Tab / Shift+Tab · indent / outdent",
                                    "( [ { \" · auto-pair",
                                    "Ctrl+/ · toggle comment",
                                    "Ctrl+D · duplicate line",
                                    "Alt+↑ / ↓ · move lines",
                                    "Ctrl+B · matching bracket",
                                    "Ctrl+Z / Shift+Z · undo / redo",
                                    "Double / triple click · word / line",
                                ] {
                                    micro(ui, line);
                                }
                            });
                        });
                    });

                    ui.card(Card::outlined().padding(theme.spacing.md).gap(8.0), |ui| {
                        heading(ui, "Analysis helpers", "kasumi-text");
                        note(
                            ui,
                            "TextAnalysis exposes UAX #29 grapheme/word boundaries, logical line \
                             ranges, tab stops and bracket pairs. TextLayout adds line-aware caret \
                             geometry, byte_at_point(x, y), selection_rects and char↔byte indices.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(8.0);
                            for api in [
                                "caret_at(byte)",
                                "byte_at_point(x, y)",
                                "selection_rects(..)",
                                "prev_grapheme(byte)",
                                "word_range_at(byte)",
                                "matching_bracket(byte)",
                                "logical_clusters(line)",
                            ] {
                                ui.column(|ui| {
                                    ui.padding_xy(8.0, 4.0)
                                        .radius(theme.shape.extra_small)
                                        .background(theme.colors.surface_container_high);
                                    code(ui, api);
                                });
                            }
                        });
                    });
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: EditorDemo },
        options("Kasumi — code editor", 1180.0, 860.0),
    )
}
