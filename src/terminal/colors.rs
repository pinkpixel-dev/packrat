//! Maps terminal colors to GPUI colors.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use gpui_kit::{Hsla, Rgba};

/// The 16 ANSI colors, tuned to sit on Packrat Dark's charcoal background.
const ANSI: [u32; 16] = [
    0x2b2e32, 0xe66e68, 0x6cc581, 0xe3b55f, 0x59b4dd, 0xaf95df, 0x6ccac4, 0xc9cdd1, // normal
    0x5c6166, 0xf08a84, 0x8bd89d, 0xf0cb7f, 0x7cc6e8, 0xc4aeea, 0x8fdad5, 0xf0f2f4, // bright
];
const FOREGROUND: u32 = 0xe5e8eb;
const BACKGROUND: u32 = 0x131518;
const CURSOR: u32 = 0x59b4dd;

/// The pane's background, a step darker than the window.
pub fn background() -> Hsla {
    hex(BACKGROUND)
}

fn hex(value: u32) -> Hsla {
    rgb(Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    })
}

fn rgb(color: Rgb) -> Hsla {
    Rgba {
        r: f32::from(color.r) / 255.,
        g: f32::from(color.g) / 255.,
        b: f32::from(color.b) / 255.,
        a: 1.,
    }
    .into()
}

/// The xterm 256-color palette entry for an index.
fn indexed(index: u8) -> Hsla {
    match index {
        0..=15 => hex(ANSI[index as usize]),
        16..=231 => {
            let ix = index - 16;
            let level = |value: u8| if value == 0 { 0 } else { value * 40 + 55 };
            rgb(Rgb {
                r: level(ix / 36),
                g: level((ix / 6) % 6),
                b: level(ix % 6),
            })
        }
        232..=255 => {
            let gray = (index - 232) * 10 + 8;
            rgb(Rgb { r: gray, g: gray, b: gray })
        }
    }
}

/// Resolves a cell color. Colors a program set through escape codes (stored
/// in `overrides`) win over the built-in palette.
pub fn resolve(color: Color, overrides: &Colors) -> Hsla {
    match color {
        Color::Spec(spec) => rgb(spec),
        Color::Indexed(index) => overrides[index as usize].map(rgb).unwrap_or_else(|| indexed(index)),
        Color::Named(name) => overrides[name].map(rgb).unwrap_or_else(|| named(name)),
    }
}

fn named(name: NamedColor) -> Hsla {
    match name {
        NamedColor::Foreground | NamedColor::BrightForeground => hex(FOREGROUND),
        NamedColor::Background => hex(BACKGROUND),
        NamedColor::Cursor => hex(CURSOR),
        NamedColor::DimForeground => hex(FOREGROUND).opacity(0.6),
        NamedColor::DimBlack => hex(ANSI[0]).opacity(0.7),
        NamedColor::DimRed => hex(ANSI[1]).opacity(0.7),
        NamedColor::DimGreen => hex(ANSI[2]).opacity(0.7),
        NamedColor::DimYellow => hex(ANSI[3]).opacity(0.7),
        NamedColor::DimBlue => hex(ANSI[4]).opacity(0.7),
        NamedColor::DimMagenta => hex(ANSI[5]).opacity(0.7),
        NamedColor::DimCyan => hex(ANSI[6]).opacity(0.7),
        NamedColor::DimWhite => hex(ANSI[7]).opacity(0.7),
        other => indexed(other as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_palette_ranges() {
        assert_eq!(indexed(1), hex(ANSI[1]));
        // 16 is the cube's black, 231 its white.
        assert_eq!(indexed(16), hex(0x000000));
        assert_eq!(indexed(231), hex(0xffffff));
        assert_eq!(indexed(232), hex(0x080808));
    }

    #[test]
    fn overrides_win() {
        let mut colors = Colors::default();
        assert_eq!(resolve(Color::Named(NamedColor::Red), &colors), hex(ANSI[1]));
        colors[NamedColor::Red] = Some(Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(resolve(Color::Named(NamedColor::Red), &colors), hex(0x010203));
    }
}
