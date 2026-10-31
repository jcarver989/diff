use crate::tokens::{ICON_SIZE, rem};
use gpui::{
    Animation, AnimationExt, App, Hsla, IntoElement, RenderOnce, SharedString, Styled, Subscription, Transformation,
    Window, prelude::*, radians, svg,
};
use std::f32::consts::TAU;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Archive,
    ArchiveRestore,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    AudioLines,
    Bell,
    Bot,
    Brain,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronsDownUp,
    ChevronsUpDown,
    Circle,
    CircleCheck,
    CircleDot,
    CircleSlash,
    CircleX,
    Cloud,
    Columns2,
    Copy,
    Cpu,
    Ellipsis,
    Eye,
    File,
    FileDiff,
    FileText,
    Folder,
    FolderOpen,
    FolderPlus,
    GitBranch,
    GitCommit,
    GitCompare,
    GitPullRequest,
    Image,
    Info,
    Laptop,
    LoaderCircle,
    MessageCircle,
    MessageSquareText,
    Minus,
    MoveHorizontal,
    Palette,
    PanelLeft,
    PanelRight,
    Paperclip,
    Pause,
    Pencil,
    Pin,
    PinOff,
    Play,
    Plus,
    Power,
    RotateCw,
    Rows2,
    Search,
    Settings,
    Square,
    SquarePen,
    SquareTerminal,
    Timer,
    TriangleAlert,
    Unplug,
    X,
}

impl IconName {
    fn svg(self) -> &'static [u8] {
        match self {
            Self::Archive => include_bytes!("../assets/icons/archive.svg"),
            Self::ArchiveRestore => include_bytes!("../assets/icons/archive-restore.svg"),
            Self::ArrowDown => include_bytes!("../assets/icons/arrow-down.svg"),
            Self::ArrowLeft => include_bytes!("../assets/icons/arrow-left.svg"),
            Self::ArrowRight => include_bytes!("../assets/icons/arrow-right.svg"),
            Self::ArrowUp => include_bytes!("../assets/icons/arrow-up.svg"),
            Self::AudioLines => include_bytes!("../assets/icons/audio-lines.svg"),
            Self::Bell => include_bytes!("../assets/icons/bell.svg"),
            Self::Bot => include_bytes!("../assets/icons/bot.svg"),
            Self::Brain => include_bytes!("../assets/icons/brain.svg"),
            Self::Check => include_bytes!("../assets/icons/check.svg"),
            Self::ChevronDown => include_bytes!("../assets/icons/chevron-down.svg"),
            Self::ChevronLeft => include_bytes!("../assets/icons/chevron-left.svg"),
            Self::ChevronRight => include_bytes!("../assets/icons/chevron-right.svg"),
            Self::ChevronsDownUp => include_bytes!("../assets/icons/chevrons-down-up.svg"),
            Self::ChevronsUpDown => include_bytes!("../assets/icons/chevrons-up-down.svg"),
            Self::Circle => include_bytes!("../assets/icons/circle.svg"),
            Self::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Self::CircleDot => include_bytes!("../assets/icons/circle-dot.svg"),
            Self::CircleSlash => include_bytes!("../assets/icons/circle-slash.svg"),
            Self::CircleX => include_bytes!("../assets/icons/circle-x.svg"),
            Self::Cloud => include_bytes!("../assets/icons/cloud.svg"),
            Self::Columns2 => include_bytes!("../assets/icons/columns-2.svg"),
            Self::Copy => include_bytes!("../assets/icons/copy.svg"),
            Self::Cpu => include_bytes!("../assets/icons/cpu.svg"),
            Self::Ellipsis => include_bytes!("../assets/icons/ellipsis.svg"),
            Self::Eye => include_bytes!("../assets/icons/eye.svg"),
            Self::File => include_bytes!("../assets/icons/file.svg"),
            Self::FileDiff => include_bytes!("../assets/icons/file-diff.svg"),
            Self::FileText => include_bytes!("../assets/icons/file-text.svg"),
            Self::Folder => include_bytes!("../assets/icons/folder.svg"),
            Self::FolderOpen => include_bytes!("../assets/icons/folder-open.svg"),
            Self::FolderPlus => include_bytes!("../assets/icons/folder-plus.svg"),
            Self::GitBranch => include_bytes!("../assets/icons/git-branch.svg"),
            Self::GitCommit => include_bytes!("../assets/icons/git-commit-horizontal.svg"),
            Self::GitCompare => include_bytes!("../assets/icons/git-compare.svg"),
            Self::GitPullRequest => include_bytes!("../assets/icons/git-pull-request.svg"),
            Self::Image => include_bytes!("../assets/icons/image.svg"),
            Self::Info => include_bytes!("../assets/icons/info.svg"),
            Self::Laptop => include_bytes!("../assets/icons/laptop.svg"),
            Self::LoaderCircle => include_bytes!("../assets/icons/loader-circle.svg"),
            Self::MessageCircle => include_bytes!("../assets/icons/message-circle.svg"),
            Self::MessageSquareText => include_bytes!("../assets/icons/message-square-text.svg"),
            Self::Minus => include_bytes!("../assets/icons/minus.svg"),
            Self::MoveHorizontal => include_bytes!("../assets/icons/move-horizontal.svg"),
            Self::Palette => include_bytes!("../assets/icons/palette.svg"),
            Self::PanelLeft => include_bytes!("../assets/icons/panel-left.svg"),
            Self::PanelRight => include_bytes!("../assets/icons/panel-right.svg"),
            Self::Paperclip => include_bytes!("../assets/icons/paperclip.svg"),
            Self::Pause => include_bytes!("../assets/icons/pause.svg"),
            Self::Pencil => include_bytes!("../assets/icons/pencil.svg"),
            Self::Pin => include_bytes!("../assets/icons/pin.svg"),
            Self::PinOff => include_bytes!("../assets/icons/pin-off.svg"),
            Self::Play => include_bytes!("../assets/icons/play.svg"),
            Self::Plus => include_bytes!("../assets/icons/plus.svg"),
            Self::Power => include_bytes!("../assets/icons/power.svg"),
            Self::RotateCw => include_bytes!("../assets/icons/rotate-cw.svg"),
            Self::Rows2 => include_bytes!("../assets/icons/rows-2.svg"),
            Self::Search => include_bytes!("../assets/icons/search.svg"),
            Self::Settings => include_bytes!("../assets/icons/settings.svg"),
            Self::Square => include_bytes!("../assets/icons/square.svg"),
            Self::SquarePen => include_bytes!("../assets/icons/square-pen.svg"),
            Self::SquareTerminal => include_bytes!("../assets/icons/square-terminal.svg"),
            Self::Timer => include_bytes!("../assets/icons/timer.svg"),
            Self::TriangleAlert => include_bytes!("../assets/icons/triangle-alert.svg"),
            Self::Unplug => include_bytes!("../assets/icons/unplug.svg"),
            Self::X => include_bytes!("../assets/icons/x.svg"),
        }
    }
}

/// Redraws `window` whenever it is shown again, which restarts the spinning icons it drew still
/// while it was hidden. A window with spinning icons keeps it for as long as it is open.
pub fn spin_when_shown(window: &Window) -> Subscription {
    window.observe_window_visibility(|visibility, window, _| {
        if visibility.is_visible() {
            window.refresh();
        }
    })
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
        Self { name, size: ICON_SIZE, color: Some(color), hover: None, spin: None }
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

    /// Spins the icon while its window is visible. A hidden window draws it still, so that it
    /// schedules no redraws, until [`spin_when_shown`] redraws the window.
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
            .when_some(self.hover, |icon, (group, color)| icon.group_hover(group, move |icon| icon.text_color(color)));
        if let Some(id) = self.spin.filter(|_| window.is_visible()) {
            let spin = Animation::new(SPIN_PERIOD).repeat_synced().with_max_fps(SPIN_MAX_FPS);
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
/// Twelve steps per turn, which look smooth at icon sizes.
const SPIN_MAX_FPS: f32 = 10.0;
