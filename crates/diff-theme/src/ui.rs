use crate::{ThemeError, UiPalette};
use std::{collections::BTreeMap, sync::LazyLock};

static BUILTINS: LazyLock<Result<BTreeMap<String, UiPalette>, ThemeError>> = LazyLock::new(|| {
    serde_json::from_slice(include_bytes!("../assets/ui.json")).map_err(|error| ThemeError::Parse {
        message: error.to_string(),
    })
});

pub(crate) fn builtin(id: &str) -> Result<UiPalette, ThemeError> {
    BUILTINS
        .as_ref()
        .map_err(Clone::clone)?
        .get(id)
        .copied()
        .ok_or_else(|| ThemeError::Parse {
            message: format!("built-in theme `{id}` has no UI palette"),
        })
}
