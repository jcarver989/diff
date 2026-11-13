use super::MarkdownOutput;
use crate::style;
use clankerdiff_markdown::{
    MarkdownAnchor, MarkdownBlock, MarkdownBlockKind as Block, MarkdownCodeBlock, MarkdownDocument,
    MarkdownInline as Inline, MarkdownListItem, MarkdownTable, MarkdownTableAlignment, MarkdownTargetId,
};
use clankerdiff_ui_core::{MarkdownPalette, ReviewTheme, Rgba};
use rouge_core::{
    Color, Corners, Div, SpanStyle, Styled, Text, TextAlign, Theme, Underline, component, div, px, rich_text, show,
    text, view,
};
use rouge_reactive::{Callback, Memo, ReadSignal};
use rouge_widgets::{CODE_FONT, IconName, UiStyles, icon_button};
use rouge_widgets::{tokens::RADIUS_CONTROL, tokens::RADIUS_INNER, tokens::scaled};
use std::{collections::HashMap, ops::Range, rc::Rc, sync::Arc};

pub(super) const LIST_INDENT: f32 = 22.0;
const BLOCK_PADDING_Y: f32 = 4.0;
const CODE_HEADER_HEIGHT: f32 = 32.0;
const PROSE: Scale = Scale::new(15.0, 24.0);
const CODE: Scale = Scale::new(13.0, 20.0);
const TABLE: Scale = Scale::new(14.0, 20.0);

#[derive(Clone, Copy)]
struct Scale {
    size: f32,
    line_height: f32,
}

impl Scale {
    const fn new(size: f32, line_height: f32) -> Self {
        Self { size, line_height }
    }

    fn apply<T: Styled>(self, styled: T) -> T {
        styled.text_size(self.size).line_height(self.line_height / self.size)
    }
}

const fn heading_scale(level: u8) -> Scale {
    match level {
        1 => Scale::new(26.0, 34.0),
        2 => Scale::new(21.0, 30.0),
        3 => Scale::new(17.0, 26.0),
        _ => PROSE,
    }
}

#[derive(Clone)]
pub(super) struct Paint {
    pub palette: ReadSignal<ReviewTheme>,
    pub clipboard: Memo<bool>,
    pub emit: Callback<(MarkdownOutput,)>,
}

impl Paint {
    fn markdown(&self, pick: fn(&MarkdownPalette) -> Rgba) -> impl Fn() -> Color + 'static {
        let palette = self.palette;
        move || palette.with(|theme| Color::from(pick(&theme.markdown)))
    }
}

pub(super) struct Rows {
    pub document: Arc<MarkdownDocument>,
    entries: Vec<Row>,
    by_target: HashMap<MarkdownTargetId, usize>,
}

pub(super) struct Row {
    pub target: Option<MarkdownTargetId>,
    depth: u16,
    pub gap: f32,
    kind: Kind,
}

enum Kind {
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    Paragraph {
        content: Vec<Inline>,
    },
    ListItem {
        marker: String,
        content: Vec<Inline>,
    },
    Quote {
        rows: Vec<Row>,
    },
    CodeHeader {
        code: Rc<MarkdownCodeBlock>,
    },
    CodeLine {
        code: Rc<MarkdownCodeBlock>,
        index: usize,
    },
    TableRow {
        cells: Vec<Vec<Inline>>,
        alignments: Rc<[MarkdownTableAlignment]>,
        header: bool,
        first: bool,
        last: bool,
    },
    Rule,
    Html {
        content: Vec<Inline>,
    },
}

impl Rows {
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        let entries = document_rows(document.blocks());
        let by_target =
            entries.iter().enumerate().filter_map(|(index, row)| row.target.map(|id| (id, index))).collect();
        Self { document, entries, by_target }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, index: usize) -> Option<&Row> {
        self.entries.get(index)
    }

    pub fn index(&self, id: MarkdownTargetId) -> Option<usize> {
        self.by_target.get(&id).copied()
    }

    pub fn target(&self, index: usize) -> Option<MarkdownTargetId> {
        self.entries.get(index)?.target
    }

    pub fn key(&self, index: usize) -> (Option<MarkdownAnchor>, usize) {
        let anchor = self.target(index).and_then(|id| self.document.anchor_for_target(id));
        let fallback = if anchor.is_some() { 0 } else { index };
        (anchor, fallback)
    }
}

impl Row {
    pub fn indent(&self) -> f32 {
        LIST_INDENT * f32::from(self.depth)
    }

    pub fn first_line_center(&self) -> f32 {
        match &self.kind {
            Kind::Heading { level, .. } => BLOCK_PADDING_Y + heading_scale(*level).line_height / 2.0,
            Kind::ListItem { .. } => 1.0 + PROSE.line_height / 2.0,
            Kind::Quote { rows } => rows.first().map_or(PROSE.line_height / 2.0, Self::first_line_center),
            Kind::CodeHeader { .. } => CODE_HEADER_HEIGHT / 2.0,
            Kind::CodeLine { .. } => CODE.line_height / 2.0,
            Kind::TableRow { .. } => 6.0 + TABLE.line_height / 2.0,
            Kind::Paragraph { .. } | Kind::Html { .. } | Kind::Rule => BLOCK_PADDING_Y + PROSE.line_height / 2.0,
        }
    }

    pub fn background(&self, paint: &Paint) -> Box<dyn Fn() -> Color> {
        match &self.kind {
            Kind::CodeHeader { .. } | Kind::CodeLine { .. } => Box::new(paint.markdown(|m| m.code_background)),
            Kind::TableRow { header: true, .. } => {
                let surface = Theme::current().surface;
                Box::new(move || surface.get())
            }
            _ => Box::new(|| Color::TRANSPARENT),
        }
    }

    pub fn block(&self, paint: &Paint) -> Div {
        let theme = Theme::current();
        match &self.kind {
            Kind::Heading { level, content } => heading_scale(*level)
                .apply(prose())
                .font_weight(600.0)
                .text_color(paint.markdown(|m| m.heading))
                .child(inline_text(content, paint)),
            Kind::Paragraph { content } => view! { <prose>{inline_text(content, paint)}</prose> },
            Kind::ListItem { marker, content } => view! {
                <prose py=1.0 flex row gap=8.0>
                    <div min_w=14.0 flex_none muted>
                        <text>{marker.clone()}</text>
                    </div>
                    <div flex_1 min_w=0.0>{inline_text(content, paint)}</div>
                </prose>
            },
            Kind::Quote { rows } => view! {
                <div px=8.0>
                    <div pl=8.0 border_l=px(2.0) border_color=paint.markdown(|m| m.quote)
                         secondary flex col>
                        {..rows.iter().map(|row| view! {
                            <div pt=row.gap pl=row.indent() flex col>{row.block(paint)}</div>
                        })}
                    </div>
                </div>
            },
            Kind::CodeHeader { code } => code_header(code, paint),
            Kind::CodeLine { code, index } => code_line(code, *index, paint),
            Kind::TableRow { cells, alignments, header, first, last } => {
                table_row(cells, alignments, *header, *first, *last, paint)
            }
            Kind::Rule => view! {
                <div py=12.0><div h=px(1.0) w_full bg=theme.border/></div>
            },
            Kind::Html { content } => view! {
                <prose muted>{inline_text(content, paint).code()}</prose>
            },
        }
    }
}

#[component]
fn prose() -> Div {
    PROSE.apply(div().px(8.0).py(BLOCK_PADDING_Y).rounded(RADIUS_INNER).min_w(0.0))
}

fn corners(top: bool, bottom: bool) -> impl Fn() -> Corners + 'static {
    let radius = scaled(RADIUS_CONTROL);
    move || {
        let radius = radius();
        let (top, bottom) = (if top { radius } else { 0.0 }, if bottom { radius } else { 0.0 });
        Corners { top_left: top, top_right: top, bottom_right: bottom, bottom_left: bottom }
    }
}

fn code_header(code: &Rc<MarkdownCodeBlock>, paint: &Paint) -> Div {
    let empty = code.lines().is_empty();
    let clipboard = paint.clipboard;
    let (code, emit, theme) = (code.clone(), paint.emit, Theme::current());
    let header = view! {
        <div h=CODE_HEADER_HEIGHT pl=16.0 pr=4.0 flex row items_center justify_between border_t=px(1.0)
             border_l=px(1.0) border_r=px(1.0) border_color=theme.border rounded=corners(true, empty)>
            <text caption muted>{code.language().unwrap_or("code").to_owned()}</text>
            <show when={move || clipboard.get()}>
                <icon_button id="markdown-copy-code" icon=IconName::Copy label="Copy code"
                             on_click={
                                 let code = code.clone();
                                 move || {
                                     let text = if code.lines().is_empty() {
                                         String::new()
                                     } else {
                                         format!("{}\n", code.text())
                                     };
                                     emit.call(MarkdownOutput::CopyCode(text));
                                 }
                             }/>
            </show>
        </div>
    };
    if empty { header.border_b(px(1.0)) } else { header }
}

fn code_line(code: &Rc<MarkdownCodeBlock>, index: usize, paint: &Paint) -> Div {
    let last = index + 1 == code.lines().len();
    let source = code.lines().get(index).map_or_else(String::new, |line| code.line_text(line).to_owned());
    let palette = paint.palette;
    let spans = {
        let code = code.clone();
        let source = source.clone();
        move || {
            palette.with(|theme| {
                style::highlights(&source, code.syntax().highlights.line(index).unwrap_or_default(), &theme.syntax)
            })
        }
    };
    let line = view! {
        <div px=16.0 min_h=CODE.line_height overflow_hidden border_l=px(1.0) border_r=px(1.0)
             border_color=Theme::current().border rounded=corners(false, last)>
            <rich_text spans code no_wrap color=paint.markdown(|m| m.code)>{source}</rich_text>
        </div>
    };
    if last { line.pb(8.0).border_b(px(1.0)) } else { line }
}

fn table_row(
    cells: &[Vec<Inline>],
    alignments: &[MarkdownTableAlignment],
    header: bool,
    first: bool,
    last: bool,
    paint: &Paint,
) -> Div {
    let theme = Theme::current();
    let cells = cells.iter().enumerate().map(|(column, cell)| {
        let align = match alignments.get(column) {
            Some(MarkdownTableAlignment::Center) => TextAlign::Center,
            Some(MarkdownTableAlignment::Right) => TextAlign::End,
            _ => TextAlign::Start,
        };
        let cell = view! {
            <div flex_1 min_w=0.0 px=12.0 py=6.0>{inline_text(cell, paint).align(align)}</div>
        };
        if column > 0 { cell.border_l(px(1.0)).border_color(theme.border) } else { cell }
    });
    let row = TABLE.apply(view! {
        <div flex row border_l=px(1.0) border_r=px(1.0) border_b=px(1.0) border_color=theme.border
             rounded=corners(first, last)>
            {..cells}
        </div>
    });
    let row = if header { row.font_weight(600.0) } else { row };
    if first { row.border_t(px(1.0)) } else { row }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Marks(u8);

impl Marks {
    const NONE: Self = Self(0);
    const STRONG: Self = Self(1);
    const EMPHASIS: Self = Self(1 << 1);
    const STRIKETHROUGH: Self = Self(1 << 2);
    const CODE: Self = Self(1 << 3);
    const LINK: Self = Self(1 << 4);
    const IMAGE: Self = Self(1 << 5);

    const fn with(self, mark: Self) -> Self {
        Self(self.0 | mark.0)
    }

    const fn has(self, mark: Self) -> bool {
        self.0 & mark.0 != 0
    }

    fn style(self, markdown: &MarkdownPalette, muted: Color) -> SpanStyle {
        let link = Color::from(markdown.link);
        let foreground = if self.has(Self::LINK) {
            Some(link)
        } else if self.has(Self::CODE) {
            Some(Color::from(markdown.inline_code))
        } else if self.has(Self::IMAGE) {
            Some(muted)
        } else {
            None
        };
        SpanStyle {
            color: foreground,
            background: self.has(Self::CODE).then(|| Color::from(markdown.inline_code_background)),
            weight: self.has(Self::STRONG).then_some(600.0),
            italic: (self.has(Self::EMPHASIS) || self.has(Self::IMAGE)).then_some(true),
            underline: self.has(Self::LINK).then_some(Underline { color: link, thickness: 1.0 }),
            strikethrough: self.has(Self::STRIKETHROUGH).then_some(true),
            family: self.has(Self::CODE).then(|| CODE_FONT.into()),
        }
    }
}

#[derive(Default)]
struct InlineBuilder {
    text: String,
    marks: Vec<(Range<usize>, Marks)>,
    links: Vec<(Range<usize>, String)>,
}

impl InlineBuilder {
    fn push(&mut self, inline: &Inline, marks: Marks) {
        match inline {
            Inline::Text(text) => self.leaf(text, marks),
            Inline::Code(text) => self.leaf(text, marks.with(Marks::CODE)),
            Inline::ImageAlt(text) => self.leaf(text, marks.with(Marks::IMAGE)),
            Inline::SoftBreak => self.leaf(" ", marks),
            Inline::HardBreak => self.leaf("\n", marks),
            Inline::Strong(children) => self.children(children, marks.with(Marks::STRONG)),
            Inline::Emphasis(children) => self.children(children, marks.with(Marks::EMPHASIS)),
            Inline::Strikethrough(children) => self.children(children, marks.with(Marks::STRIKETHROUGH)),
            Inline::Link { destination, content, .. } => {
                let start = self.text.len();
                self.children(content, marks.with(Marks::LINK));
                if self.text.len() > start {
                    self.links.push((start..self.text.len(), destination.clone()));
                }
            }
        }
    }

    fn children(&mut self, children: &[Inline], marks: Marks) {
        for child in children {
            self.push(child, marks);
        }
    }

    fn leaf(&mut self, text: &str, marks: Marks) {
        if text.is_empty() {
            return;
        }
        let range = self.text.len()..self.text.len() + text.len();
        self.text.push_str(text);
        if marks != Marks::NONE {
            self.marks.push((range, marks));
        }
    }
}

fn inline_text(content: &[Inline], paint: &Paint) -> Text {
    let mut builder = InlineBuilder::default();
    builder.children(content, Marks::NONE);
    let InlineBuilder { text, marks, links } = builder;
    let (palette, muted) = (paint.palette, Theme::current().text_muted);
    let spans = move || {
        let muted = muted.get();
        palette.with(|theme| {
            marks.iter().map(|(range, mark)| (range.clone(), mark.style(&theme.markdown, muted))).collect()
        })
    };
    let text = rich_text(text, spans);
    let links: Vec<_> = links.into_iter().filter(|(_, url)| openable(url)).collect();
    if links.is_empty() {
        return text;
    }
    let emit = paint.emit;
    text.on_range_click(move |index| {
        if let Some((_, url)) = index.and_then(|index| links.iter().find(|(range, _)| range.contains(&index))) {
            emit.call(MarkdownOutput::OpenLink(url.clone()));
        }
    })
}

fn openable(url: &str) -> bool {
    ["https://", "http://", "mailto:"].iter().any(|scheme| url.starts_with(scheme))
}

pub(super) fn document_rows(blocks: &[MarkdownBlock]) -> Vec<Row> {
    let mut rows = Vec::new();
    for block in blocks {
        push_block(block, 0, &mut rows);
    }
    for index in 1..rows.len() {
        rows[index].gap = gap(&rows[index - 1].kind, &rows[index]);
    }
    rows
}

fn push_block(block: &MarkdownBlock, depth: u16, rows: &mut Vec<Row>) {
    let kind = match &block.kind {
        Block::Heading { level, content } => Kind::Heading { level: *level, content: content.clone() },
        Block::Paragraph { content } => Kind::Paragraph { content: content.clone() },
        Block::BlockQuote { blocks } => Kind::Quote { rows: document_rows(blocks) },
        Block::HtmlFallback { content } => Kind::Html { content: content.clone() },
        Block::Rule => Kind::Rule,
        Block::List { ordered, start, items } => return push_list(*ordered, *start, items, depth, rows),
        Block::CodeBlock(code) => return push_code(code, depth, rows),
        Block::Table(table) => return push_table(table, depth, rows),
    };
    rows.push(Row { target: block.target_id, depth, gap: 0.0, kind });
}

fn push_list(ordered: bool, start: Option<u64>, items: &[MarkdownListItem], depth: u16, rows: &mut Vec<Row>) {
    let first = start.unwrap_or(1);
    for (offset, item) in (0_u64..).zip(items) {
        let marker = if ordered { format!("{}.", first + offset) } else { bullet(depth).to_owned() };
        rows.push(Row {
            target: item.target_id,
            depth,
            gap: 0.0,
            kind: Kind::ListItem { marker, content: item.content.clone() },
        });
        for block in &item.blocks {
            push_block(block, depth.saturating_add(1), rows);
        }
    }
}

fn push_code(code: &MarkdownCodeBlock, depth: u16, rows: &mut Vec<Row>) {
    let code = Rc::new(code.clone());
    rows.push(Row { target: code.target_id, depth, gap: 0.0, kind: Kind::CodeHeader { code: code.clone() } });
    rows.extend(code.lines().iter().map(|line| Row {
        target: line.target_id,
        depth,
        gap: 0.0,
        kind: Kind::CodeLine { code: code.clone(), index: line.index },
    }));
}

fn push_table(table: &MarkdownTable, depth: u16, rows: &mut Vec<Row>) {
    let alignments: Rc<[MarkdownTableAlignment]> = Rc::from(table.alignments.as_slice());
    let count = table.rows.len();
    rows.extend(table.rows.iter().enumerate().map(|(index, row)| Row {
        target: row.target_id,
        depth,
        gap: 0.0,
        kind: Kind::TableRow {
            cells: row.cells.iter().map(|cell| cell.content.clone()).collect(),
            alignments: alignments.clone(),
            header: row.header,
            first: index == 0,
            last: index + 1 == count,
        },
    }));
}

fn gap(previous: &Kind, row: &Row) -> f32 {
    match (&row.kind, previous) {
        (Kind::CodeLine { .. } | Kind::TableRow { first: false, .. }, _) => 0.0,
        (Kind::Heading { level: 1, .. }, _) => 36.0,
        (Kind::Heading { level: 2, .. }, _) => 32.0,
        (Kind::Heading { .. }, _) => 24.0,
        (_, Kind::Heading { .. }) => 8.0,
        (Kind::ListItem { .. }, Kind::ListItem { .. }) => 4.0,
        (_, _) if row.depth > 0 => 6.0,
        (Kind::Rule, _) => 20.0,
        _ => 14.0,
    }
}

const fn bullet(depth: u16) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}
