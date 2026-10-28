use lsp_types::Uri;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, thiserror::Error)]
#[error("Invalid path '{}': {reason}", path.display())]
pub struct UriError {
    path: PathBuf,
    reason: String,
}

pub fn path_to_uri(path: &Path) -> Result<Uri, UriError> {
    let absolute =
        if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let absolute = absolute.canonicalize().unwrap_or(absolute);
    let url = Url::from_file_path(&absolute)
        .map_err(|()| UriError { path: absolute.clone(), reason: "not an absolute file path".to_string() })?;
    url.as_str().parse::<Uri>().map_err(|e| UriError { path: absolute, reason: e.to_string() })
}

#[must_use]
pub fn uri_to_path(uri: &Uri) -> PathBuf {
    Url::parse(uri.as_str()).ok().and_then(|url| url.to_file_path().ok()).unwrap_or_else(|| PathBuf::from(uri.as_str()))
}
