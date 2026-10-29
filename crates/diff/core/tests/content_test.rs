use clankerdiff_core::SourceDocument;
use std::error::Error;

#[test]
fn unchanged_lines_map_to_their_place_in_another_version() -> Result<(), Box<dyn Error>> {
    let old = SourceDocument::new("keep\nremoved\nmoved down\nend\n")?;
    let new = SourceDocument::new("added\nkeep\nmoved down\nend\n")?;
    let mapped: Vec<_> = (0..=5).map(|line| old.matching_line(&new, line)).collect();
    assert_eq!(mapped, [None, Some(2), None, Some(3), Some(4), None]);
    assert_eq!(new.matching_line(&new, 3), Some(3));
    Ok(())
}

#[test]
fn a_line_whose_text_changed_has_no_match() -> Result<(), Box<dyn Error>> {
    let old = SourceDocument::new("fn main() {\n    run(1);\n}\n")?;
    let new = SourceDocument::new("fn main() {\n    run(2);\n}\n")?;
    assert_eq!(old.matching_line(&new, 2), None);
    assert_eq!(old.matching_line(&new, 3), Some(3));
    Ok(())
}

#[test]
fn line_endings_and_a_missing_final_newline_do_not_break_a_match() -> Result<(), Box<dyn Error>> {
    let crlf = SourceDocument::new("fn main() {\r\n    run();\r\n}")?;
    let lf = SourceDocument::new("fn main() {\n    run();\n}\n")?;
    assert_eq!((1..=3).map(|line| crlf.matching_line(&lf, line)).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)]);
    Ok(())
}
