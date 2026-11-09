use super::{Bevel, EffectSet, FontKind, TitleBarStyle};
use sdl2::pixels::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Win95,
    Win98,
    WinXpLuna,
    Win7Aero,
    ModernLight,
    ModernDark,
}

impl ThemeId {
    pub const ALL: [ThemeId; 6] = [
        ThemeId::Win95,
        ThemeId::Win98,
        ThemeId::WinXpLuna,
        ThemeId::Win7Aero,
        ThemeId::ModernLight,
        ThemeId::ModernDark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Win95 => "Windows 95",
            ThemeId::Win98 => "Windows 98",
            ThemeId::WinXpLuna => "Windows XP Luna",
            ThemeId::Win7Aero => "Windows 7 Aero",
            ThemeId::ModernLight => "Modern Light",
            ThemeId::ModernDark => "Modern Dark",
        }
    }

    /// Stable prefs key for `.ffini`.
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Win95 => "win95",
            ThemeId::Win98 => "win98",
            ThemeId::WinXpLuna => "winxp_luna",
            ThemeId::Win7Aero => "win7_aero",
            ThemeId::ModernLight => "modern_light",
            ThemeId::ModernDark => "modern_dark",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "win95" => Some(ThemeId::Win95),
            "win98" => Some(ThemeId::Win98),
            "winxp_luna" | "luna" => Some(ThemeId::WinXpLuna),
            "win7_aero" => Some(ThemeId::Win7Aero),
            "modern_light" => Some(ThemeId::ModernLight),
            "modern_dark" => Some(ThemeId::ModernDark),
            _ => None,
        }
    }

    pub fn next(self) -> ThemeId {
        match self {
            ThemeId::Win95 => ThemeId::Win98,
            ThemeId::Win98 => ThemeId::WinXpLuna,
            ThemeId::WinXpLuna => ThemeId::Win7Aero,
            ThemeId::Win7Aero => ThemeId::ModernLight,
            ThemeId::ModernLight => ThemeId::ModernDark,
            ThemeId::ModernDark => ThemeId::Win95,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub id: ThemeId,
    #[allow(dead_code)]
    pub name: &'static str,
    pub font: FontKind,
    pub desktop: Color,
    pub window: Color,
    pub client: Color,
    pub text: Color,
    pub text_disabled: Color,
    pub selection: Color,
    pub selection_text: Color,
    pub border: Color,
    pub chrome: Bevel,
    pub button: Bevel,
    pub sunken: Bevel,
    pub title: TitleBarStyle,
    pub status_bar: Color,
    pub address_bar: Color,
    #[allow(dead_code)]
    pub scrollbar: Color,
    pub icon_folder: Color,
    pub icon_file: Color,
    pub row_height: i32,
    pub title_height: i32,
    pub toolbar_height: i32,
    pub status_height: i32,
    pub border_width: i32,
    pub font_size: f32,
    /// Effects this theme wants (gated at draw time by settings).
    pub effects: EffectSet,
}

pub fn theme(id: ThemeId) -> Theme {
    match id {
        ThemeId::Win95 => WIN95,
        ThemeId::Win98 => WIN98,
        ThemeId::WinXpLuna => WINXP_LUNA,
        ThemeId::Win7Aero => WIN7_AERO,
        ThemeId::ModernLight => MODERN_LIGHT,
        ThemeId::ModernDark => MODERN_DARK,
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::RGB(r, g, b)
}

const WIN95_FACE: Color = rgb(0xC0, 0xC0, 0xC0);
const WIN95_HIGHLIGHT: Color = rgb(0xFF, 0xFF, 0xFF);
const WIN95_SHADOW: Color = rgb(0x80, 0x80, 0x80);
const WIN95_DARK: Color = rgb(0x00, 0x00, 0x00);
const WIN95_NAVY: Color = rgb(0x00, 0x00, 0x80);

const WIN95: Theme = Theme {
    id: ThemeId::Win95,
    name: "Windows 95",
    font: FontKind::ClassicBitmap,
    desktop: rgb(0x00, 0x80, 0x80),
    window: WIN95_FACE,
    client: rgb(0xFF, 0xFF, 0xFF),
    text: WIN95_DARK,
    text_disabled: WIN95_SHADOW,
    selection: WIN95_NAVY,
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: WIN95_DARK,
    chrome: Bevel {
        face: WIN95_FACE,
        highlight: WIN95_HIGHLIGHT,
        shadow: WIN95_SHADOW,
        dark_shadow: WIN95_DARK,
    },
    button: Bevel {
        face: WIN95_FACE,
        highlight: WIN95_HIGHLIGHT,
        shadow: WIN95_SHADOW,
        dark_shadow: WIN95_DARK,
    },
    sunken: Bevel {
        face: rgb(0xFF, 0xFF, 0xFF),
        highlight: WIN95_SHADOW,
        shadow: WIN95_HIGHLIGHT,
        dark_shadow: WIN95_FACE,
    },
    title: TitleBarStyle {
        active_left: WIN95_NAVY,
        active_right: WIN95_NAVY,
        inactive_left: WIN95_SHADOW,
        inactive_right: WIN95_SHADOW,
        text: rgb(0xFF, 0xFF, 0xFF),
        inactive_text: WIN95_FACE,
        gradient: false,
    },
    status_bar: WIN95_FACE,
    address_bar: rgb(0xFF, 0xFF, 0xFF),
    scrollbar: WIN95_FACE,
    icon_folder: rgb(0xFF, 0xFF, 0x00),
    icon_file: rgb(0xFF, 0xFF, 0xFF),
    row_height: 18,
    title_height: 18,
    toolbar_height: 28,
    status_height: 22,
    border_width: 3,
    font_size: 13.0,
    effects: EffectSet::none(),
};

const WIN98: Theme = Theme {
    id: ThemeId::Win98,
    name: "Windows 98",
    font: FontKind::ClassicBitmap,
    desktop: rgb(0x00, 0x80, 0x80),
    window: WIN95_FACE,
    client: rgb(0xFF, 0xFF, 0xFF),
    text: WIN95_DARK,
    text_disabled: WIN95_SHADOW,
    selection: WIN95_NAVY,
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: WIN95_DARK,
    chrome: Bevel {
        face: WIN95_FACE,
        highlight: WIN95_HIGHLIGHT,
        shadow: WIN95_SHADOW,
        dark_shadow: WIN95_DARK,
    },
    button: Bevel {
        face: WIN95_FACE,
        highlight: WIN95_HIGHLIGHT,
        shadow: WIN95_SHADOW,
        dark_shadow: WIN95_DARK,
    },
    sunken: Bevel {
        face: rgb(0xFF, 0xFF, 0xFF),
        highlight: WIN95_SHADOW,
        shadow: WIN95_HIGHLIGHT,
        dark_shadow: WIN95_FACE,
    },
    title: TitleBarStyle {
        active_left: WIN95_NAVY,
        active_right: rgb(0x10, 0x84, 0xD0),
        inactive_left: WIN95_SHADOW,
        inactive_right: rgb(0xB5, 0xB5, 0xB5),
        text: rgb(0xFF, 0xFF, 0xFF),
        inactive_text: WIN95_FACE,
        gradient: true,
    },
    status_bar: WIN95_FACE,
    address_bar: rgb(0xFF, 0xFF, 0xFF),
    scrollbar: WIN95_FACE,
    icon_folder: rgb(0xFF, 0xFF, 0x80),
    icon_file: rgb(0xFF, 0xFF, 0xFF),
    row_height: 18,
    title_height: 18,
    toolbar_height: 28,
    status_height: 22,
    border_width: 3,
    font_size: 13.0,
    effects: EffectSet::classic(),
};

/// Windows XP Luna Blue — olive chrome, blue caption with gloss.
const WINXP_LUNA: Theme = Theme {
    id: ThemeId::WinXpLuna,
    name: "Windows XP Luna",
    font: FontKind::Sans,
    desktop: rgb(0x00, 0x4E, 0x98),
    window: rgb(0xEC, 0xE9, 0xD8),
    client: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x00, 0x00, 0x00),
    text_disabled: rgb(0x80, 0x80, 0x80),
    selection: rgb(0x31, 0x6A, 0xC5),
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: rgb(0x00, 0x5A, 0xD6),
    chrome: Bevel {
        face: rgb(0xEC, 0xE9, 0xD8),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xAC, 0xA8, 0x99),
        dark_shadow: rgb(0x71, 0x6F, 0x64),
    },
    button: Bevel {
        face: rgb(0xEC, 0xE9, 0xD8),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xAC, 0xA8, 0x99),
        dark_shadow: rgb(0x71, 0x6F, 0x64),
    },
    sunken: Bevel {
        face: rgb(0xFF, 0xFF, 0xFF),
        highlight: rgb(0xAC, 0xA8, 0x99),
        shadow: rgb(0xFF, 0xFF, 0xFF),
        dark_shadow: rgb(0xEC, 0xE9, 0xD8),
    },
    title: TitleBarStyle {
        // Luna Blue caption: deep royal → sky
        active_left: rgb(0x0A, 0x24, 0x6A),
        active_right: rgb(0xA6, 0xCA, 0xF0),
        inactive_left: rgb(0x7A, 0x96, 0xDF),
        inactive_right: rgb(0xC3, 0xD5, 0xF0),
        text: rgb(0xFF, 0xFF, 0xFF),
        inactive_text: rgb(0xD0, 0xDC, 0xF0),
        gradient: true,
    },
    status_bar: rgb(0xEC, 0xE9, 0xD8),
    address_bar: rgb(0xFF, 0xFF, 0xFF),
    scrollbar: rgb(0xEC, 0xE9, 0xD8),
    icon_folder: rgb(0xF2, 0xC8, 0x4A),
    icon_file: rgb(0xFF, 0xFF, 0xFF),
    row_height: 20,
    title_height: 26,
    toolbar_height: 30,
    status_height: 22,
    border_width: 3,
    font_size: 13.0,
    effects: EffectSet::luna(),
};

/// Soft glass-inspired Win7 Aero (opaque approximation — no real blur yet).
const WIN7_AERO: Theme = Theme {
    id: ThemeId::Win7Aero,
    name: "Windows 7 Aero",
    font: FontKind::Sans,
    desktop: rgb(0x3A, 0x6E, 0xA5),
    window: rgb(0xE8, 0xEE, 0xF5),
    client: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x1E, 0x1E, 0x1E),
    text_disabled: rgb(0x8A, 0x8A, 0x8A),
    selection: rgb(0x33, 0x99, 0xFF),
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: rgb(0x6A, 0x9C, 0xD3),
    chrome: Bevel {
        face: rgb(0xE3, 0xEA, 0xF2),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xA8, 0xBE, 0xD4),
        dark_shadow: rgb(0x6A, 0x8A, 0xA8),
    },
    button: Bevel {
        face: rgb(0xF0, 0xF4, 0xF9),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xB0, 0xC4, 0xD8),
        dark_shadow: rgb(0x7A, 0x96, 0xB0),
    },
    sunken: Bevel {
        face: rgb(0xFF, 0xFF, 0xFF),
        highlight: rgb(0xA8, 0xBE, 0xD4),
        shadow: rgb(0xFF, 0xFF, 0xFF),
        dark_shadow: rgb(0xE0, 0xE8, 0xF0),
    },
    title: TitleBarStyle {
        // Glass bar: pale sky → soft Aero blue
        active_left: rgb(0xD6, 0xE8, 0xF8),
        active_right: rgb(0x5B, 0x9B, 0xD5),
        inactive_left: rgb(0xE0, 0xE4, 0xE8),
        inactive_right: rgb(0xB0, 0xB8, 0xC0),
        text: rgb(0x1A, 0x33, 0x55),
        inactive_text: rgb(0x60, 0x68, 0x70),
        gradient: true,
    },
    status_bar: rgb(0xE8, 0xEE, 0xF5),
    address_bar: rgb(0xFF, 0xFF, 0xFF),
    scrollbar: rgb(0xD0, 0xDC, 0xE8),
    icon_folder: rgb(0xF2, 0xC8, 0x4A),
    icon_file: rgb(0x6B, 0xA3, 0xD8),
    row_height: 22,
    title_height: 28,
    toolbar_height: 32,
    status_height: 24,
    border_width: 2,
    font_size: 14.0,
    effects: EffectSet::aero(),
};

const MODERN_LIGHT: Theme = Theme {
    id: ThemeId::ModernLight,
    name: "Modern Light",
    font: FontKind::Sans,
    desktop: rgb(0xE8, 0xEA, 0xED),
    window: rgb(0xF5, 0xF5, 0xF5),
    client: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x1F, 0x1F, 0x1F),
    text_disabled: rgb(0x9A, 0x9A, 0x9A),
    selection: rgb(0x00, 0x78, 0xD4),
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: rgb(0xC8, 0xC8, 0xC8),
    chrome: Bevel {
        face: rgb(0xF5, 0xF5, 0xF5),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xD0, 0xD0, 0xD0),
        dark_shadow: rgb(0xA0, 0xA0, 0xA0),
    },
    button: Bevel {
        face: rgb(0xF0, 0xF0, 0xF0),
        highlight: rgb(0xFF, 0xFF, 0xFF),
        shadow: rgb(0xC0, 0xC0, 0xC0),
        dark_shadow: rgb(0x90, 0x90, 0x90),
    },
    sunken: Bevel {
        face: rgb(0xFF, 0xFF, 0xFF),
        highlight: rgb(0xD0, 0xD0, 0xD0),
        shadow: rgb(0xFF, 0xFF, 0xFF),
        dark_shadow: rgb(0xE8, 0xE8, 0xE8),
    },
    title: TitleBarStyle {
        active_left: rgb(0xFF, 0xFF, 0xFF),
        active_right: rgb(0xFF, 0xFF, 0xFF),
        inactive_left: rgb(0xF0, 0xF0, 0xF0),
        inactive_right: rgb(0xF0, 0xF0, 0xF0),
        text: rgb(0x1F, 0x1F, 0x1F),
        inactive_text: rgb(0x80, 0x80, 0x80),
        gradient: false,
    },
    status_bar: rgb(0xF5, 0xF5, 0xF5),
    address_bar: rgb(0xFF, 0xFF, 0xFF),
    scrollbar: rgb(0xE0, 0xE0, 0xE0),
    icon_folder: rgb(0xDC, 0xB6, 0x47),
    icon_file: rgb(0x6B, 0x9B, 0xD1),
    row_height: 24,
    title_height: 32,
    toolbar_height: 36,
    status_height: 24,
    border_width: 1,
    font_size: 15.0,
    effects: EffectSet::modern(),
};

const MODERN_DARK: Theme = Theme {
    id: ThemeId::ModernDark,
    name: "Modern Dark",
    font: FontKind::Sans,
    desktop: rgb(0x20, 0x20, 0x20),
    window: rgb(0x2D, 0x2D, 0x2D),
    client: rgb(0x1E, 0x1E, 0x1E),
    text: rgb(0xF0, 0xF0, 0xF0),
    text_disabled: rgb(0x80, 0x80, 0x80),
    selection: rgb(0x26, 0x4F, 0x78),
    selection_text: rgb(0xFF, 0xFF, 0xFF),
    border: rgb(0x3F, 0x3F, 0x3F),
    chrome: Bevel {
        face: rgb(0x2D, 0x2D, 0x2D),
        highlight: rgb(0x45, 0x45, 0x45),
        shadow: rgb(0x18, 0x18, 0x18),
        dark_shadow: rgb(0x00, 0x00, 0x00),
    },
    button: Bevel {
        face: rgb(0x3A, 0x3A, 0x3A),
        highlight: rgb(0x55, 0x55, 0x55),
        shadow: rgb(0x20, 0x20, 0x20),
        dark_shadow: rgb(0x00, 0x00, 0x00),
    },
    sunken: Bevel {
        face: rgb(0x1E, 0x1E, 0x1E),
        highlight: rgb(0x10, 0x10, 0x10),
        shadow: rgb(0x40, 0x40, 0x40),
        dark_shadow: rgb(0x2A, 0x2A, 0x2A),
    },
    title: TitleBarStyle {
        active_left: rgb(0x2D, 0x2D, 0x2D),
        active_right: rgb(0x2D, 0x2D, 0x2D),
        inactive_left: rgb(0x25, 0x25, 0x25),
        inactive_right: rgb(0x25, 0x25, 0x25),
        text: rgb(0xF0, 0xF0, 0xF0),
        inactive_text: rgb(0x90, 0x90, 0x90),
        gradient: false,
    },
    status_bar: rgb(0x2D, 0x2D, 0x2D),
    address_bar: rgb(0x1E, 0x1E, 0x1E),
    scrollbar: rgb(0x3A, 0x3A, 0x3A),
    icon_folder: rgb(0xE6, 0xC3, 0x5C),
    icon_file: rgb(0x7A, 0xA2, 0xF7),
    row_height: 24,
    title_height: 32,
    toolbar_height: 36,
    status_height: 24,
    border_width: 1,
    font_size: 15.0,
    effects: EffectSet::modern(),
};