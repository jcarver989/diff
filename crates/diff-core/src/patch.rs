use similar::TextDiff;
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PatchError {
    #[error("patch path must be nonempty, contain no NUL bytes, and not be /dev/null")]
    InvalidPath,
    #[error("text patches cannot contain NUL bytes")]
    BinaryText,
}

pub fn git_patch_from_texts(
    path: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> Result<Option<String>, PatchError> {
    if path.is_empty() || path.contains('\0') || path == "/dev/null" {
        return Err(PatchError::InvalidPath);
    }
    if old.into_iter().chain(new).any(|text| text.contains('\0')) {
        return Err(PatchError::BinaryText);
    }
    if old == new {
        return Ok(None);
    }

    let quoted_path = quote_path(path);
    let mut output = format!("diff --git {quoted_path} {quoted_path}\n");
    if old.is_none() {
        output.push_str("new file mode 100644\n");
    } else if new.is_none() {
        output.push_str("deleted file mode 100644\n");
    }

    let old_path = if old.is_some() {
        &quoted_path
    } else {
        "/dev/null"
    };
    let new_path = if new.is_some() {
        &quoted_path
    } else {
        "/dev/null"
    };
    output.extend(["--- ", old_path, "\n+++ ", new_path, "\n"]);
    let diff = TextDiff::from_lines(old.unwrap_or_default(), new.unwrap_or_default());
    output.push_str(&diff.unified_diff().context_radius(3).to_string());
    Ok(Some(output))
}

fn quote_path(path: &str) -> String {
    let mut quoted = String::from("\"");
    for byte in path.bytes() {
        match byte {
            b'"' => quoted.push_str("\\\""),
            b'\\' => quoted.push_str("\\\\"),
            0x20..=0x7e => quoted.push(char::from(byte)),
            _ => {
                quoted.push('\\');
                quoted.push(char::from(b'0' + (byte >> 6)));
                quoted.push(char::from(b'0' + ((byte >> 3) & 7)));
                quoted.push(char::from(b'0' + (byte & 7)));
            }
        }
    }
    quoted.push('"');
    quoted
}
