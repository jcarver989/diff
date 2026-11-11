#![allow(clippy::must_use_candidate, clippy::too_many_arguments, clippy::fn_params_excessive_bools)]

pub mod components;
pub mod dialog;
pub mod fonts;
pub mod icon;
pub mod indicator_layer;
pub mod inline_edit;
pub mod interface_size;
pub mod keymap;
pub mod menu;
pub mod picker;
pub mod resizable;
pub mod scrollbar;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
pub mod theme;
pub mod tokens;
pub mod tree;
pub mod typography;
pub mod waveform;
pub mod window_chrome;

pub use interface_size::InterfaceSize;

pub mod prelude {
    pub use super::InterfaceSize;
    pub use super::components::{
        ButtonVariant, CheckState, ControlSize, ControlState, InteractionState, ModalSize, NoticeTone, SelectionState,
        SurfaceExt, action_bar, backdrop, badge, button, checkbox, disclosure, empty_state, hover_group, icon_button,
        list_row, modal, modal_width, muted_text, notification, section_header, segment, segmented_control, surface,
    };
    pub use super::dialog::{ConfirmDialogEvent, confirm_dialog};
    pub use super::fonts::{CODE_FONT, UI_FONT};
    pub use super::icon::{Icon, IconName};
    pub use super::indicator_layer::{IndicatorLayer, IndicatorMark};
    pub use super::inline_edit::inline_edit;
    pub use super::keymap::{binding_label, key_binding};
    pub use super::menu::{MenuItem, with_menu};
    pub use super::picker::{
        PickerDetail, PickerItem, PickerMarker, Preview, SearchPickerEvent, SearchPickerHandle, ThemePickerEvent,
        picker_list, search_picker, theme_picker, theme_picker_overlay, theme_picker_view,
    };
    pub use super::resizable::{PanelSide, panel_toggle, resizable_panel};
    pub use super::scrollbar::ListScrollbar;
    pub use super::theme::{ActiveTheme, UiColors, UiStyle, UiTheme, color, window_appearance};
    pub use super::tree::tree_row;
    pub use super::typography::{Typography, UiStyles};
    pub use super::waveform::Waveform;
    pub use super::window_chrome::WindowDragExt;
    pub use rouge_widgets::Hint;
}
