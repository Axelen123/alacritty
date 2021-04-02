use std::boxed::Box;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::ansi::{Color, NamedColor};
use crate::grid::{self, GridCell};
use crate::index::Column;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Flags: u16 {
        const INVERSE                   = 0b0000_0000_0000_0001;
        const BOLD                      = 0b0000_0000_0000_0010;
        const ITALIC                    = 0b0000_0000_0000_0100;
        const BOLD_ITALIC               = 0b0000_0000_0000_0110;
        const UNDERLINE                 = 0b0000_0000_0000_1000;
        const WRAPLINE                  = 0b0000_0000_0001_0000;
        const WIDE_CHAR                 = 0b0000_0000_0010_0000;
        const WIDE_CHAR_SPACER          = 0b0000_0000_0100_0000;
        const DIM                       = 0b0000_0000_1000_0000;
        const DIM_BOLD                  = 0b0000_0000_1000_0010;
        const HIDDEN                    = 0b0000_0001_0000_0000;
        const STRIKEOUT                 = 0b0000_0010_0000_0000;
        const LEADING_WIDE_CHAR_SPACER  = 0b0000_0100_0000_0000;
        const DOUBLE_UNDERLINE          = 0b0000_1000_0000_0000;
    }
}

/// Trait for determining if a reset should be performed.
pub trait ResetDiscriminant<T> {
    /// Value based on which equality for the reset will be determined.
    fn discriminant(&self) -> T;
}

impl<T: Copy> ResetDiscriminant<T> for T {
    fn discriminant(&self) -> T {
        *self
    }
}

impl ResetDiscriminant<Color> for Cell {
    fn discriminant(&self) -> Color {
        self.bg
    }
}

/// Dynamically allocated cell content.
///
/// This storage is reserved for cell attributes which are rarely set. This allows reducing the
/// allocation required ahead of time for every cell, with some additional overhead when the extra
/// storage is actually required.
#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
struct CellExtra {
    zerowidth: Vec<char>,
}

/// Content and attributes of a single cell in the terminal grid.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    #[serde(default)]
    extra: Option<Box<CellExtra>>,
}

impl Default for Cell {
    #[inline]
    fn default() -> Cell {
        Cell {
            c: ' ',
            bg: Color::Named(NamedColor::Background),
            fg: Color::Named(NamedColor::Foreground),
            flags: Flags::empty(),
            extra: None,
        }
    }
}

impl Cell {
    /// Zerowidth characters stored in this cell.
    #[inline]
    pub fn zerowidth(&self) -> Option<&[char]> {
        self.extra.as_ref().map(|extra| extra.zerowidth.as_slice())
    }

    /// Write a new zerowidth character to this cell.
    #[inline]
    pub fn push_zerowidth(&mut self, c: char) {
        self.extra.get_or_insert_with(Default::default).zerowidth.push(c);
    }

    /// Free all dynamically allocated cell storage.
    #[inline]
    pub fn drop_extra(&mut self) {
        if self.extra.is_some() {
            self.extra = None;
        }
    }

    pub fn as_escape(&self, buf: &mut String, last: &Self) {
        // Always push CSI introducer since it's more efficient to truncate later
        *buf += "\x1b[";
        let empty_len = buf.len();

        self.fg.as_escape(buf, &last.fg, true);
        self.bg.as_escape(buf, &last.bg, false);

        macro_rules! csi {
            () => {{
                if buf.len() == empty_len {
                    // Remove previously added CSI introducer if nothing changed
                    buf.truncate(empty_len - 2);
                } else {
                    unsafe {
                        let last_byte = buf.len() - 1;
                        buf.as_bytes_mut()[last_byte] = b'm';
                    }
                }
            }};
        }

        if self.flags == last.flags {
            csi!();
            return;
        }

        let diff = self.flags ^ last.flags;

        if diff.intersects(Flags::BOLD | Flags::DIM) {
            if !self.flags.intersects(Flags::BOLD | Flags::DIM) {
                *buf += "22;";
            } else if self.flags.contains(Flags::BOLD) {
                *buf += "1;";
            } else {
                *buf += "2;";
            }
        }

        macro_rules! append_if_flags_differ {
            ($flag:expr, $num:literal) => {{
                if diff.contains($flag) {
                    if self.flags.contains($flag) {
                        *buf += concat!($num, ";");
                    } else {
                        *buf += concat!("2", $num, ";");
                    }
                }
            }};
        }

        append_if_flags_differ!(Flags::ITALIC, 3);
        append_if_flags_differ!(Flags::UNDERLINE, 4);
        append_if_flags_differ!(Flags::INVERSE, 7);
        append_if_flags_differ!(Flags::HIDDEN, 8);
        append_if_flags_differ!(Flags::STRIKEOUT, 9);

        csi!()
    }
}

impl GridCell for Cell {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.bg == Color::Named(NamedColor::Background)
            && self.fg == Color::Named(NamedColor::Foreground)
            && !self.flags.intersects(
                Flags::INVERSE
                    | Flags::UNDERLINE
                    | Flags::DOUBLE_UNDERLINE
                    | Flags::STRIKEOUT
                    | Flags::WRAPLINE
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER,
            )
            && self.extra.as_ref().map(|extra| extra.zerowidth.is_empty()) != Some(false)
    }

    #[inline]
    fn flags(&self) -> &Flags {
        &self.flags
    }

    #[inline]
    fn flags_mut(&mut self) -> &mut Flags {
        &mut self.flags
    }

    #[inline]
    fn reset(&mut self, template: &Self) {
        *self = Cell { bg: template.bg, ..Cell::default() };
    }
}

impl From<Color> for Cell {
    #[inline]
    fn from(color: Color) -> Self {
        Self { bg: color, ..Cell::default() }
    }
}

/// Get the length of occupied cells in a line.
pub trait LineLength {
    /// Calculate the occupied line length.
    fn line_length(&self) -> Column;
}

impl LineLength for grid::Row<Cell> {
    fn line_length(&self) -> Column {
        let mut length = Column(0);

        if self[Column(self.len() - 1)].flags.contains(Flags::WRAPLINE) {
            return Column(self.len());
        }

        for (index, cell) in self[..].iter().rev().enumerate() {
            if !cell.is_empty() {
                length = Column(self.len() - index);
                break;
            }
        }

        length
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, Flags, LineLength};

    use crate::ansi::{Color, NamedColor};
    use crate::grid::Row;
    use crate::index::Column;
    use crate::term::color::Rgb;

    #[test]
    fn line_length_works() {
        let mut row = Row::<Cell>::new(10);
        row[Column(5)].c = 'a';

        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn line_length_works_with_wrapline() {
        let mut row = Row::<Cell>::new(10);
        row[Column(9)].flags.insert(super::Flags::WRAPLINE);

        assert_eq!(row.line_length(), Column(10));
    }

    #[test]
    fn as_escape_works() {
        let mut s = String::new();

        macro_rules! ansi_escape {
            ($str:literal) => {{
                concat!("\x1b[", $str, "m")
            }};
        }

        macro_rules! assert_as_escape_eq {
            ($cell:expr, $text:expr) => {{
                let default = Cell::default();
                let cell = $cell;
                cell.as_escape(&mut s, &default);
                default.as_escape(&mut s, &cell);
                assert_eq!(s, $text);
                s.clear();
            }};
        }

        let fg_reset = ansi_escape!("39");
        let bg_reset = ansi_escape!("49");

        // Test color
        assert_as_escape_eq!(
            Cell { fg: Color::Indexed(100), ..Cell::default() },
            format!("{}{}", ansi_escape!("38;5;100"), fg_reset)
        );

        assert_as_escape_eq!(
            Cell { fg: Color::Named(NamedColor::Green), ..Cell::default() },
            format!("{}{}", ansi_escape!("32"), fg_reset)
        );

        assert_as_escape_eq!(
            Cell { bg: Color::Spec(Rgb { r: 5, g: 10, b: 255 }), ..Cell::default() },
            format!("{}{}", ansi_escape!("48;2;5;10;255"), bg_reset)
        );

        let bold_reset = ansi_escape!("22");

        // Test Bold
        assert_as_escape_eq!(
            Cell { flags: Flags::BOLD, ..Cell::default() },
            format!("{}{}", ansi_escape!("1"), bold_reset)
        );

        // Test Dim
        assert_as_escape_eq!(
            Cell { flags: Flags::DIM, ..Cell::default() },
            format!("{}{}", ansi_escape!("2"), bold_reset)
        );

        // Test Italic
        assert_as_escape_eq!(
            Cell { flags: Flags::ITALIC, ..Cell::default() },
            format!("{}{}", ansi_escape!("3"), ansi_escape!("23"))
        );
    }
}
