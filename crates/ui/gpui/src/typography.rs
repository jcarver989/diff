use crate::{fonts::CODE_FONT, tokens::rem};
use gpui::{FontWeight, Styled};

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
}

impl Typography {
    #[must_use]
    pub const fn size(self) -> f32 {
        match self {
            Self::Caption => 11.0,
            Self::Small | Self::Label => 12.0,
            Self::Body | Self::BodyStrong | Self::Code => 13.0,
            Self::Title => 14.0,
            Self::Heading => 16.0,
            Self::Display => 20.0,
        }
    }

    #[must_use]
    pub const fn line_height(self) -> f32 {
        match self {
            Self::Caption | Self::Small | Self::Label => 16.0,
            Self::Body | Self::BodyStrong | Self::Title | Self::Code => 20.0,
            Self::Heading => 24.0,
            Self::Display => 28.0,
        }
    }

    #[must_use]
    pub const fn weight(self) -> FontWeight {
        match self {
            Self::Caption | Self::Small | Self::Body | Self::Code => FontWeight::NORMAL,
            Self::Label | Self::BodyStrong | Self::Title => FontWeight::MEDIUM,
            Self::Heading | Self::Display => FontWeight::SEMIBOLD,
        }
    }
}

pub trait TypographyExt: Styled + Sized {
    #[must_use]
    fn typography(self, typography: Typography) -> Self {
        let styled = self
            .text_size(rem(typography.size()))
            .line_height(rem(typography.line_height()))
            .font_weight(typography.weight());
        if typography == Typography::Code {
            styled.font_family(CODE_FONT)
        } else {
            styled
        }
    }
}

impl<T: Styled> TypographyExt for T {}
