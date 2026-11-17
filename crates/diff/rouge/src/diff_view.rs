use crate::{
    ReviewHandle, ScrollRequest, ViewerPane, annotation, comment_editor, comments, definition_card, review_bar,
    shortcut_label, sidebar, style, toolbar, viewer::Viewport,
};
use clankerdiff_core::{
    DiffReviewCommand as C, DiffSide, DiffTone, FileStatus, FileView, Layout, LineAnchor, LineStat, PresentedCell,
    PresentedRow, RevealAmount, RowId, RowKind, SourceLocation, tooltip,
};
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ui_core::{DiffPalette, ReviewTheme, Rgba};
use rouge_core::{
    Align, AnyView, Canvas, Color, Cursor, Div, Measure, Painter, Point, Quad, Rect, Role, RowKey, ScrollLinked, Size,
    SpanStyle, Styled, TextHit, TextRun, Theme, Underline, View, VirtualListHandle, canvas, div, dynamic, modifiers,
    pct, px, show, text, view, virtual_list, virtual_list_handle,
};
use rouge_reactive::{Memo, ReadSignal, Signal, effect, memo, on_cleanup, signal};
use rouge_widgets::{
    CODE_WRAP, Icon, IconName, SegmentedControl, UiStyles, empty_state, icon_button, markdown, segment,
    segmented_control, syntax_spans,
    tokens::{self, Radius},
    typography::CODE,
    vertical_scrollbar,
};
use std::{cell::Cell, ops::Range, rc::Rc};

const CODE_HEIGHT: f32 = 20.0;
const HEADER_HEIGHT: f32 = 34.0;
const STATUS_CHIP: f32 = 18.0;
const COMMENT_COLUMN: f32 = 40.0;
const ADDED_COLUMN: f32 = 64.0;
const DELETED_COLUMN: f32 = 40.0;
const STAT_INSET: f32 = 16.0;
const ACCENT_WIDTH: f32 = 4.0;
const GUTTER_WIDTH: f32 = 52.0;
const GUTTER_PADDING: f32 = 8.0;
const TEXT_TOP: f32 = 0.0;
const META_PADDING: f32 = 12.0;
const CODE_PADDING_RIGHT: f32 = 8.0;
/// Between side-by-side columns.
const DIVIDER: f32 = 1.0;
const MAX_LINE_BYTES: usize = 16 * 1024;
const CONTINUATION: &str = "↪";
const PREVIEW_WIDTH: f32 = 780.0;
const PREVIEW_PADDING: f32 = 24.0;

#[derive(Clone, Copy)]
struct Flow {
    wrap: Memo<bool>,
    offset: Signal<f32>,
    limit: Signal<f32>,
}

type Hover = Signal<Option<(usize, DiffSide)>>;

pub(crate) fn content(handle: ReviewHandle) -> impl View {
    let diff = virtual_list_handle();
    let files = virtual_list_handle();
    comment_editor::install(&handle);
    let status = handle.status();
    let shown = memo(move || !status.get().sidebar_collapsed);
    let review_bar = handle.with(|state| state.options().review_bar);
    let bg = handle.theme();
    view! {
        <div size_full flex col bg={move || bg.with(|t| Color::from(t.diff.background))}>
            {toolbar::toolbar(&handle)}
            <div flex row flex_1 min_h=0.0 overflow_hidden>
                <show when={move || shown.get()}>{sidebar::sidebar(handle, files)}</show>
                {diff_pane(handle, diff, files)}
            </div>
            {review_bar.then(|| review_bar::review_bar(handle))}
        </div>
    }
}

fn diff_pane(handle: ReviewHandle, list: VirtualListHandle, files: VirtualListHandle) -> Div {
    let revision = handle.presentation_revision();
    let range = memo(move || {
        revision.get();
        handle.with(|state| state.session().visible_range())
    });
    let visible = signal(0..0);
    let status = handle.status();
    let wrap = memo(move || status.get().wrap);
    let flow = Flow { wrap, offset: signal(0.0), limit: signal(0.0) };
    let hover: Hover = signal(None);
    let anchor = signal(Rect::ZERO);
    effect(move || {
        revision.get();
        wrap.get();
        flow.offset.set_if_changed(0.0);
        flow.limit.set_if_changed(0.0);
    });
    bind_navigation(&handle, list, files, range, visible);
    let rows = virtual_list(
        move || range.get().len(),
        CODE_HEIGHT,
        move |index| row(&handle, range.get_untracked().start + index, flow, hover, anchor),
    )
    .row_identity(move |index| {
        let revision = revision.get();
        let index = range.get().start + index;
        RowKey::new(
            handle.with(|state| state.session().presentation().row(index).map_or(RowId(0), |row| row.id)),
            revision,
        )
    })
    .measure_rows(move || u64::from(wrap.get()))
    .track_rows(visible)
    .handle(list)
    .size_full();
    let active = memo(move || {
        revision.get();
        let top = range.get().start + visible.with(|rows| rows.start);
        handle.with(|state| state.session().presentation().row(top).map(|row| row.file_index))
    });
    effect(move || {
        if let Some(file) = active.get() {
            handle.follow_visible_file(file);
        }
    });
    let empty = move || {
        revision.get();
        range.get().is_empty().then(|| {
            let detail = handle.with(|state| format!("Nothing to review in {}", state.session().target()));
            view! {
                <div absolute inset=0.0 flex items_center justify_center>
                    <empty_state icon=IconName::FileDiff title="No changes" detail/>
                </div>
            }
        })
    };
    view! {
        <div test_id="diff-pane" relative flex_1 min_w=0.0 min_h=0.0 overflow_hidden
             on_pointer_down={move |event| {
                 if !definition_card::links(event.modifiers) && handle.with(|s| s.definition_card().is_some()) {
                     handle.handle_command(C::CloseDefinition);
                 }
             }}
             on_scroll={move |event| {
                 if event.delta.x != 0.0 && !flow.wrap.get_untracked() {
                     let max = flow.limit.get_untracked();
                     flow.offset.update(|x| *x = (*x - event.delta.x).clamp(0.0, max));
                 }
             }}>
            {rows}
            <dynamic children=empty/>
            {sticky(handle, list, active, range)}
            {definition_card::popover(handle, anchor)}
            <div absolute right=0.0 top=0.0 bottom=0.0>{vertical_scrollbar(list.scroll_state())}</div>
        </div>
    }
}

fn bind_navigation(
    handle: &ReviewHandle,
    list: VirtualListHandle,
    files: VirtualListHandle,
    range: Memo<Range<usize>>,
    visible: Signal<Range<usize>>,
) {
    handle.bind_viewport(Viewport { diff: list, files, visible });
    let requests = handle.reveal_row();
    effect(move || {
        if let Some((row, align)) = requests.get() {
            let range = range.get_untracked();
            if range.contains(&row) {
                let index = row - range.start;
                if align == Align::Visible {
                    list.reveal_row(index, HEADER_HEIGHT);
                } else {
                    list.scroll_to_row(index, align);
                }
            }
        }
    });
}

impl Viewport {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Page sizes are bounded viewport heights divided by a row height"
    )]
    pub(crate) fn page_rows(&self, pane: ViewerPane) -> isize {
        let rows = if pane == ViewerPane::Diff {
            self.visible.with_untracked(Range::len)
        } else {
            let height = self.files.scroll_state().try_get().map_or(0.0, |state| state.viewport.height);
            (height / CODE_HEIGHT).floor() as usize
        };
        isize::try_from(rows.max(1)).unwrap_or(isize::MAX)
    }

    #[expect(clippy::cast_precision_loss, reason = "Scroll requests are a handful of lines")]
    pub(crate) fn scroll(&self, request: ScrollRequest) {
        let list = if request.pane == ViewerPane::Diff { self.diff } else { self.files };
        let Some(state) = list.scroll_state().try_get() else {
            return;
        };
        let offset = state.offset.y + request.lines as f32 * CODE_HEIGHT;
        list.scroll_to(state.clamp(Point::new(state.offset.x, offset)));
    }
}

fn sticky(
    handle: ReviewHandle,
    list: VirtualListHandle,
    active: Memo<Option<usize>>,
    range: Memo<Range<usize>>,
) -> impl View {
    let revision = handle.presentation_revision();
    dynamic(move || {
        revision.get();
        let file = active.get()?;
        let span = handle.with(|state| state.session().presentation().file_range(file))?;
        let start = range.get().start;
        let link = move || ScrollLinked::Sticky {
            lo: list.row_top(span.start.saturating_sub(start)),
            hi: list.row_top(span.end.saturating_sub(start)) - HEADER_HEIGHT,
        };
        Some(view! {
            <div test_id="sticky-file-header" absolute top=0.0 left=0.0 right=0.0
                 scroll_linked=(list.scroll_state(), link)>
                {header(&handle, file)}
            </div>
        })
    })
}

fn row(handle: &ReviewHandle, index: usize, flow: Flow, hover: Hover, anchor: Signal<Rect>) -> AnyView {
    let Some(row) = handle.with(|s| s.session().presentation().row(index).cloned()) else {
        return div().into_any();
    };
    match row.kind {
        RowKind::FileHeader => header(handle, row.file_index).into_any(),
        RowKind::Meta => meta(index, &row).into_any(),
        RowKind::ExpandGap => gap(handle, index).into_any(),
        RowKind::MarkdownPreview => markdown_preview(handle, row.file_index).into_any(),
        RowKind::Code | RowKind::ExpandedContext => line(handle, index, &row, flow, hover, anchor).into_any(),
    }
}

fn meta(index: usize, row: &PresentedRow) -> Div {
    let theme = Theme::current();
    let content = row.primary_cell().map_or_else(String::new, |cell| cell.text.to_string());
    let label = canvas(move |painter| {
        let style = CODE.text_style();
        let size = painter.size();
        let origin = Point::new(META_PADDING, TEXT_TOP);
        let clip = Rect::new(0.0, 0.0, size.width, size.height);
        painter.run(TextRun::new(content.clone(), origin, &style, theme.get().text_muted).clip(clip));
    });
    view! {
        <div test_id={format!("diff-meta-{index}")} min_h=CODE_HEIGHT>{label.w_full().h(CODE_HEIGHT)}</div>
    }
}

struct Column {
    side: DiffSide,
    cell: Option<PresentedCell>,
    location: Option<SourceLocation>,
    anchor: Option<LineAnchor>,
}

impl Column {
    fn all(handle: &ReviewHandle, row: &PresentedRow) -> Rc<[Column]> {
        let split = handle.with(|s| s.session().presentation().file_layout(row.file_index)) == Layout::Split;
        let sides = if split {
            vec![DiffSide::Old, DiffSide::New]
        } else if row.right.is_some() {
            vec![DiffSide::New]
        } else {
            vec![DiffSide::Old]
        };
        sides
            .into_iter()
            .map(|side| {
                let cell = row.cell(side).cloned();
                let (location, anchor) = cell.as_ref().map_or((None, None), |cell| {
                    handle.with(|s| {
                        let presentation = s.session().presentation();
                        (presentation.source_location(row, cell), presentation.cell_anchor(row, cell))
                    })
                });
                Column { side, cell, location, anchor }
            })
            .collect()
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Column counts are one or two and positions are bounded by the row width"
    )]
    fn at(columns: &[Column], width: f32, x: f32) -> Option<&Column> {
        let each = width / columns.len().max(1) as f32;
        let position = (x / each.max(1.0)).floor().max(0.0) as usize;
        columns.get(position.min(columns.len().saturating_sub(1)))
    }

    fn location(columns: &[Column], hit: TextHit) -> Option<SourceLocation> {
        let column = columns.iter().find(|column| column.side as u32 == hit.tag)?;
        let shown = column.cell.as_ref().map_or(0, |cell| shown_len(cell.text.as_ref()));
        column.location.clone().filter(|_| hit.index < shown)
    }
}

fn line(
    handle: &ReviewHandle,
    index: usize,
    row: &PresentedRow,
    flow: Flow,
    hover: Hover,
    anchor: Signal<Rect>,
) -> Div {
    let handle = *handle;
    let columns = Column::all(&handle, row);
    let size = signal(Size::ZERO);
    let overflow = signal(0.0_f32);
    effect(move || {
        let width = overflow.get();
        if width > flow.limit.get_untracked() {
            flow.limit.set(width);
        }
    });
    let code_height = Rc::new(Cell::new(0.0_f32));
    let painted = painted(&handle, index, row.clone(), &columns, flow, code_height.clone(), anchor)
        .track_overflow(overflow)
        .on_text_hover({
            let columns = columns.clone();
            let hovered = signal(None::<Range<usize>>);
            let last = Cell::new(None::<TextHit>);
            move |hit, modifiers| {
                let hit = hit.filter(|hit| Column::location(&columns, *hit).is_some());
                if let Some(previous) = last.replace(hit).filter(|previous| Some(previous.tag) != hit.map(|h| h.tag))
                    && let Some(location) = Column::location(&columns, previous)
                {
                    definition_card::hover(&handle, &location, None, modifiers, hovered);
                }
                if let Some(hit) = hit
                    && let Some(location) = Column::location(&columns, hit)
                {
                    definition_card::hover(&handle, &location, Some(hit.index), modifiers, hovered);
                }
            }
        })
        .on_text_click({
            let columns = columns.clone();
            move |hit, modifiers| {
                if let Some(hit) = hit
                    && let Some(location) = Column::location(&columns, hit)
                {
                    definition_card::click(&handle, index, &location, hit, modifiers);
                }
            }
        })
        .element({
            let columns = columns.clone();
            move |div| {
                div.on_click(move |event| {
                    if definition_card::links(event.modifiers) {
                        return;
                    }
                    let width = size.get_untracked().width;
                    if let Some(column) =
                        Column::at(&columns, width, event.local().x).filter(|column| column.cell.is_some())
                    {
                        handle.select_cell(index, column.side);
                    }
                })
            }
        })
        .w_full();
    let pointer = columns.clone();
    view! {
        <div test_id={format!("diff-row-{index}")} layer flex col min_h=CODE_HEIGHT relative track_size=size
             on_pointer_move={move |event| {
                 let point = event.local();
                 let target = (point.y < code_height.get())
                     .then(|| Column::at(&pointer, size.get_untracked().width, point.x))
                     .flatten()
                     .filter(|column| column.anchor.is_some())
                     .map(|column| (index, column.side));
                 hover.set_if_changed(target);
             }}
             on_hover={move |inside| {
                 if !inside && hover.get_untracked().is_some_and(|(row, _)| row == index) {
                     hover.set(None);
                 }
             }}>
            {painted}
            {add_comment(&handle, index, &columns, hover)}
            {blocks(&handle, index, columns)}
        </div>
    }
}

type Spans = Vec<(Range<usize>, SpanStyle)>;

#[derive(PartialEq)]
struct Styling {
    spans: Spans,
    card: Option<Range<usize>>,
}

#[derive(Default)]
struct Marks {
    card: Option<Range<usize>>,
    references: Vec<Range<usize>>,
}

impl Marks {
    fn at(handle: &ReviewHandle, location: Option<&SourceLocation>, links: bool) -> Self {
        let Some(location) = location else {
            return Self::default();
        };
        let card = handle
            .with(|s| s.definition_card().filter(|card| card.origin == *location).map(|card| card.columns.clone()));
        let references = if links {
            handle.with_syntax(|_, syntax| {
                syntax.ready_symbols().map_or_else(Vec::new, |index| {
                    index.references_at(location).iter().map(|reference| reference.span.columns.clone()).collect()
                })
            })
        } else {
            Vec::new()
        };
        Self { card, references }
    }

    fn spans(&self, underline: Color, carded: Color) -> impl Iterator<Item = (Range<usize>, SpanStyle)> + '_ {
        let underline = Underline { color: underline, thickness: 1.0 };
        let references = self
            .references
            .iter()
            .map(move |range| (range.clone(), SpanStyle { underline: Some(underline), ..SpanStyle::default() }));
        let card = self
            .card
            .iter()
            .map(move |range| (range.clone(), SpanStyle { background: Some(carded), ..SpanStyle::default() }));
        card.chain(references)
    }
}

struct Fill {
    background: Color,
    accent: Option<Color>,
}

#[derive(Clone, Copy)]
struct Ink {
    number: Color,
    text: Color,
    border: Color,
}

#[derive(Clone, Copy)]
struct Geometry {
    width: f32,
    count: usize,
    wrap: bool,
    offset: f32,
}

impl Geometry {
    #[expect(clippy::cast_precision_loss, reason = "Column counts are one or two")]
    fn column(&self, position: usize) -> Rect {
        let width = self.width / self.count.max(1) as f32;
        let last = position + 1 >= self.count;
        Rect::new(position as f32 * width, 0.0, if last { width } else { width - DIVIDER }, 0.0)
    }

    fn code_left(&self, position: usize) -> f32 {
        self.column(position).origin.x + ACCENT_WIDTH + GUTTER_WIDTH
    }

    fn number_edge(&self, position: usize) -> f32 {
        self.column(position).origin.x + ACCENT_WIDTH + GUTTER_WIDTH - GUTTER_PADDING
    }

    fn code_run(&self, position: usize, column: &Column, cell: &PresentedCell, spans: &Spans, ink: Ink) -> TextRun {
        let side = column.side;
        let column = self.column(position);
        let left = self.code_left(position);
        let mut style = CODE.text_style();
        let (content, spans) = displayed(cell.text.as_ref(), spans, ink.number);
        style.spans = spans;
        style.wrap_indent = CODE_WRAP;
        let top = TEXT_TOP;
        let run = if self.wrap {
            let available = column.max().x - left - CODE_PADDING_RIGHT;
            TextRun::new(content, Point::new(left, top), &style, ink.text).wrap(available).continuation(
                CONTINUATION,
                self.number_edge(position),
                ink.number.with_alpha(0.6),
            )
        } else {
            TextRun::new(content, Point::new(left - self.offset, top), &style, ink.text)
        };
        run.tag(side as u32)
    }

    fn height(&self, measure: &mut Measure<'_>, columns: &[Column], styled: &[Styling], ink: Ink) -> f32 {
        let line = CODE_HEIGHT;
        if !self.wrap {
            return line;
        }
        columns
            .iter()
            .zip(styled)
            .enumerate()
            .filter_map(|(position, (column, styling))| {
                let cell = column.cell.as_ref()?;
                let run = self.code_run(position, column, cell, &styling.spans, ink);
                Some(measure.text(&run).max().y + TEXT_TOP)
            })
            .fold(line, f32::max)
            .ceil()
    }
}

fn shown_len(text: &str) -> usize {
    if text.len() <= MAX_LINE_BYTES {
        return text.len();
    }
    (0..=MAX_LINE_BYTES).rev().find(|index| text.is_char_boundary(*index)).unwrap_or(0)
}

fn displayed(text: &str, spans: &Spans, muted: Color) -> (String, Spans) {
    let shown = shown_len(text);
    if shown == text.len() {
        return (text.to_owned(), spans.clone());
    }
    let hidden = text[shown..].chars().count();
    let mut content = text[..shown].to_owned();
    let suffix = format!(" … {hidden} more characters");
    let suffix_range = content.len()..content.len() + suffix.len();
    content.push_str(&suffix);
    let mut clipped: Spans = spans
        .iter()
        .filter(|(range, _)| range.start < shown)
        .map(|(range, style)| (range.start..range.end.min(shown), style.clone()))
        .collect();
    clipped.push((suffix_range, SpanStyle { color: Some(muted), italic: Some(true), ..SpanStyle::default() }));
    (content, clipped)
}

fn painted(
    handle: &ReviewHandle,
    index: usize,
    row: PresentedRow,
    columns: &Rc<[Column]>,
    flow: Flow,
    code_height: Rc<Cell<f32>>,
    anchor: Signal<Rect>,
) -> Canvas {
    let theme = Theme::current();
    let selection = handle.selection();
    let selected = memo(move || {
        let selection = selection.get();
        (selection.row == Some(index)).then_some(selection.side)
    });
    let (handle, columns) = (*handle, columns.clone());
    let palette = handle.theme();
    let revision = handle.syntax_revision();
    let definition = handle.definition_revision();
    let held = modifiers();
    let styled = memo({
        let columns = columns.clone();
        move || {
            revision.get();
            definition.get();
            let links = definition_card::links(held.get());
            palette.with(|review| {
                let (underline, carded) = (Color::from(review.diff.accent), Color::from(review.carded_name()));
                columns
                    .iter()
                    .map(|column| {
                        let mut spans = column.cell.as_ref().map_or_else(Vec::new, |cell| {
                            handle.with_syntax(|_, syntax| {
                                syntax_spans(syntax.ready_highlights(&row, cell), &review.syntax)
                            })
                        });
                        let marks = Marks::at(&handle, column.location.as_ref(), links);
                        spans.extend(marks.spans(underline, carded));
                        Styling { spans, card: marks.card }
                    })
                    .collect::<Rc<[Styling]>>()
            })
        }
    });
    let measured = columns.clone();
    let anchored = Rc::new(Cell::new(false));
    on_cleanup({
        let anchored = anchored.clone();
        move || {
            if anchored.get() {
                anchor.set(Rect::ZERO);
            }
        }
    });
    canvas(move |painter| {
        let colors = theme.get();
        let selected = selected.get();
        let styled = styled.get();
        anchored.set(styled.iter().any(|styling| styling.card.is_some()));
        let wrap = flow.wrap.get();
        let offset = if wrap { 0.0 } else { flow.offset.get() };
        let size = painter.size();
        code_height.set(size.height);
        let geometry = Geometry { width: size.width, count: columns.len(), wrap, offset };
        let ink = Ink { number: colors.text_muted, text: colors.text, border: colors.border };
        palette.with(|review| {
            let fill = |column: &Column, cell: &PresentedCell| Fill {
                background: Color::from(if selected == Some(column.side) {
                    review.diff.selection
                } else {
                    review.diff.tone(cell.tone).background
                }),
                accent: match cell.tone {
                    DiffTone::Added => Some(Color::from(review.diff.addition)),
                    DiffTone::Removed => Some(Color::from(review.diff.deletion)),
                    _ => None,
                },
            };
            paint_line(painter, index, &columns, &styled, geometry, ink, fill, anchor);
        });
    })
    .measure(move |measure, width| {
        let colors = theme.get();
        let wrap = flow.wrap.get();
        let geometry = Geometry { width, count: measured.len(), wrap, offset: 0.0 };
        let ink = Ink { number: colors.text_muted, text: colors.text, border: colors.border };
        geometry.height(measure, &measured, &styled.get(), ink)
    })
}

#[expect(clippy::too_many_arguments, reason = "Painting a line needs its geometry, colours and card anchor")]
fn paint_line(
    painter: &mut Painter,
    index: usize,
    columns: &[Column],
    styled: &[Styling],
    geometry: Geometry,
    ink: Ink,
    fill: impl Fn(&Column, &PresentedCell) -> Fill,
    anchor: Signal<Rect>,
) {
    let bounds = painter.size();
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let style = CODE.text_style();
    for (position, (column, styling)) in columns.iter().zip(styled).enumerate() {
        let inner =
            Rect { size: Size::new(geometry.column(position).size.width, bounds.height), ..geometry.column(position) };
        let left = inner.origin.x;
        let side = column.side;
        if position + 1 < columns.len() {
            painter.quad(Quad::line(Rect::new(inner.max().x, 0.0, DIVIDER, bounds.height), ink.border));
        }
        let Some(cell) = &column.cell else {
            painter.region(format!("empty-{index}-{side:?}"), inner);
            continue;
        };
        painter.region(format!("diff-cell-{index}-{side:?}"), inner);
        let Fill { background, accent } = fill(column, cell);
        painter.fill_rect(inner, background);
        if let Some(accent) = accent {
            painter.fill_rect(Rect::new(left, 0.0, ACCENT_WIDTH, bounds.height), accent);
        }
        if column.anchor.is_some() {
            let button = annotation::ADD_COMMENT_SIZE;
            painter
                .region(format!("add-comment-{index}-{side:?}"), Rect::new(left + ACCENT_WIDTH, 0.0, button, button));
        }
        if let Some(line) = cell.line_number() {
            painter.run(
                TextRun::new(
                    line.to_string(),
                    Point::new(geometry.number_edge(position), TEXT_TOP),
                    &style,
                    ink.number,
                )
                .right_aligned(),
            );
        }
        let code_left = geometry.code_left(position);
        let clip = Rect::new(code_left, 0.0, (inner.max().x - code_left).max(0.0), bounds.height);
        let run = geometry
            .code_run(position, column, cell, &styling.spans, ink)
            .clip(clip)
            .name(format!("code-{index}-{side:?}"));
        let run = match styling.card.clone() {
            Some(range) => run.track_range(range, anchor),
            None => run,
        };
        painter.run(run);
    }
}

#[expect(clippy::cast_precision_loss, reason = "Column counts are one or two")]
fn add_comment(handle: &ReviewHandle, index: usize, columns: &Rc<[Column]>, hover: Hover) -> impl View {
    let hovered = memo(move || hover.get().filter(|(row, _)| *row == index).map(|(_, side)| side));
    let (handle, columns) = (*handle, columns.clone());
    dynamic(move || {
        let side = hovered.get()?;
        let position = columns.iter().position(|column| column.side == side && column.anchor.is_some())?;
        Some(view! {
            <div absolute top=0.0 left=pct(100.0 * position as f32 / columns.len() as f32)>
                <div ml=ACCENT_WIDTH><annotation::add_comment handle row=index side/></div>
            </div>
        })
    })
}

fn blocks(handle: &ReviewHandle, index: usize, columns: Rc<[Column]>) -> impl View {
    let handle = *handle;
    let comments = handle.comments_revision();
    let draft = handle.draft_placement();
    let shown = memo({
        let columns = columns.clone();
        move || {
            comments.get();
            let draft = draft.get();
            handle.with(|state| {
                columns
                    .iter()
                    .map(|column| {
                        column.anchor.as_ref().is_some_and(|anchor| {
                            draft.as_ref().is_some_and(|(placed, _)| placed == anchor)
                                || state.session().review().comments_for_anchor(anchor).any(|comment| !comment.outdated)
                        })
                    })
                    .collect::<Vec<_>>()
            })
        }
    });
    dynamic(move || {
        let shown = shown.get();
        if shown.iter().all(|thread| !thread) {
            return None;
        }
        let cells = columns.iter().zip(shown).map(|(column, thread)| {
            let thread = column.anchor.clone().filter(|_| thread);
            view! {
                <div flex_1 min_w=0.0 flex col>
                    {thread.map(|anchor| comments::thread(handle, index, column.side, anchor))}
                </div>
            }
        });
        Some(view! { <div flex row>{..cells}</div> })
    })
}

struct HeaderInfo {
    status: FileStatus,
    name: String,
    directory: Option<String>,
    renamed_from: Option<String>,
    stat: LineStat,
    view: FileView,
    previewable: bool,
    source: Option<&'static str>,
    start: Option<usize>,
    after_header: bool,
}

fn header(handle: &ReviewHandle, index: usize) -> Div {
    let Some(info) = header_info(handle, index) else {
        return div();
    };
    let handle = *handle;
    let palette = handle.theme();
    let tone = move |pick: fn(&DiffPalette) -> Rgba| move || palette.with(|t| Color::from(pick(&t.diff)));
    let selection = handle.selection();
    let start = info.start;
    let count = collapsed_comments(&handle, index);
    let (status, view) = (info.status, info.view);
    let collapsed = view == FileView::Collapsed;
    let (icon, label) =
        if collapsed { (IconName::ChevronRight, "Expand file") } else { (IconName::ChevronDown, "Collapse file") };
    let toggle = info.source.is_none().then(|| {
        view! {
            <icon_button test_id={format!("collapse-file-{index}")} icon label
                         on_click={move || {
                             handle.handle_command(C::ToggleFileCollapsed(Some(index)));
                         }}/>
        }
    });
    let selected = memo(move || {
        let selection = selection.get();
        selection.pane == ViewerPane::Diff && start.is_some() && selection.row == start
    });
    let comments = memo(move || if view == FileView::Diff { 0 } else { count() });
    let status_color = move || palette.with(|t| style::file_status_color(status, &t.diff));
    let band = move |pick: fn(&DiffPalette) -> Rgba| {
        move || {
            palette.with(|t| {
                let p = &t.diff;
                Color::from(if selected.get() { p.accent.mix(p.background, 26) } else { pick(p) })
            })
        }
    };
    let rule = move || {
        palette.with(|t| {
            let p = &t.diff;
            Color::from(if selected.get() { p.accent.mix(p.background, 115) } else { p.border })
        })
    };
    let trailing = match info.source {
        Some(source) => view! {
            <div test_id="source-mode-header" flex_none pr=STAT_INSET>
                <text body no_wrap color=tone(|p| p.muted)>{source}</text>
            </div>
        },
        None => stat_columns(info.stat, palette, comments),
    };
    let preview =
        (info.previewable && info.source.is_none()).then(|| preview_toggle(&handle, index, view == FileView::Preview));
    view! {
        <div test_id={format!("diff-file-header-{index}")} h=HEADER_HEIGHT w_full flex row items_center
             border_t=px(if info.after_header { 0.0 } else { 1.0 }) border_b=px(1.0) border_color=rule
             bg=band(|p| p.foreground.mix(p.background, 10)) cursor=Cursor::Pointer overflow_hidden
             on_click={move |_| {
                 handle.handle_command(C::SelectFile(index));
             }}>
            <div w=ACCENT_WIDTH h_full flex_none bg={move || Color::from(status_color())}/>
            <div w=GUTTER_WIDTH flex_none flex justify_center>{toggle}</div>
            <div flex row flex_1 min_w=0.0 items_center gap=10.0 overflow_hidden>
                <div size=STATUS_CHIP flex_none flex items_center justify_center rounded=Radius::Inner
                     bg={move || palette.with(|t| Color::from(status_color().mix(t.diff.background, 36)))}>
                    <text caption font_weight=700.0 color={move || Color::from(status_color())}>
                        {status.code().to_string()}
                    </text>
                </div>
                <div flex row min_w=0.0 overflow_hidden>
                    {info.renamed_from.map(|old| view! { <text body no_wrap color=tone(|p| p.muted)>{old}</text> })}
                    {info.directory.map(|directory| view! {
                        <text body no_wrap color=tone(|p| p.muted)>{directory}</text>
                    })}
                    <text body_strong no_wrap color=tone(|p| p.foreground)>{info.name}</text>
                </div>
            </div>
            {preview}
            {trailing}
        </div>
    }
    .hover(move |style| style.bg(band(|p| p.foreground.mix(p.background, 18))))
}

fn preview_toggle(handle: &ReviewHandle, index: usize, previewed: bool) -> SegmentedControl {
    let handle = *handle;
    let segments = [
        (false, "show-diff", IconName::FileDiff, "Show diff"),
        (true, "preview-markdown", IconName::Eye, "Preview markdown"),
    ]
    .map(|(preview, id, icon, text)| {
        let label = tooltip(text, shortcut_label(&C::ToggleMarkdownPreview(None)));
        view! {
            <segment test_id={format!("{id}-{index}")} label icon=icon selected={previewed == preview}
                     on_click={move || {
                         if previewed != preview {
                             handle.handle_command(C::ToggleMarkdownPreview(Some(index)));
                         }
                     }}/>
        }
    });
    view! { <segmented_control test_id={format!("markdown-toggle-{index}")} flex_none p=2.0>{..segments}</segmented_control> }
}

fn markdown_preview(handle: &ReviewHandle, file: usize) -> Div {
    let source = handle.with(|s| {
        s.session().presentation().document().files.get(file).and_then(|file| file.markdown_source().cloned())
    });
    let Some(source) = source else {
        return div();
    };
    let (handle, theme) = (*handle, handle.theme());
    let document = markdown(MarkdownDocument::parse(source.text()))
        .palette(theme.select(|t| t.markdown))
        .syntax(theme.select(|t| t.syntax.clone()))
        .on_link(move |url| handle.open_link(url));
    view! {
        <div test_id={format!("markdown-preview-{file}")} p=PREVIEW_PADDING pl={ACCENT_WIDTH + GUTTER_WIDTH}>
            <div max_w=PREVIEW_WIDTH>{document}</div>
        </div>
    }
}

fn stat_columns(stat: LineStat, palette: ReadSignal<ReviewTheme>, comments: Memo<usize>) -> Div {
    let tone = move |pick: fn(&DiffPalette) -> Rgba| move || palette.with(|t| Color::from(pick(&t.diff)));
    let count = |value: usize, sign: char| if value == 0 { String::new() } else { format!("{sign}{value}") };
    view! {
        <div flex row flex_none items_center pr=STAT_INSET>
            <div w=COMMENT_COLUMN flex row items_center justify_end gap=4.0>
                <show when={move || comments.get() > 0}>
                    {Icon::new(IconName::MessageCircle, Role::Accent).size(tokens::ICON_SIZE_SMALL)}
                    <text caption accent no_wrap>{move || comments.get().to_string()}</text>
                </show>
            </div>
            <div w=ADDED_COLUMN flex justify_end>
                <text code no_wrap color=tone(|p| p.addition)>{count(stat.added, '+')}</text>
            </div>
            <div w=DELETED_COLUMN flex justify_end>
                <text code no_wrap color=tone(|p| p.deletion)>{count(stat.deleted, '−')}</text>
            </div>
        </div>
    }
}

fn collapsed_comments(handle: &ReviewHandle, index: usize) -> impl Fn() -> usize + 'static {
    let comments = handle.comments_revision();
    let revision = handle.presentation_revision();
    let handle = *handle;
    move || {
        comments.get();
        revision.get();
        handle.with(|s| {
            s.session()
                .presentation()
                .document()
                .files
                .get(index)
                .map_or(0, |file| s.session().review().comments_for_file(&file.path).count())
        })
    }
}

fn header_info(handle: &ReviewHandle, index: usize) -> Option<HeaderInfo> {
    handle.with(|state| {
        let session = state.session();
        let presentation = session.presentation();
        let file = presentation.document().files.get(index)?;
        let start = presentation.file_range(index).map(|range| range.start);
        Some(HeaderInfo {
            status: file.status,
            name: file.path.file_name().to_owned(),
            directory: file.path.directory().map(|directory| format!("{directory}/")),
            renamed_from: file.renamed_from().map(|old| format!("{old}  →  ")),
            stat: file.line_stat(),
            view: presentation.file_view(index),
            previewable: session.command_available(&C::ToggleMarkdownPreview(Some(index))),
            source: session.source_view_label(),
            start,
            after_header: start
                .and_then(|start| start.checked_sub(1))
                .and_then(|previous| presentation.row(previous))
                .is_some_and(|row| row.kind == RowKind::FileHeader),
        })
    })
}

fn gap(handle: &ReviewHandle, index: usize) -> Div {
    let Some(info) = handle.with(|s| s.session().presentation().gap_info(index).cloned()) else {
        return div();
    };
    let handle = *handle;
    let palette = handle.theme();
    let selection = handle.selection();
    let selected = memo(move || {
        let selection = selection.get();
        selection.pane == ViewerPane::Diff && selection.row == Some(index)
    });
    let line = move || {
        div().border_t(1.0).border_color(move || {
            palette.with(|t| Color::from(if selected.get() { t.diff.accent } else { t.diff.border }))
        })
    };
    let reveal = info.is_expandable().then(|| {
        view! {
            <icon_button test_id={format!("reveal-gap-{index}")} icon=IconName::ChevronsUpDown label={info.message()}
                         on_click={move || {
                             if handle.handle_command(C::SelectRow(index)) {
                                 handle.handle_command(C::RevealGap(RevealAmount::Step));
                             }
                         }}/>
        }
    });
    let hint = info.hint();
    view! {
        <div test_id={format!("diff-gap-{index}")} h=CODE_HEIGHT w_full flex row items_center
             on_click={move |_| {
                 handle.handle_command(C::SelectRow(index));
             }}>
            <div w={ACCENT_WIDTH + GUTTER_WIDTH} flex_none flex justify_center>{reveal}</div>
            <show when={move || selected.get()}>
                <div flex row items_center flex_none>
                    {line().w(12.0)}
                    <div px=8.0>
                        <text caption no_wrap color={move || palette.with(|t| Color::from(t.diff.muted))}>{hint.clone()}</text>
                    </div>
                </div>
            </show>
            {line().flex_1()}
        </div>
    }
}
