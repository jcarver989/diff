use crate::{
    fonts::{CODE_FONT, UI_FONT},
    tokens::rem,
};
use rouge_core::{Styled, TextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typography {
    Caption,
    Small,
    Label,
    Body,
    BodyStrong,
    Title,
    Heading,
    Display,
    Code,
    Reading,
}

impl Typography {
    #[must_use]
    pub const fn size(self) -> f32 {
        match self {
            Self::Caption => 11.0,
            Self::Small | Self::Label => 12.0,
            Self::Body | Self::BodyStrong | Self::Code => 13.0,
            Self::Title => 14.0,
            Self::Reading => 15.0,
            Self::Heading => 16.0,
            Self::Display => 20.0,
        }
    }

    #[must_use]
    pub const fn line_height(self) -> f32 {
        match self {
            Self::Caption | Self::Small | Self::Label => 16.0,
            Self::Body | Self::BodyStrong | Self::Title | Self::Code => 20.0,
            Self::Heading | Self::Reading => 24.0,
            Self::Display => 28.0,
        }
    }

    #[must_use]
    pub fn text_style(self, root_font_size: f32) -> TextStyle {
        TextStyle {
            family: if self == Self::Code { CODE_FONT } else { UI_FONT }.into(),
            size: self.size() * root_font_size / crate::tokens::BASE_SIZE,
            weight: self.weight(),
            line_height: self.line_height() / self.size(),
            wrap: false,
            ..TextStyle::default()
        }
    }

    #[must_use]
    pub const fn weight(self) -> f32 {
        match self {
            Self::Caption | Self::Small | Self::Body | Self::Code | Self::Reading => 400.0,
            Self::Label | Self::BodyStrong | Self::Title => 500.0,
            Self::Heading | Self::Display => 600.0,
        }
    }
}

pub trait TypographyExt: Styled + Sized {
    #[must_use]
    fn typography(self, typography: Typography) -> Self {
        let styled = self
            .text_size(rem(typography.size()))
            .line_height(typography.line_height() / typography.size())
            .font_weight(typography.weight());
        styled.font(if typography == Typography::Code { CODE_FONT } else { UI_FONT })
    }
}

impl<T: Styled> TypographyExt for T {}
