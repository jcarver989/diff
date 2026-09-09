#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NavigationPane {
    #[default]
    Auto,
    Hidden,
    Width(u16),
}

impl NavigationPane {
    pub(crate) fn width(self, available: u16, breakpoint: u16, automatic: u16) -> u16 {
        let width = match self {
            Self::Auto if available >= breakpoint => automatic,
            Self::Width(width) => width,
            Self::Auto | Self::Hidden => 0,
        };
        width.min(available.saturating_sub(2))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewOptions {
    pub footer: bool,
    pub navigation: NavigationPane,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            footer: true,
            navigation: NavigationPane::Auto,
        }
    }
}
