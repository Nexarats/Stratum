//! Theme engine — built-in and custom terminal color schemes.
//!
//! Each theme defines the 16 ANSI colors, default foreground/background,
//! cursor color, and selection highlight color. Themes are referenced by
//! name in the config file.

use crate::screen::Color;

/// A complete terminal color theme.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme display name.
    pub name: &'static str,
    /// Default foreground (text) color as RGBA.
    pub fg: [f32; 4],
    /// Default background color as RGBA.
    pub bg: [f32; 4],
    /// Cursor color as RGBA.
    pub cursor: [f32; 4],
    /// Selection highlight background as RGBA (semi-transparent).
    pub selection_bg: [f32; 4],
    /// Selection foreground override (or None to keep original fg).
    pub selection_fg: Option<[f32; 4]>,
    /// The 16 ANSI colors (0=black, 1=red, ... 7=white, 8-15=bright variants).
    pub ansi: [[f32; 4]; 16],
}

impl Theme {
    /// Resolve a `Color` to RGBA using this theme for foreground.
    pub fn resolve_fg(&self, color: Color) -> [f32; 4] {
        match color {
            Color::Default => self.fg,
            other => self.resolve_shared(other),
        }
    }

    /// Resolve a `Color` to RGBA using this theme for background.
    pub fn resolve_bg(&self, color: Color) -> [f32; 4] {
        match color {
            Color::Default => self.bg,
            other => self.resolve_shared(other),
        }
    }

    /// Shared color resolution for named/indexed/RGB colors.
    fn resolve_shared(&self, color: Color) -> [f32; 4] {
        match color {
            Color::Default => self.fg,
            Color::Black => self.ansi[0],
            Color::Red => self.ansi[1],
            Color::Green => self.ansi[2],
            Color::Yellow => self.ansi[3],
            Color::Blue => self.ansi[4],
            Color::Magenta => self.ansi[5],
            Color::Cyan => self.ansi[6],
            Color::White => self.ansi[7],
            Color::BrightBlack => self.ansi[8],
            Color::BrightRed => self.ansi[9],
            Color::BrightGreen => self.ansi[10],
            Color::BrightYellow => self.ansi[11],
            Color::BrightBlue => self.ansi[12],
            Color::BrightMagenta => self.ansi[13],
            Color::BrightCyan => self.ansi[14],
            Color::BrightWhite => self.ansi[15],
            Color::Rgb(r, g, b) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0],
            Color::Indexed(i) => self.resolve_indexed(i),
        }
    }

    /// Resolve a 256-color index.
    fn resolve_indexed(&self, idx: u8) -> [f32; 4] {
        match idx {
            0..=15 => self.ansi[idx as usize],
            16..=231 => {
                let n = idx - 16;
                let r = (n / 36) as f32;
                let g = ((n % 36) / 6) as f32;
                let b = (n % 6) as f32;
                [
                    if r > 0.0 { (r * 40.0 + 55.0) / 255.0 } else { 0.0 },
                    if g > 0.0 { (g * 40.0 + 55.0) / 255.0 } else { 0.0 },
                    if b > 0.0 { (b * 40.0 + 55.0) / 255.0 } else { 0.0 },
                    1.0,
                ]
            }
            232..=255 => {
                let v = (8 + 10 * (idx - 232) as u32) as f32 / 255.0;
                [v, v, v, 1.0]
            }
        }
    }
}

// =============================================================================
// Built-in Themes
// =============================================================================

/// Stratum — the default theme. Clean black background, white text, warm accents.
pub const STRATUM: Theme = Theme {
    name: "stratum",
    fg: [1.00, 1.00, 1.00, 1.0],       // #FFFFFF — pure white
    bg: [0.00, 0.00, 0.00, 1.0],       // #000000 — pure black
    cursor: [0.90, 0.90, 0.90, 1.0],   // #E6E6E6 — bright gray cursor
    selection_bg: [0.30, 0.30, 0.35, 0.50], // Subtle gray highlight
    selection_fg: None,
    ansi: [
        [0.07, 0.07, 0.07, 1.0],       //  0 Black    #121212
        [0.90, 0.27, 0.27, 1.0],       //  1 Red      #E64545
        [0.30, 0.82, 0.40, 1.0],       //  2 Green    #4DD166
        [0.90, 0.75, 0.30, 1.0],       //  3 Yellow   #E6BF4D (warm amber/orange)
        [0.35, 0.57, 0.93, 1.0],       //  4 Blue     #5992ED
        [0.72, 0.40, 0.90, 1.0],       //  5 Magenta  #B866E6
        [0.30, 0.78, 0.80, 1.0],       //  6 Cyan     #4DC7CC
        [0.85, 0.85, 0.85, 1.0],       //  7 White    #D9D9D9
        [0.40, 0.40, 0.40, 1.0],       //  8 Bright Black   #666666
        [1.00, 0.40, 0.40, 1.0],       //  9 Bright Red     #FF6666
        [0.40, 0.95, 0.53, 1.0],       // 10 Bright Green   #66F287
        [1.00, 0.85, 0.40, 1.0],       // 11 Bright Yellow  #FFD966 (warm orange)
        [0.50, 0.70, 1.00, 1.0],       // 12 Bright Blue    #80B3FF
        [0.83, 0.55, 1.00, 1.0],       // 13 Bright Magenta #D48CFF
        [0.40, 0.90, 0.93, 1.0],       // 14 Bright Cyan    #66E6ED
        [1.00, 1.00, 1.00, 1.0],       // 15 Bright White   #FFFFFF
    ],
};

/// Stratum Dark — deep navy with vibrant accents.
pub const STRATUM_DARK: Theme = Theme {
    name: "stratum-dark",
    fg: [0.85, 0.87, 0.91, 1.0],       // #D9DEE8
    bg: [0.08, 0.08, 0.12, 1.0],       // #14141F
    cursor: [0.40, 0.72, 1.0, 1.0],    // #66B8FF — electric blue
    selection_bg: [0.25, 0.45, 0.80, 0.35], // Blue highlight, semi-transparent
    selection_fg: None,
    ansi: [
        [0.12, 0.12, 0.18, 1.0],       //  0 Black    #1E1E2E
        [0.95, 0.30, 0.35, 1.0],       //  1 Red      #F24D59
        [0.35, 0.90, 0.50, 1.0],       //  2 Green    #59E680
        [0.95, 0.82, 0.30, 1.0],       //  3 Yellow   #F2D14D
        [0.35, 0.55, 0.95, 1.0],       //  4 Blue     #598CF2
        [0.75, 0.40, 0.95, 1.0],       //  5 Magenta  #BF66F2
        [0.30, 0.82, 0.85, 1.0],       //  6 Cyan     #4DD1D9
        [0.78, 0.80, 0.85, 1.0],       //  7 White    #C7CCD9
        [0.38, 0.40, 0.50, 1.0],       //  8 Bright Black   #616680
        [1.00, 0.42, 0.45, 1.0],       //  9 Bright Red     #FF6B73
        [0.45, 1.00, 0.60, 1.0],       // 10 Bright Green   #73FF99
        [1.00, 0.90, 0.40, 1.0],       // 11 Bright Yellow  #FFE566
        [0.50, 0.70, 1.00, 1.0],       // 12 Bright Blue    #80B3FF
        [0.85, 0.55, 1.00, 1.0],       // 13 Bright Magenta #D98CFF
        [0.40, 0.92, 0.95, 1.0],       // 14 Bright Cyan    #66EBF2
        [0.92, 0.94, 0.98, 1.0],       // 15 Bright White   #EBF0FA
    ],
};

/// Monokai Pro — warm, high-contrast.
pub const MONOKAI: Theme = Theme {
    name: "monokai",
    fg: [0.97, 0.97, 0.95, 1.0],       // #F8F8F2
    bg: [0.16, 0.16, 0.16, 1.0],       // #282828
    cursor: [0.97, 0.97, 0.95, 1.0],   // #F8F8F2
    selection_bg: [0.27, 0.27, 0.27, 0.50],
    selection_fg: None,
    ansi: [
        [0.16, 0.16, 0.16, 1.0],       //  0 Black
        [1.00, 0.38, 0.36, 1.0],       //  1 Red       #FF615A
        [0.65, 0.89, 0.18, 1.0],       //  2 Green     #A6E22E
        [0.90, 0.86, 0.45, 1.0],       //  3 Yellow    #E6DB74
        [0.40, 0.85, 0.94, 1.0],       //  4 Blue      #66D9EF
        [0.68, 0.51, 1.00, 1.0],       //  5 Magenta   #AE81FF
        [0.65, 0.89, 0.18, 1.0],       //  6 Cyan      (same as green)
        [0.97, 0.97, 0.95, 1.0],       //  7 White
        [0.46, 0.44, 0.40, 1.0],       //  8 Bright Black
        [1.00, 0.38, 0.36, 1.0],       //  9 Bright Red
        [0.65, 0.89, 0.18, 1.0],       // 10 Bright Green
        [0.90, 0.86, 0.45, 1.0],       // 11 Bright Yellow
        [0.40, 0.85, 0.94, 1.0],       // 12 Bright Blue
        [0.68, 0.51, 1.00, 1.0],       // 13 Bright Magenta
        [0.65, 0.89, 0.18, 1.0],       // 14 Bright Cyan
        [0.97, 0.97, 0.95, 1.0],       // 15 Bright White
    ],
};

/// Dracula — purple-accented dark theme.
pub const DRACULA: Theme = Theme {
    name: "dracula",
    fg: [0.97, 0.97, 0.95, 1.0],       // #F8F8F2
    bg: [0.16, 0.16, 0.21, 1.0],       // #282A36
    cursor: [0.97, 0.97, 0.95, 1.0],
    selection_bg: [0.27, 0.28, 0.35, 0.50],
    selection_fg: None,
    ansi: [
        [0.13, 0.14, 0.18, 1.0],       //  0 Black    #21222C
        [1.00, 0.33, 0.33, 1.0],       //  1 Red      #FF5555
        [0.31, 0.98, 0.48, 1.0],       //  2 Green    #50FA7B
        [0.94, 0.90, 0.55, 1.0],       //  3 Yellow   #F1FA8C
        [0.74, 0.58, 0.98, 1.0],       //  4 Blue     #BD93F9
        [1.00, 0.47, 0.66, 1.0],       //  5 Magenta  #FF79C6
        [0.55, 0.91, 0.99, 1.0],       //  6 Cyan     #8BE9FD
        [0.97, 0.97, 0.95, 1.0],       //  7 White    #F8F8F2
        [0.38, 0.39, 0.50, 1.0],       //  8 Bright Black  #6272A4
        [1.00, 0.44, 0.44, 1.0],       //  9 Bright Red
        [0.41, 0.98, 0.58, 1.0],       // 10 Bright Green
        [0.94, 1.00, 0.65, 1.0],       // 11 Bright Yellow
        [0.82, 0.68, 0.98, 1.0],       // 12 Bright Blue
        [1.00, 0.57, 0.76, 1.0],       // 13 Bright Magenta
        [0.65, 0.95, 0.99, 1.0],       // 14 Bright Cyan
        [1.00, 1.00, 1.00, 1.0],       // 15 Bright White
    ],
};

/// Nord — Arctic, cool-toned theme.
pub const NORD: Theme = Theme {
    name: "nord",
    fg: [0.85, 0.87, 0.91, 1.0],       // #D8DEE9
    bg: [0.18, 0.20, 0.25, 1.0],       // #2E3440
    cursor: [0.81, 0.87, 0.96, 1.0],   // #D8DEF5
    selection_bg: [0.26, 0.30, 0.37, 0.50],
    selection_fg: None,
    ansi: [
        [0.23, 0.26, 0.32, 1.0],       //  0 Black    #3B4252
        [0.75, 0.38, 0.42, 1.0],       //  1 Red      #BF616A
        [0.64, 0.75, 0.55, 1.0],       //  2 Green    #A3BE8C
        [0.92, 0.80, 0.55, 1.0],       //  3 Yellow   #EBCB8B
        [0.51, 0.63, 0.76, 1.0],       //  4 Blue     #81A1C1
        [0.71, 0.56, 0.68, 1.0],       //  5 Magenta  #B48EAD
        [0.53, 0.75, 0.82, 1.0],       //  6 Cyan     #88C0D0
        [0.91, 0.93, 0.96, 1.0],       //  7 White    #E5ECEF
        [0.30, 0.34, 0.42, 1.0],       //  8 Bright Black  #4C566A
        [0.75, 0.38, 0.42, 1.0],       //  9 Bright Red
        [0.64, 0.75, 0.55, 1.0],       // 10 Bright Green
        [0.92, 0.80, 0.55, 1.0],       // 11 Bright Yellow
        [0.51, 0.63, 0.76, 1.0],       // 12 Bright Blue
        [0.71, 0.56, 0.68, 1.0],       // 13 Bright Magenta
        [0.56, 0.74, 0.73, 1.0],       // 14 Bright Cyan   #8FBCBB
        [0.93, 0.95, 0.97, 1.0],       // 15 Bright White  #ECEFF4
    ],
};

/// Stratum Light — bright, clean theme for daytime use.
pub const STRATUM_LIGHT: Theme = Theme {
    name: "stratum-light",
    fg: [0.15, 0.15, 0.20, 1.0],       // #262633
    bg: [0.97, 0.97, 0.98, 1.0],       // #F8F8FA
    cursor: [0.20, 0.50, 0.90, 1.0],   // #3380E6
    selection_bg: [0.20, 0.50, 0.90, 0.20],
    selection_fg: None,
    ansi: [
        [0.25, 0.25, 0.30, 1.0],       //  0 Black
        [0.80, 0.15, 0.20, 1.0],       //  1 Red
        [0.15, 0.60, 0.25, 1.0],       //  2 Green
        [0.70, 0.55, 0.05, 1.0],       //  3 Yellow
        [0.15, 0.35, 0.80, 1.0],       //  4 Blue
        [0.55, 0.20, 0.75, 1.0],       //  5 Magenta
        [0.10, 0.55, 0.60, 1.0],       //  6 Cyan
        [0.60, 0.60, 0.65, 1.0],       //  7 White
        [0.45, 0.45, 0.50, 1.0],       //  8 Bright Black
        [0.90, 0.25, 0.30, 1.0],       //  9 Bright Red
        [0.20, 0.70, 0.35, 1.0],       // 10 Bright Green
        [0.80, 0.65, 0.10, 1.0],       // 11 Bright Yellow
        [0.25, 0.45, 0.90, 1.0],       // 12 Bright Blue
        [0.65, 0.30, 0.85, 1.0],       // 13 Bright Magenta
        [0.15, 0.65, 0.70, 1.0],       // 14 Bright Cyan
        [0.30, 0.30, 0.35, 1.0],       // 15 Bright White
    ],
};

/// Solarized Dark — precise LAB perceptual color scheme.
pub const SOLARIZED_DARK: Theme = Theme {
    name: "solarized-dark",
    fg: [0.51, 0.58, 0.59, 1.0],       // #839496
    bg: [0.00, 0.17, 0.21, 1.0],       // #002B36
    cursor: [0.58, 0.63, 0.63, 1.0],
    selection_bg: [0.07, 0.26, 0.30, 0.50],
    selection_fg: None,
    ansi: [
        [0.03, 0.21, 0.26, 1.0],       //  0 Black    #073642
        [0.86, 0.20, 0.18, 1.0],       //  1 Red      #DC322F
        [0.52, 0.60, 0.00, 1.0],       //  2 Green    #859900
        [0.71, 0.54, 0.00, 1.0],       //  3 Yellow   #B58900
        [0.15, 0.55, 0.82, 1.0],       //  4 Blue     #268BD2
        [0.83, 0.21, 0.51, 1.0],       //  5 Magenta  #D33682
        [0.16, 0.63, 0.60, 1.0],       //  6 Cyan     #2AA198
        [0.93, 0.91, 0.84, 1.0],       //  7 White    #EEE8D5
        [0.00, 0.27, 0.33, 1.0],       //  8 Bright Black  #002B36 (base03)
        [0.80, 0.29, 0.09, 1.0],       //  9 Bright Red (orange) #CB4B16
        [0.40, 0.48, 0.51, 1.0],       // 10 Bright Green (base01)
        [0.35, 0.43, 0.46, 1.0],       // 11 Bright Yellow (base00)
        [0.51, 0.58, 0.59, 1.0],       // 12 Bright Blue (base0)
        [0.42, 0.44, 0.77, 1.0],       // 13 Bright Magenta (violet) #6C71C4
        [0.58, 0.63, 0.63, 1.0],       // 14 Bright Cyan (base1)
        [0.99, 0.96, 0.89, 1.0],       // 15 Bright White  #FDF6E3
    ],
};

/// All built-in themes.
pub const ALL_THEMES: &[&Theme] = &[
    &STRATUM,
    &STRATUM_DARK,
    &STRATUM_LIGHT,
    &MONOKAI,
    &DRACULA,
    &NORD,
    &SOLARIZED_DARK,
];

/// Look up a theme by name (case-insensitive). Falls back to STRATUM_DARK.
pub fn get_theme(name: &str) -> &'static Theme {
    let lower = name.to_lowercase();
    for theme in ALL_THEMES {
        if theme.name == lower {
            return theme;
        }
    }
    // Also support aliases
    match lower.as_str() {
        "default" | "classic" => &STRATUM,
        "dark" => &STRATUM_DARK,
        "light" => &STRATUM_LIGHT,
        "solarized" => &SOLARIZED_DARK,
        _ => &STRATUM,
    }
}

/// List all available theme names.
pub fn theme_names() -> Vec<&'static str> {
    ALL_THEMES.iter().map(|t| t.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_theme_by_name() {
        assert_eq!(get_theme("stratum").name, "stratum");
        assert_eq!(get_theme("stratum-dark").name, "stratum-dark");
        assert_eq!(get_theme("monokai").name, "monokai");
        assert_eq!(get_theme("dracula").name, "dracula");
        assert_eq!(get_theme("nord").name, "nord");
        assert_eq!(get_theme("stratum-light").name, "stratum-light");
        assert_eq!(get_theme("solarized-dark").name, "solarized-dark");
    }

    #[test]
    fn test_aliases() {
        assert_eq!(get_theme("default").name, "stratum");
        assert_eq!(get_theme("classic").name, "stratum");
        assert_eq!(get_theme("dark").name, "stratum-dark");
        assert_eq!(get_theme("light").name, "stratum-light");
        assert_eq!(get_theme("solarized").name, "solarized-dark");
    }

    #[test]
    fn test_fallback() {
        assert_eq!(get_theme("nonexistent").name, "stratum");
        assert_eq!(get_theme("").name, "stratum");
    }

    #[test]
    fn test_resolve_fg_default() {
        let theme = &STRATUM_DARK;
        let rgba = theme.resolve_fg(Color::Default);
        assert_eq!(rgba, theme.fg);
    }

    #[test]
    fn test_resolve_bg_default() {
        let theme = &STRATUM_DARK;
        let rgba = theme.resolve_bg(Color::Default);
        assert_eq!(rgba, theme.bg);
    }

    #[test]
    fn test_resolve_ansi_color() {
        let theme = &DRACULA;
        let red = theme.resolve_fg(Color::Red);
        assert_eq!(red, theme.ansi[1]);
    }

    #[test]
    fn test_resolve_rgb() {
        let theme = &STRATUM_DARK;
        let color = theme.resolve_fg(Color::Rgb(128, 64, 255));
        assert!((color[0] - 128.0 / 255.0).abs() < 0.01);
        assert!((color[1] - 64.0 / 255.0).abs() < 0.01);
        assert!((color[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_theme_names() {
        let names = theme_names();
        assert_eq!(names.len(), 7);
        assert!(names.contains(&"stratum"));
        assert!(names.contains(&"stratum-dark"));
        assert!(names.contains(&"dracula"));
    }
}
