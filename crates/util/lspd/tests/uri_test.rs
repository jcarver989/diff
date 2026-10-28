use clankerdiff_lspd::{path_to_uri, uri_to_path};
use lsp_types::Uri;
use std::error::Error;
use std::path::Path;

#[test]
fn paths_round_trip_through_percent_encoded_uris() -> Result<(), Box<dyn Error>> {
    for (path, uri) in [
        ("/src/main.rs", "file:///src/main.rs"),
        ("/src/my file.rs", "file:///src/my%20file.rs"),
        ("/src/a#b.rs", "file:///src/a%23b.rs"),
        ("/src/a?b.rs", "file:///src/a%3Fb.rs"),
        ("/src/100%.rs", "file:///src/100%25.rs"),
        ("/src/café.rs", "file:///src/caf%C3%A9.rs"),
    ] {
        assert_eq!(path_to_uri(Path::new(path))?.as_str(), uri, "{path}");
        assert_eq!(uri_to_path(&uri.parse()?), Path::new(path), "{uri}");
    }
    Ok(())
}

#[test]
fn non_file_uris_pass_through_unchanged() -> Result<(), Box<dyn Error>> {
    let uri: Uri = "https://example.com/file.rs".parse()?;
    assert_eq!(uri_to_path(&uri), Path::new("https://example.com/file.rs"));
    Ok(())
}
