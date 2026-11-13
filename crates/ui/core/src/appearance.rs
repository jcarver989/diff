use crate::{Appearance, AppearanceMode, ReviewTheme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePair {
    pub light: String,
    pub dark: String,
}

impl ThemePair {
    #[must_use]
    pub fn id(&self, appearance: Appearance) -> &str {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    pub fn set(&mut self, appearance: Appearance, id: impl Into<String>) {
        let slot = match appearance {
            Appearance::Light => &mut self.light,
            Appearance::Dark => &mut self.dark,
        };
        *slot = id.into();
    }
}

impl Default for ThemePair {
    fn default() -> Self {
        Self { light: "github-light".to_owned(), dark: "github-dark".to_owned() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePreference {
    pub mode: AppearanceMode,
    pub pair: ThemePair,
}

impl ThemePreference {
    #[must_use]
    pub const fn appearance(&self, system: Appearance) -> Appearance {
        self.mode.resolve(system)
    }

    #[must_use]
    pub fn theme(&self, system: Appearance) -> ReviewTheme {
        ReviewTheme::builtin(self.pair.id(self.appearance(system))).unwrap_or_default()
    }
}
