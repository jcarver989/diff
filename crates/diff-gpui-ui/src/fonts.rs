use gpui::{App, Result};
use std::borrow::Cow;

pub const UI_FONT: &str = "Inter";
pub const CODE_FONT: &str = "Lilex";

const FACES: [&[u8]; 16] = [
    include_bytes!("../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Italic.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Regular.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Bold.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Italic.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-BoldItalic.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Regular.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Bold.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Italic.otf"),
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-BoldItalic.otf"),
];

pub fn load_fonts(cx: &mut App) -> Result<()> {
    cx.text_system()
        .add_fonts(FACES.iter().map(|face| Cow::Borrowed(*face)).collect())
}
