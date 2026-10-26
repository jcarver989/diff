use crate::RemoteError;

pub(crate) fn expand(template: &str, lookup: impl Fn(&str) -> Option<String>) -> Result<String, RemoteError> {
    let mut expanded = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('$') {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let (name, consumed) = if let Some(escaped) = after.strip_prefix('$') {
            expanded.push('$');
            rest = escaped;
            continue;
        } else if let Some(braced) = after.strip_prefix('{')
            && let Some(end) = braced.find('}')
        {
            (&braced[..end], end + 2)
        } else {
            let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
            if end == 0 || after.starts_with(|c: char| c.is_ascii_digit()) {
                expanded.push('$');
                rest = after;
                continue;
            }
            (&after[..end], end)
        };
        expanded.push_str(&lookup(name).ok_or_else(|| RemoteError::Variable(name.to_owned()))?);
        rest = &after[consumed..];
    }
    expanded.push_str(rest);
    Ok(expanded)
}
