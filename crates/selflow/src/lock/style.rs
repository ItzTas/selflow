use iced::theme::Palette;
use iced::{Color, Theme};

pub const BACKGROUND: Color = Color::from_rgb8(0x17, 0x19, 0x1c);
pub const TEXT: Color = Color::from_rgb8(0xd9, 0xdb, 0xde);
pub const MUTED: Color = Color::from_rgb8(0x8b, 0x90, 0x97);
pub const ACCENT: Color = Color::from_rgb8(0x7f, 0x9c, 0xb3);

pub const BRAND_SIZE: f32 = 32.0;
pub const LABEL_SIZE: f32 = 28.0;
pub const CAPTION_SIZE: f32 = 18.0;
pub const TIME_SIZE: f32 = 112.0;
pub const BODY_SIZE: f32 = 18.0;

pub const SPACING: f32 = 12.0;
pub const SECTION_SPACING: f32 = 48.0;
pub const FORM_WIDTH: f32 = 440.0;
pub const FIELD_PADDING: f32 = 12.0;
pub const BUTTON_PADDING: [f32; 2] = [10.0, 20.0];

pub fn theme() -> Theme {
    Theme::custom(
        "Selflow",
        Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT,
            success: ACCENT,
            warning: ACCENT,
            danger: ACCENT,
        },
    )
}
