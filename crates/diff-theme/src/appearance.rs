use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
}

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
        Self {
            light: "github-light".to_owned(),
            dark: "github-dark".to_owned(),
        }
    }
}
