use crate::tokens::{ICON_SIZE, rem};
use gpui::{
    Animation, AnimationExt, App, Hsla, IntoElement, RenderOnce, SharedString, Styled,
    Transformation, Window, prelude::*, radians, svg,
};
use std::f32::consts::TAU;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Archive,
    ArchiveRestore,
    ArrowDown,
    ArrowUp,
    AudioLines,
    Bot,
    Brain,
    Check,
    ChevronDown,
    ChevronRight,
    ChevronsDownUp,
    ChevronsUpDown,
    Circle,
    CircleCheck,
    CircleDot,
    CircleSlash,
    CircleX,
    Copy,
    Cpu,
    Ellipsis,
    Eye,
    File,
    Folder,
    FolderOpen,
    FolderPlus,
    GitCompare,
    Image,
    Info,
    LoaderCircle,
    MessageCircle,
    MessageSquareText,
    Minus,
    PanelLeft,
    PanelRight,
    Paperclip,
    Pencil,
    Pin,
    PinOff,
    Plus,
    RotateCw,
    Search,
    Settings,
    Square,
    SquarePen,
    TriangleAlert,
    X,
}

impl IconName {
    fn svg(self) -> &'static [u8] {
        match self {
            Self::Archive => include_bytes!("../assets/icons/archive.svg"),
            Self::ArchiveRestore => include_bytes!("../assets/icons/archive-restore.svg"),
            Self::ArrowDown => include_bytes!("../assets/icons/arrow-down.svg"),
            Self::ArrowUp => include_bytes!("../assets/icons/arrow-up.svg"),
            Self::AudioLines => include_bytes!("../assets/icons/audio-lines.svg"),
            Self::Bot => include_bytes!("../assets/icons/bot.svg"),
            Self::Brain => include_bytes!("../assets/icons/brain.svg"),
            Self::Check => include_bytes!("../assets/icons/check.svg"),
            Self::ChevronDown => include_bytes!("../assets/icons/chevron-down.svg"),
            Self::ChevronRight => include_bytes!("../assets/icons/chevron-right.svg"),
            Self::ChevronsDownUp => include_bytes!("../assets/icons/chevrons-down-up.svg"),
            Self::ChevronsUpDown => include_bytes!("../assets/icons/chevrons-up-down.svg"),
            Self::Circle => include_bytes!("../assets/icons/circle.svg"),
            Self::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Self::CircleDot => include_bytes!("../assets/icons/circle-dot.svg"),
            Self::CircleSlash => include_bytes!("../assets/icons/circle-slash.svg"),
            Self::CircleX => include_bytes!("../assets/icons/circle-x.svg"),
            Self::Copy => include_bytes!("../assets/icons/copy.svg"),
            Self::Cpu => include_bytes!("../assets/icons/cpu.svg"),
            Self::Ellipsis => include_bytes!("../assets/icons/ellipsis.svg"),
            Self::Eye => include_bytes!("../assets/icons/eye.svg"),
            Self::File => include_bytes!("../assets/icons/file.svg"),
            Self::Folder => include_bytes!("../assets/icons/folder.svg"),
            Self::FolderOpen => include_bytes!("../assets/icons/folder-open.svg"),
            Self::FolderPlus => include_bytes!("../assets/icons/folder-plus.svg"),
            Self::GitCompare => include_bytes!("../assets/icons/git-compare.svg"),
            Self::Image => include_bytes!("../assets/icons/image.svg"),
            Self::Info => include_bytes!("../assets/icons/info.svg"),
            Self::LoaderCircle => include_bytes!("../assets/icons/loader-circle.svg"),
            Self::MessageCircle => include_bytes!("../assets/icons/message-circle.svg"),
            Self::MessageSquareText => include_bytes!("../assets/icons/message-square-text.svg"),
            Self::Minus => include_bytes!("../assets/icons/minus.svg"),
            Self::PanelLeft => include_bytes!("../assets/icons/panel-left.svg"),
            Self::PanelRight => include_bytes!("../assets/icons/panel-right.svg"),
            Self::Paperclip => include_bytes!("../assets/icons/paperclip.svg"),
            Self::Pencil => include_bytes!("../assets/icons/pencil.svg"),
            Self::Pin => include_bytes!("../assets/icons/pin.svg"),
            Self::PinOff => include_bytes!("../assets/icons/pin-off.svg"),
            Self::Plus => include_bytes!("../assets/icons/plus.svg"),
            Self::RotateCw => include_bytes!("../assets/icons/rotate-cw.svg"),
            Self::Search => include_bytes!("../assets/icons/search.svg"),
            Self::Settings => include_bytes!("../assets/icons/settings.svg"),
            Self::Square => include_bytes!("../assets/icons/square.svg"),
            Self::SquarePen => include_bytes!("../assets/icons/square-pen.svg"),
            Self::TriangleAlert => include_bytes!("../assets/icons/triangle-alert.svg"),
            Self::X => include_bytes!("../assets/icons/x.svg"),
        }
    }
}

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: f32,
    color: Option<Hsla>,
    hover: Option<(SharedString, Hsla)>,
    spin: Option<SharedString>,
}

impl Icon {
    #[must_use]
    pub fn new(name: IconName, color: Hsla) -> Self {
        Self {
            name,
            size: ICON_SIZE,
            color: Some(color),
            hover: None,
            spin: None,
        }
    }

    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn inherit_color(mut self) -> Self {
        self.color = None;
        self
    }

    #[must_use]
    pub fn spin(mut self, id: impl Into<SharedString>) -> Self {
        self.spin = Some(id.into());
        self
    }

    #[must_use]
    pub fn group_hover(mut self, group: impl Into<SharedString>, color: Hsla) -> Self {
        self.hover = Some((group.into(), color));
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon = svg()
            .data(self.name.svg())
            .flex_none()
            .size(rem(self.size))
            .text_color(self.color.unwrap_or_else(|| window.text_style().color))
            .when_some(self.hover, |icon, (group, color)| {
                icon.group_hover(group, move |icon| icon.text_color(color))
            });
        if let Some(id) = self.spin {
            let spin = Animation::new(SPIN_PERIOD)
                .repeat_synced()
                .with_max_fps(SPIN_MAX_FPS);
            icon.with_animation(id, spin, |icon, delta| {
                icon.with_transformation(Transformation::rotate(radians(delta * TAU)))
            })
            .into_any_element()
        } else {
            icon.into_any_element()
        }
    }
}

const SPIN_PERIOD: Duration = Duration::from_millis(1200);
const SPIN_MAX_FPS: f32 = 24.0;
