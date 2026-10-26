use gpui::{Rems, rems};

pub const BASE_SIZE: f32 = 16.0;

#[must_use]
pub const fn rem(design_px: f32) -> Rems {
    rems(design_px / BASE_SIZE)
}

pub const ROW_HEIGHT: f32 = 28.0;
pub const GROUP_HEIGHT: f32 = 32.0;
pub const BAR_HEIGHT: f32 = 40.0;
pub const CODE_ROW_HEIGHT: f32 = 20.0;
pub const ICON_BUTTON_SIZE: f32 = 24.0;
pub const ICON_SIZE: f32 = 16.0;
pub const ICON_SIZE_SMALL: f32 = 12.0;
pub const CHECKBOX_SIZE: f32 = 12.0;
pub const TREE_INDENT: f32 = 12.0;
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;
pub const SCROLLBAR_WIDTH: f32 = 12.0;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 32.0;

pub const RADIUS_CONTROL: f32 = 4.0;
pub const RADIUS_CONTAINER: f32 = 8.0;

pub const CONTROL_PADDING_X_SMALL: f32 = 8.0;
pub const CONTROL_PADDING_X_MEDIUM: f32 = 12.0;
pub const CONTROL_PADDING_Y: f32 = 4.0;

pub const MODAL_WIDTH_COMPACT: f32 = 440.0;
pub const MODAL_WIDTH: f32 = 560.0;
pub const MODAL_WIDTH_WIDE: f32 = 680.0;
pub const PICKER_WIDTH: f32 = 440.0;
pub const PICKER_LIST_HEIGHT: f32 = 360.0;
pub const MENU_WIDTH: f32 = 224.0;
pub const MODAL_VIEWPORT_MARGIN: f32 = 32.0;
pub const MODAL_TOP_OFFSET: f32 = 96.0;

pub const SCRIM_OPACITY: f32 = clankerdiff_theme::SCRIM_ALPHA as f32 / 255.0;
