mod common;

use clankerdiff_core::{DiffDocument, SourceLocation};
use clankerdiff_core::{
    DiffReviewCommand,
    testing::{DocumentBuilder, location},
};
use clankerdiff_rouge::{ReviewController, ReviewHandle};
use rouge_core::{
    Modifiers, Point, Rect,
    testing::{TestResult, TestUi},
};
use std::{error::Error, fmt::Write, io, ops::Range, sync::Arc};

#[test]
fn modifier_hover_uses_shaped_utf8_ranges_and_click_jumps() -> TestResult {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn build() -> u32 { 1 }\n")
        .changed("src/main.rs", "", "fn main() {\n    let _ = \"漢\"; build();\n}\n")
        .build();
    let origin = location("src/main.rs", 2)?;
    let destination = location("src/lib.rs", 1)?;
    common::run(document, |ui, handle, _| {
        assert!(handle.handle_command(DiffReviewCommand::JumpTo(origin.into())));
        let row = handle.selection().get().row.ok_or_else(|| io::Error::other("no selected row"))?;
        assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
        let bounds = ui.bounds_of(&format!("code-{row}-New")).ok_or_else(|| io::Error::other("code not mounted"))?;
        let token = "    let _ = \"漢\"; ".len();
        let point = (0_u16..350)
            .map(|n| Point::new(bounds.origin.x + f32::from(n), bounds.center().y))
            .find(|point| ui.text_index_at(*point).is_some_and(|index| (token + 1..token + 4).contains(&index)))
            .ok_or_else(|| io::Error::other("build token has no shaped hit range"))?;
        ui.pointer_moved(point);
        assert!(handle.with(|s| s.definition_card().is_none()));
        ui.set_modifiers(Modifiers::COMMAND);
        assert!(handle.with(|s| s.definition_card().is_some_and(|card| card.name == "build")));
        assert!(ui.center_of("definition-card-signature").is_ok());
        ui.click(point);
        assert_eq!(handle.with(|s| s.session().selected_source_line()), Some(destination));
        Ok(())
    })
}

#[test]
fn the_card_floats_below_its_symbol_without_moving_the_rows() -> TestResult {
    let (document, origin) = build_call("pub fn build() -> u32 { 1 }\n")?;
    common::run(document, |ui, handle, _| {
        let token = token(ui, &handle, &origin, "build")?;
        let next = ui.bounds_of(&format!("diff-row-{}", token.row + 1)).ok_or_else(|| missing("next row"))?;
        ui.pointer_moved(token.point);
        ui.set_modifiers(Modifiers::COMMAND);
        ui.run(0.3);
        let card = ui.bounds_of("definition-card").ok_or_else(|| missing("definition card"))?;
        assert_eq!(ui.bounds_of(&format!("diff-row-{}", token.row + 1)), Some(next), "rows stay put");
        assert!(card.origin.y >= token.line.max().y, "{card:?} sits below {:?}", token.line);
        assert!((card.origin.x - token.left).abs() < 1.0, "{card:?} starts at the symbol {}", token.left);
        Ok(())
    })
}

#[test]
fn docs_render_as_markdown_with_highlighted_code() -> TestResult {
    let (document, origin) =
        build_call("/// Builds a `u32`.\n///\n/// ```\n/// let n = build();\n/// ```\npub fn build() -> u32 { 1 }\n")?;
    common::run(document, |ui, handle, _| {
        let token = token(ui, &handle, &origin, "build")?;
        ui.pointer_moved(token.point);
        ui.set_modifiers(Modifiers::COMMAND);
        ui.run(0.3);
        let docs = ui.bounds_of("definition-card-docs").ok_or_else(|| missing("docs"))?;
        assert!(ui.text_bounds("let n = build();").is_some_and(|code| docs.contains(code.center())));
        assert!(ui.text_bounds("```").is_none(), "fences are parsed, not shown");
        assert!(ui.text_bounds("`u32`").is_none(), "inline code is parsed, not shown");
        Ok(())
    })
}

#[test]
fn holding_the_link_modifier_underlines_references() -> TestResult {
    let (document, origin) = build_call("pub fn build() -> u32 { 1 }\n")?;
    common::run(document, |ui, handle, _| {
        let token = token(ui, &handle, &origin, "build")?;
        ui.pointer_moved(Point::new(token.line.origin.x + 2.0, token.line.center().y));
        let plain = pixels(ui, token.ink)?;
        ui.set_modifiers(Modifiers::COMMAND);
        let linked = pixels(ui, token.ink)?;
        assert!(handle.with(|s| s.definition_card().is_none()), "no card without hovering a symbol");
        assert_ne!(plain, linked, "the symbol is underlined");
        ui.set_modifiers(Modifiers::NONE);
        assert_eq!(pixels(ui, token.ink)?, plain, "releasing the modifier removes it");
        Ok(())
    })
}

#[test]
fn clicking_outside_a_pinned_card_closes_it() -> TestResult {
    let (document, origin) = build_call("pub fn build() -> u32 { 1 }\n")?;
    common::run(document, |ui, handle, _| {
        let token = token(ui, &handle, &origin, "build")?;
        ui.press("shift-D");
        ui.run(0.3);
        let card = ui.bounds_of("definition-card").ok_or_else(|| missing("definition card"))?;
        ui.click(Point::new(card.origin.x + 4.0, card.origin.y + 4.0));
        assert!(handle.with(|s| s.definition_card().is_some()), "clicks inside keep it open");
        ui.click(Point::new(token.line.origin.x + 2.0, token.line.center().y));
        assert!(handle.with(|s| s.definition_card().is_none()), "an outside click closes it");
        Ok(())
    })
}

#[test]
fn going_back_after_a_modifier_click_returns_to_the_clicked_line_where_it_was_on_screen() -> TestResult {
    let filler = (0..150).fold(String::new(), |mut filler, n| {
        let _ = writeln!(filler, "    let a{n} = {n};");
        filler
    });
    let library = format!("{}pub fn build() -> u32 {{ 1 }}\n", filler.replace("    let", "const _: () = {}; //"));
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", &library)
        .changed("src/main.rs", "", &format!("fn main() {{\n{filler}    build();\n}}\n"))
        .build();
    let origin = location("src/main.rs", 152)?;
    let destination = location("src/lib.rs", 151)?;
    common::run(document, |ui, handle, _| {
        let token = token(ui, &handle, &origin, "build")?;
        assert!(handle.handle_command(DiffReviewCommand::SelectRow(token.row - 5)));
        ui.pointer_moved(token.line.center());
        ui.scroll(Point::new(0.0, -37.0));
        ui.run(0.1);
        let before = ui.bounds_of(&format!("code-{}-New", token.row)).ok_or_else(|| missing("clicked line"))?;
        let point = Point::new(token.point.x, before.center().y);
        ui.set_modifiers(Modifiers::COMMAND);
        ui.pointer_moved(point);
        ui.click(point);
        ui.set_modifiers(Modifiers::NONE);
        ui.run(0.1);
        assert_eq!(handle.with(|s| s.session().selected_source_line()), Some(destination));
        assert!(handle.handle_command(DiffReviewCommand::NavigateBack));
        ui.run(0.3);
        assert_eq!(
            handle.with(|s| s.session().selected_source_line()),
            Some(origin.clone()),
            "back to the clicked line"
        );
        let row = handle.selection().get().row.ok_or_else(|| missing("selected row"))?;
        let after = ui.bounds_of(&format!("code-{row}-New")).ok_or_else(|| missing("returned line"))?;
        assert!((after.origin.y - before.origin.y).abs() < 0.5, "{after:?} returns to {before:?}");
        Ok(())
    })
}

struct Token {
    row: usize,
    point: Point,
    left: f32,
    line: Rect,
    ink: Rect,
}

fn build_call(library: &str) -> Result<(Arc<DiffDocument>, SourceLocation), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", library)
        .changed("src/main.rs", "", "fn main() {\n    build();\n}\n")
        .build();
    Ok((document, location("src/main.rs", 2)?))
}

fn token(ui: &mut TestUi, handle: &ReviewHandle, origin: &SourceLocation, name: &str) -> Result<Token, io::Error> {
    assert!(handle.handle_command(DiffReviewCommand::JumpTo(origin.clone().into())));
    let row = handle.selection().get().row.ok_or_else(|| missing("selected row"))?;
    assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
    let line = ui.bounds_of(&format!("code-{row}-New")).ok_or_else(|| missing("code"))?;
    let text = handle
        .with(|s| {
            s.session().presentation().row(row).and_then(|row| row.right.as_ref()).map(|cell| cell.text.to_string())
        })
        .ok_or_else(|| missing("row text"))?;
    let start = text.find(name).ok_or_else(|| missing("symbol"))?;
    let bytes: Range<usize> = start..start + name.len();
    let hits: Vec<f32> = (0_u16..600)
        .map(|n| line.origin.x + f32::from(n) / 2.0)
        .filter(|x| ui.text_index_at(Point::new(*x, line.center().y)).is_some_and(|index| bytes.contains(&index)))
        .collect();
    let (first, last) =
        (hits.first().copied().ok_or_else(|| missing("shaped symbol"))?, hits.last().copied().unwrap_or(0.0));
    let ink = Rect::new(first, line.origin.y, last - first, line.size.height);
    Ok(Token { row, point: Point::new(first.midpoint(last), line.center().y), left: first, line, ink })
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "Test rects are on screen")]
fn pixels(ui: &mut TestUi, rect: Rect) -> Result<Vec<[u8; 4]>, io::Error> {
    ui.advance(0.0);
    let image = ui.render().map_err(|error| io::Error::other(error.to_string()))?;
    let (x0, x1) = (rect.origin.x.ceil() as u32, rect.max().x.floor() as u32);
    let (y0, y1) = (rect.origin.y.ceil() as u32, rect.max().y.floor() as u32);
    Ok((y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| image.pixel(x, y)).collect())
}

fn missing(what: &str) -> io::Error {
    io::Error::other(format!("{what} not found"))
}
