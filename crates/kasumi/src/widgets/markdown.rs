use kasumi_core::{Align, Edges, FontFamily, FontWeight, Length, TextWrap};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// A CommonMark/GFM renderer built from the framework's text and layout
/// primitives. Headings, paragraphs, emphasis, code, links, quotes, lists,
/// task-list markers, rules, and tables' cell text are supported.
#[derive(Debug, Clone)]
pub struct Markdown {
    source: String,
    font_size: f32,
    width: Length,
}

impl Markdown {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            font_size: 14.0,
            width: Length::Percent(100.0),
        }
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size.max(6.0);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl Widget for Markdown {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let theme = ui.theme().clone();
        let blocks = parse(&self.source);
        ui.column(|ui| {
            ui.width(self.width).gap(11.0);
            for (index, block) in blocks.iter().enumerate() {
                ui.keyed(index, |ui| {
                    render_block(ui, block, &theme, self.font_size);
                });
            }
        })
    }
}

#[derive(Debug, Clone)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
}

#[derive(Debug, Clone)]
enum Block {
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    Quote(Vec<Span>),
    ListItem(String, Vec<Span>),
    Code(String),
    Rule,
}

#[derive(Debug, Clone, Copy)]
enum DraftKind {
    Heading(u8),
    Paragraph,
    Quote,
    ListItem,
}

#[derive(Debug)]
struct Draft {
    kind: DraftKind,
    marker: Option<String>,
    spans: Vec<Span>,
}

fn parse(source: &str) -> Vec<Block> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut blocks = Vec::new();
    let mut current: Option<Draft> = None;
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut link = false;
    let mut quote_depth = 0usize;
    let mut quote_pending = false;
    let mut list_number: Option<u64> = None;
    let mut item_marker: Option<String> = None;
    let mut code_block: Option<String> = None;
    let mut in_image = false;

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                finish(&mut current, &mut blocks);
                current = Some(Draft {
                    kind: DraftKind::Heading(heading_level(level)),
                    marker: None,
                    spans: Vec::new(),
                });
            }
            Event::Start(Tag::Paragraph) if code_block.is_none() => {
                if current.is_none() {
                    let marker = item_marker.take();
                    let kind = if quote_depth > 0 {
                        DraftKind::Quote
                    } else if marker.is_some() {
                        DraftKind::ListItem
                    } else {
                        DraftKind::Paragraph
                    };
                    current = Some(Draft {
                        kind,
                        marker,
                        spans: Vec::new(),
                    });
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                finish(&mut current, &mut blocks);
                quote_depth += 1;
                quote_pending = true;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                finish(&mut current, &mut blocks);
                code_block = Some(String::new());
            }
            Event::Start(Tag::List(start)) => {
                list_number = start;
            }
            Event::Start(Tag::Item) => {
                finish(&mut current, &mut blocks);
                item_marker = Some(if let Some(number) = list_number {
                    list_number = Some(number + 1);
                    format!("{number}.")
                } else {
                    "•".to_owned()
                });
            }
            Event::Start(Tag::Strong) => bold += 1,
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::Start(Tag::Link { .. }) => link = true,
            Event::Start(Tag::Image { .. }) => {
                in_image = true;
                push_span(&mut current, "[", bold, italic, false, link);
            }
            Event::End(TagEnd::Heading(_)) => finish(&mut current, &mut blocks),
            Event::End(TagEnd::Paragraph) => finish(&mut current, &mut blocks),
            Event::End(TagEnd::BlockQuote(_)) => {
                finish(&mut current, &mut blocks);
                quote_depth = quote_depth.saturating_sub(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(code) = code_block.take() {
                    blocks.push(Block::Code(code.trim_end().to_owned()));
                }
            }
            Event::End(TagEnd::List(_)) => {
                finish(&mut current, &mut blocks);
                list_number = None;
            }
            Event::End(TagEnd::Item) => {
                finish(&mut current, &mut blocks);
                item_marker = None;
            }
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::End(TagEnd::Link) => link = false,
            Event::End(TagEnd::Image) => {
                in_image = false;
                push_span(&mut current, "](", bold, italic, false, link);
            }
            Event::Text(text) => {
                if let Some(code) = code_block.as_mut() {
                    code.push_str(&text);
                } else {
                    ensure_current(&mut current, quote_depth, quote_pending, item_marker.take());
                    push_span(&mut current, &text, bold, italic, false, link);
                }
            }
            Event::Code(text) | Event::InlineMath(text) => {
                ensure_current(&mut current, quote_depth, quote_pending, item_marker.take());
                push_span(&mut current, &text, bold, italic, true, link);
            }
            Event::InlineHtml(text) | Event::Html(text) => {
                if let Some(code) = code_block.as_mut() {
                    code.push_str(&text);
                }
            }
            Event::SoftBreak => {
                if code_block.is_some() {
                    if let Some(code) = code_block.as_mut() {
                        code.push('\n');
                    }
                } else {
                    ensure_current(&mut current, quote_depth, quote_pending, item_marker.take());
                    push_span(&mut current, " ", bold, italic, false, link);
                }
            }
            Event::HardBreak => {
                ensure_current(&mut current, quote_depth, quote_pending, item_marker.take());
                push_span(&mut current, "\n", bold, italic, false, link);
            }
            Event::Rule => {
                finish(&mut current, &mut blocks);
                blocks.push(Block::Rule);
            }
            Event::TaskListMarker(checked) => {
                ensure_current(&mut current, quote_depth, quote_pending, item_marker.take());
                push_span(
                    &mut current,
                    if checked { "☑ " } else { "☐ " },
                    bold,
                    italic,
                    false,
                    link,
                );
            }
            _ => {}
        }
    }
    finish(&mut current, &mut blocks);
    if in_image {
        // Malformed image events never leave a dangling layout node.
    }
    blocks
}

fn ensure_current(
    current: &mut Option<Draft>,
    quote_depth: usize,
    quote_pending: bool,
    marker: Option<String>,
) {
    if current.is_none() {
        if quote_depth > 0 || quote_pending {
            *current = Some(Draft {
                kind: DraftKind::Quote,
                marker: None,
                spans: Vec::new(),
            });
        } else {
            *current = Some(Draft {
                kind: if marker.is_some() {
                    DraftKind::ListItem
                } else {
                    DraftKind::Paragraph
                },
                marker,
                spans: Vec::new(),
            });
        }
    }
}

fn push_span(
    current: &mut Option<Draft>,
    text: &str,
    bold: usize,
    italic: usize,
    code: bool,
    link: bool,
) {
    let Some(draft) = current.as_mut() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    if let Some(last) = draft.spans.last_mut()
        && last.bold == (bold > 0)
        && last.italic == (italic > 0)
        && last.code == code
        && last.link == link
    {
        last.text.push_str(text);
    } else {
        draft.spans.push(Span {
            text: text.to_owned(),
            bold: bold > 0,
            italic: italic > 0,
            code,
            link,
        });
    }
}

fn finish(current: &mut Option<Draft>, blocks: &mut Vec<Block>) {
    let Some(draft) = current.take() else {
        return;
    };
    if draft.spans.is_empty() {
        return;
    }
    blocks.push(match draft.kind {
        DraftKind::Heading(level) => Block::Heading(level, draft.spans),
        DraftKind::Paragraph => Block::Paragraph(draft.spans),
        DraftKind::Quote => Block::Quote(draft.spans),
        DraftKind::ListItem => {
            Block::ListItem(draft.marker.unwrap_or_else(|| "•".to_owned()), draft.spans)
        }
    });
}

fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel;
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn render_block(ui: &mut Ui, block: &Block, theme: &crate::Theme, font_size: f32) {
    match block {
        Block::Heading(level, spans) => {
            let size = font_size + (7.0 - f32::from(*level)) * 5.0;
            render_inline(
                ui,
                spans,
                size,
                theme.text,
                FontWeight::SEMIBOLD,
                false,
                theme,
            );
        }
        Block::Paragraph(spans) => {
            render_inline(
                ui,
                spans,
                font_size,
                theme.text_muted,
                FontWeight::NORMAL,
                false,
                theme,
            );
        }
        Block::Quote(spans) => {
            ui.row(|ui| {
                ui.width_full().gap(10.0).align_items(Align::Stretch);
                ui.column(|ui| {
                    ui.width(3.0).height_full().background(theme.accent);
                });
                render_inline(
                    ui,
                    spans,
                    font_size,
                    theme.text_muted,
                    FontWeight::NORMAL,
                    true,
                    theme,
                );
            });
        }
        Block::ListItem(marker, spans) => {
            ui.row(|ui| {
                ui.width_full().gap(8.0);
                ui.add(
                    crate::Label::new(marker.clone())
                        .font_size(font_size)
                        .color(theme.accent)
                        .family(FontFamily::Monospace)
                        .width(24.0),
                );
                render_inline(
                    ui,
                    spans,
                    font_size,
                    theme.text_muted,
                    FontWeight::NORMAL,
                    false,
                    theme,
                );
            });
        }
        Block::Code(code) => {
            ui.column(|ui| {
                ui.width_full()
                    .padding(12.0)
                    .background(theme.background)
                    .border(1.0, theme.border)
                    .radius(theme.radius_small);
                ui.add(
                    crate::Label::new(code.clone())
                        .font_size(font_size - 1.0)
                        .line_height(font_size * 1.45)
                        .family(FontFamily::Monospace)
                        .color(theme.success)
                        .width(Length::Percent(100.0)),
                );
            });
        }
        Block::Rule => {
            ui.column(|ui| {
                ui.width_full().height(1.0).background(theme.border);
            });
        }
    }
}

fn render_inline(
    ui: &mut Ui,
    spans: &[Span],
    size: f32,
    color: kasumi_core::Color,
    weight: FontWeight,
    quote: bool,
    theme: &crate::Theme,
) {
    ui.row(|ui| {
        ui.width_full()
            .wrap(true)
            .gap(0.0)
            .align_items(Align::Center);
        if quote {
            ui.padding_edges(Edges {
                left: 12.0,
                ..Edges::ZERO
            });
        }
        for span in spans {
            let mut label = crate::Label::new(span.text.clone())
                .font_size(if span.code { size - 1.0 } else { size })
                .color(if span.link { theme.accent } else { color })
                .weight(if span.bold {
                    FontWeight::SEMIBOLD
                } else {
                    weight
                })
                .wrap(TextWrap::Word);
            if span.italic {
                label = label.italic();
            }
            if span.code {
                label = label.family(FontFamily::Monospace);
            }
            ui.add(label);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_lists_quotes_and_code() {
        let blocks = parse(
            "# Heading\n\nParagraph with **bold** and `code`.\n\n- one\n- two\n\n> quote\n\n```rust\nlet x = 1;\n```",
        );
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Heading(1, _)))
        );
        assert!(blocks.iter().any(|block| matches!(block, Block::Quote(_))));
        assert!(
            blocks
                .iter()
                .filter(|block| matches!(block, Block::ListItem(_, _)))
                .count()
                >= 2
        );
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Code(code) if code == "let x = 1;"))
        );
    }
}
