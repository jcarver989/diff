//! Design-system tokens and reusable GPUI components.

pub(crate) mod comments;
pub mod components;
pub mod patterns;
pub mod theme;
pub mod tokens;

/// Common imports for constructing Diff GPUI interfaces.
pub mod prelude {
    pub use super::components::{
        ActionBar, Button, ButtonVariant, ControlSize, ControlState, EmptyState, InteractionState,
        ListRow, Modal, ModalSize, MutedText, NoticeTone, Notification, SelectionState, Surface,
        icon_button,
    };
    pub use super::patterns::{ThemePicker, ThemePickerItem};
    pub use super::theme::{UiColors, UiStyle, UiTheme};
}
