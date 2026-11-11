use crate::{
    ReviewHandle, ScrollRequest, ViewerPane, annotation, comment_editor, comments, definition_card, review_bar,
    sidebar, style, toolbar, viewer::Viewport,
};
use clankerdiff_core::{
    DiffReviewCommand as C, DiffSide, DiffTone, FileStatus, Layout, LineAnchor, LineStat, PresentedCell, PresentedRow,
    RevealAmount, RowId, RowKind, SourceLocation,
};
use clankerdiff_rouge_ui::{
    components::{empty_state, icon_button},
    icon::IconName,
    scrollbar::ListScrollbar,
    theme::{UiTheme, color},
    tokens::{BASE_SIZE, RADIUS_CONTROL, scaled},
    typography::{Typography, UiStyles},
};
use clankerdiff_ui_core::{DiffPalette, Rgba};
use rouge_core::{
    Align, AnyView, Canvas, Color, Cursor, Div, Painter, Point, Rect, RowKey, ScrollLinked, Size, SpanStyle, Styled,
    TextRun, View, VirtualListHandle, canvas, div, dynamic, pct, px, root_font_size, show, text, view, virtual_list,
    virtual_list_handle,
};
use rouge_reactive::{Memo, Signal, effect, memo, signal, untrack};
use std::{cell::Cell, ops::Range, rc::Rc};

const CODE_HEIGHT: f32 = 22.0;
const HEADER_HEIGHT: f32 = 40.0;
const HEADER_INSET: f32 = 4.0;
const ACCENT_WIDTH: f32 = 3.0;
const GUTTER_WIDTH: f32 = 52.0;
const GUTTER_PADDING: f32 = 7.0;
const TEXT_TOP: f32 = 1.0;
const META_PADDING: f32 = 12.0;
const ADD_COMMENT_SIZE: f32 = 19.0;

#[derive(Clone, Copy)]
struct Horizontal {
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
        <div size_full flex col bg={move || bg.with(|t| color(t.diff.background))}>
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
    let horizontal = Horizontal { offset: signal(0.0), limit: signal(0.0) };
    let hover: Hover = signal(None);
    effect(move || {
        revision.get();
        horizontal.offset.set_if_changed(0.0);
        horizontal.limit.set_if_changed(0.0);
    });
    bind_navigation(&handle, list, files, range, visible);
    let rows = virtual_list(
        move || range.get().len(),
        CODE_HEIGHT,
        move |index| row(&handle, range.get_untracked().start + index, horizontal, hover),
    )
    .row_identity(move |index| {
        let revision = revision.get();
        let index = range.get().start + index;
        RowKey::new(
            handle.with(|state| state.session().presentation().row(index).map_or(RowId(0), |row| row.id)),
            revision,
        )
    })
    .measure_rows(|| 0)
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
             on_scroll={move |event| {
                 if event.delta.x != 0.0 {
                     let max = horizontal.limit.get_untracked();
                     horizontal.offset.update(|x| *x = (*x - event.delta.x).clamp(0.0, max));
                 }
             }}>
            {rows}
            <dynamic children=empty/>
            {sticky(handle, list, active, range)}
            <div absolute right=0.0 top=0.0 bottom=0.0>{ListScrollbar::list(list)}</div>
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
    handle.bind_viewport(Viewport { diff: list, files, visible, root_font_size: root_font_size() });
    let requests = handle.reveal_row();
    let header = scaled(HEADER_HEIGHT);
    effect(move || {
        if let Some((row, align)) = requests.get() {
            let range = range.get_untracked();
            if range.contains(&row) {
                let index = row - range.start;
                if align == Align::Visible {
                    list.reveal_row(index, untrack(&header));
                } else {
                    list.scroll_to_row(index, align);
                }
            }
        }
    });
}

impl Viewport {
    fn row_height(&self) -> f32 {
        CODE_HEIGHT * self.root_font_size.get_untracked() / BASE_SIZE
    }

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
            (height / self.row_height()).floor() as usize
        };
        isize::try_from(rows.max(1)).unwrap_or(isize::MAX)
    }

    #[expect(clippy::cast_precision_loss, reason = "Scroll requests are a handful of lines")]
    pub(crate) fn scroll(&self, request: ScrollRequest) {
        let list = if request.pane == ViewerPane::Diff { self.diff } else { self.files };
        let Some(state) = list.scroll_state().try_get() else {
            return;
        };
        let offset = state.offset.y + request.lines as f32 * self.row_height();
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
        let size = scaled(HEADER_HEIGHT);
        let link = move || ScrollLinked::Sticky {
            lo: list.row_top(span.start.saturating_sub(start)),
            hi: list.row_top(span.end.saturating_sub(start)) - size(),
        };
        Some(view! {
            <div test_id="sticky-file-header" absolute top=0.0 left=0.0 right=0.0
                 scroll_linked=(list.scroll_state(), link)>
                {header(&handle, file)}
            </div>
        })
    })
}

fn row(handle: &ReviewHandle, index: usize, x: Horizontal, hover: Hover) -> AnyView {
    let Some(row) = handle.with(|s| s.session().presentation().row(index).cloned()) else {
        return div().into_any();
    };
    match row.kind {
        RowKind::FileHeader => header(handle, row.file_index).into_any(),
        RowKind::Meta => meta(index, &row).into_any(),
        RowKind::ExpandGap => gap(handle, index).into_any(),
        RowKind::Code | RowKind::ExpandedContext => line(handle, index, &row, x, hover).into_any(),
    }
}

fn meta(index: usize, row: &PresentedRow) -> Div {
    let theme = UiTheme::current();
    let content = row.primary_cell().map_or_else(String::new, |cell| cell.text.to_string());
    let root = root_font_size();
    let label = canvas(move |painter| {
        let scale = root.get() / BASE_SIZE;
        let style = Typography::Code.text_style(root.get());
        let size = painter.size();
        let origin = Point::new(META_PADDING * scale, TEXT_TOP * scale);
        let clip = Rect::new(0.0, 0.0, size.width, size.height);
        painter.run(TextRun::new(content.clone(), origin, &style, theme.text_muted.get()).clip(clip));
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

    fn location(columns: &[Column], tag: u32) -> Option<SourceLocation> {
        columns.iter().find(|column| column.side as u32 == tag).and_then(|column| column.location.clone())
    }
}

fn line(handle: &ReviewHandle, index: usize, row: &PresentedRow, x: Horizontal, hover: Hover) -> Div {
    let handle = *handle;
    let columns = Column::all(&handle, row);
    let size = signal(Size::ZERO);
    let overflow = signal(0.0_f32);
    effect(move || {
        let width = overflow.get();
        if width > x.limit.get_untracked() {
            x.limit.set(width);
        }
    });
    let height = scaled(CODE_HEIGHT);
    let painted = painted(&handle, index, row.clone(), &columns, x)
        .track_overflow(overflow)
        .on_text_hover({
            let columns = columns.clone();
            let hovered = signal(None::<Range<usize>>);
            let last = Cell::new(None::<u32>);
            move |hit, modifiers| {
                let tag = hit.map(|hit| hit.tag);
                if let Some(previous) = last.replace(tag).filter(|previous| Some(*previous) != tag)
                    && let Some(location) = Column::location(&columns, previous)
                {
                    definition_card::hover(&handle, &location, None, modifiers, hovered);
                }
                if let Some(hit) = hit
                    && let Some(location) = Column::location(&columns, hit.tag)
                {
                    definition_card::hover(&handle, &location, Some(hit.index), modifiers, hovered);
                }
            }
        })
        .on_text_click({
            let columns = columns.clone();
            move |hit, modifiers| {
                if let Some(hit) = hit
                    && let Some(location) = Column::location(&columns, hit.tag)
                {
                    definition_card::click(&handle, &location, Some(hit.index), modifiers);
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
        .w_full()
        .h(CODE_HEIGHT);
    let pointer = columns.clone();
    view! {
        <div test_id={format!("diff-row-{index}")} layer flex col min_h=CODE_HEIGHT relative track_size=size
             on_pointer_move={move |event| {
                 let point = event.local();
                 let target = (point.y < height())
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

struct Fill {
    background: Color,
    accent: Option<Color>,
    spans: Vec<(Range<usize>, SpanStyle)>,
}

fn painted(handle: &ReviewHandle, index: usize, row: PresentedRow, columns: &Rc<[Column]>, x: Horizontal) -> Canvas {
    let theme = UiTheme::current();
    let selection = handle.selection();
    let selected = memo(move || {
        let selection = selection.get();
        (selection.row == Some(index)).then_some(selection.side)
    });
    let (handle, columns) = (*handle, columns.clone());
    let palette = handle.theme();
    let root = root_font_size();
    let file_index = row.file_index;
    let revision = handle.syntax_revision();
    let highlighted = memo(move || {
        revision.get();
        handle.with_syntax(|_, syntax| syntax.is_prepared(file_index))
    });
    canvas(move |painter| {
        let colors = theme.get();
        let selected = selected.get();
        highlighted.get();
        let offset = x.offset.get();
        palette.with(|review| {
            let fill = |column: &Column, cell: &PresentedCell| Fill {
                background: color(if selected == Some(column.side) {
                    review.diff.selection
                } else {
                    review.diff.tone(cell.tone).background
                }),
                accent: match cell.tone {
                    DiffTone::Added => Some(color(review.diff.addition)),
                    DiffTone::Removed => Some(color(review.diff.deletion)),
                    _ => None,
                },
                spans: handle.with_syntax(|_, syntax| {
                    style::highlights(cell.source_text(), syntax.ready_highlights(&row, cell), &review.syntax)
                }),
            };
            let ink = Ink { number: colors.text_muted, text: colors.text, border: colors.border };
            paint_line(painter, index, &columns, root.get(), offset, ink, fill);
        });
    })
}

#[derive(Clone, Copy)]
struct Ink {
    number: Color,
    text: Color,
    border: Color,
}

#[expect(clippy::cast_precision_loss, reason = "Column counts are one or two")]
fn paint_line(
    painter: &mut Painter,
    index: usize,
    columns: &[Column],
    root: f32,
    offset: f32,
    ink: Ink,
    fill: impl Fn(&Column, &PresentedCell) -> Fill,
) {
    let bounds = painter.size();
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let scale = root / BASE_SIZE;
    let width = bounds.width / columns.len() as f32;
    let style = Typography::Code.text_style(root);
    for (position, column) in columns.iter().enumerate() {
        let left = position as f32 * width;
        let rect = Rect::new(left, 0.0, width, bounds.height);
        let side = column.side;
        if position + 1 < columns.len() {
            painter.fill_rect(Rect::new(left + width - scale, 0.0, scale, bounds.height), ink.border);
        }
        let Some(cell) = &column.cell else {
            painter.region(format!("empty-{index}-{side:?}"), rect);
            continue;
        };
        painter.region(format!("diff-cell-{index}-{side:?}"), rect);
        let Fill { background, accent, spans } = fill(column, cell);
        let inner =
            Rect::new(left, 0.0, if position + 1 < columns.len() { width - scale } else { width }, bounds.height);
        painter.fill_rect(inner, background);
        if let Some(accent) = accent {
            painter.fill_rect(Rect::new(left, 0.0, ACCENT_WIDTH * scale, bounds.height), accent);
        }
        if column.anchor.is_some() {
            let button = ADD_COMMENT_SIZE * scale;
            painter.region(
                format!("add-comment-{index}-{side:?}"),
                Rect::new(left + ACCENT_WIDTH * scale, 0.0, button, button),
            );
        }
        if let Some(line) = cell.line_number() {
            let edge = left + (ACCENT_WIDTH + GUTTER_WIDTH - GUTTER_PADDING) * scale;
            painter.run(
                TextRun::new(line.to_string(), Point::new(edge, TEXT_TOP * scale), &style, ink.number).right_aligned(),
            );
        }
        let code_left = left + (ACCENT_WIDTH + GUTTER_WIDTH) * scale;
        let clip = Rect::new(code_left, 0.0, (inner.max().x - code_left).max(0.0), bounds.height);
        let mut code = style.clone();
        code.spans = spans;
        painter.run(
            TextRun::new(cell.text.to_string(), Point::new(code_left - offset, TEXT_TOP * scale), &code, ink.text)
                .clip(clip)
                .tag(side as u32)
                .name(format!("code-{index}-{side:?}")),
        );
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
    let definition = handle.definition_revision();
    let shown = memo({
        let columns = columns.clone();
        move || {
            comments.get();
            let draft = draft.get();
            definition.get();
            handle.with(|state| {
                columns
                    .iter()
                    .map(|column| {
                        let thread = column.anchor.as_ref().is_some_and(|anchor| {
                            draft.as_ref().is_some_and(|(placed, _)| placed == anchor)
                                || state.session().review().comments_for_anchor(anchor).any(|comment| !comment.outdated)
                        });
                        let card = column.location.as_ref().is_some_and(|location| {
                            state.definition_card().is_some_and(|card| card.origin == *location)
                        });
                        (thread, card)
                    })
                    .collect::<Vec<_>>()
            })
        }
    });
    dynamic(move || {
        let shown = shown.get();
        if shown.iter().all(|(thread, card)| !thread && !card) {
            return None;
        }
        let cells = columns.iter().zip(shown).map(|(column, (thread, card))| {
            let thread = column.anchor.clone().filter(|_| thread);
            let card = column.location.clone().filter(|_| card);
            view! {
                <div flex_1 min_w=0.0 flex col>
                    {thread.map(|anchor| comments::thread(handle, index, column.side, anchor))}
                    {card.map(|location| definition_card::at_line(handle, location))}
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
    collapsed: bool,
    source: Option<&'static str>,
    start: Option<usize>,
}

fn header(handle: &ReviewHandle, index: usize) -> Div {
    let Some(info) = header_info(handle, index) else {
        return div();
    };
    let theme = UiTheme::current();
    let handle = *handle;
    let palette = handle.theme();
    let tone = move |pick: fn(&DiffPalette) -> Rgba| move || palette.with(|t| color(pick(&t.diff)));
    let selection = handle.selection();
    let start = info.start;
    let count = collapsed_comments(&handle, index);
    let (status, collapsed) = (info.status, info.collapsed);
    let (icon, label) =
        if collapsed { (IconName::ChevronRight, "Expand file") } else { (IconName::ChevronDown, "Collapse file") };
    let toggle = info.source.is_none().then(|| {
        view! {
            <icon_button id={format!("collapse-file-{index}")} icon label
                         on_click={move || {
                             handle.handle_command(C::ToggleFileCollapsed(Some(index)));
                         }}/>
        }
    });
    let paths = info.directory.into_iter().chain(info.renamed_from);
    view! {
        <div test_id={format!("diff-file-header-{index}")} h=HEADER_HEIGHT w_full px=8.0 py=HEADER_INSET
             bg=tone(|p| p.background) cursor=Cursor::Pointer
             on_click={move |_| {
                 handle.handle_command(C::SelectFile(index));
             }}>
            <div size_full flex row items_center gap=8.0 px=12.0 rounded=RADIUS_CONTROL border=px(1.0)
                 border_color={move || {
                     let selection = selection.get();
                     let selected = selection.pane == ViewerPane::Diff && start.is_some() && selection.row == start;
                     palette.with(|t| color(if selected { t.diff.accent } else { t.diff.border }))
                 }}
                 bg=theme.surface overflow_hidden>
                {toggle}
                <text code color={move || palette.with(|t| color(style::file_status_color(status, &t.diff)))}>
                    {status.code().to_string()}
                </text>
                <text body_strong no_wrap color=tone(|p| p.foreground)>{info.name}</text>
                <div flex row flex_1 min_w=0.0 gap=8.0 overflow_hidden>
                    {..paths.map(move |label| view! { <text body no_wrap color=tone(|p| p.muted)>{label}</text> })}
                </div>
                {match info.source {
                    Some(source) => view! {
                        <div test_id="source-mode-header" flex_none>
                            <text body no_wrap color=tone(|p| p.muted)>{source}</text>
                        </div>
                    },
                    None => style::stat_badges(info.stat, palette),
                }}
                <text caption accent>
                    {move || {
                        let count = count();
                        if collapsed && count > 0 { format!("{count} comments") } else { String::new() }
                    }}
                </text>
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
        Some(HeaderInfo {
            status: file.status,
            name: file.path.file_name().to_owned(),
            directory: file.path.directory().map(|directory| format!("{directory}/")),
            renamed_from: file.renamed_from().map(|old| format!("← {old}")),
            stat: file.line_stat(),
            collapsed: presentation.is_collapsed(index),
            source: session.source_view_label(),
            start: presentation.file_range(index).map(|range| range.start),
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
        div().h(px(1.0)).bg(move || palette.with(|t| color(if selected.get() { t.diff.accent } else { t.diff.border })))
    };
    let reveal = info.is_expandable().then(|| {
        view! {
            <icon_button id={format!("reveal-gap-{index}")} icon=IconName::ChevronsUpDown label={info.message()}
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
                        <text caption no_wrap color={move || palette.with(|t| color(t.diff.muted))}>{hint.clone()}</text>
                    </div>
                </div>
            </show>
            {line().flex_1()}
        </div>
    }
}
