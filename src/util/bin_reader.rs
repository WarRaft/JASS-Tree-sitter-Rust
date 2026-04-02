//! Lightweight sequential binary reader for little-endian Warcraft 3 data files.
//!
//! Wraps a byte slice and a cursor position.  Every `read_*` method advances
//! the cursor by exactly the number of bytes consumed.  On EOF or invalid data
//! the methods return `Err`.

use std::fmt;

// ─── Parse metadata ──────────────────────────────────────────────────────────

/// Metadata produced after parsing, reporting how much of the input was consumed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BinReaderMeta {
    /// Total size of the input buffer in bytes.
    pub total: usize,
    /// Byte offset where the parser stopped (= number of bytes consumed).
    pub read: usize,
    /// Bytes remaining after the parser stopped.
    pub remaining: usize,
}

// ─── Error type ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum BinError {
    /// Not enough bytes left in the buffer.
    Eof {
        needed: usize,
        remaining: usize,
        offset: usize,
    },
    /// A null-terminated string was not terminated before the end of the buffer.
    UnterminatedString { offset: usize },
}

impl fmt::Display for BinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinError::Eof { needed, remaining, offset } => {
                write!(
                    f,
                    "unexpected EOF at offset 0x{:X}: need {} bytes, {} remaining",
                    offset, needed, remaining,
                )
            }
            BinError::UnterminatedString { offset } => {
                write!(f, "unterminated string at offset 0x{:X}", offset)
            }
        }
    }
}

impl std::error::Error for BinError {}

pub type BinResult<T> = Result<T, BinError>;

// ─── Reader ──────────────────────────────────────────────────────────────────

/// Sequential little-endian binary reader over a borrowed byte slice.
///
/// ```text
/// let mut r = BinReader::new(&bytes);
/// let magic = r.read_u32()?;
/// let name  = r.read_cstring()?;
/// ```
pub struct BinReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BinReader<'a> {
    /// Create a new reader starting at offset 0.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current byte offset in the buffer.
    #[inline]
    #[allow(dead_code)]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Total length of the underlying buffer.
    #[inline]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Remaining bytes from the current position.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Produce parse metadata — total / read / remaining byte counts.
    #[inline]
    pub fn meta(&self) -> BinReaderMeta {
        BinReaderMeta {
            total: self.data.len(),
            read: self.pos,
            remaining: self.remaining(),
        }
    }

    /// Seek to an absolute byte offset.
    #[inline]
    #[allow(dead_code)]
    pub fn seek(&mut self, offset: usize) {
        self.pos = offset;
    }

    // ── Raw slice ────────────────────────────────────────────────────────

    /// Read exactly `n` bytes and advance the cursor.
    pub fn read_bytes(&mut self, n: usize) -> BinResult<&'a [u8]> {
        if self.remaining() < n {
            return Err(BinError::Eof {
                needed: n,
                remaining: self.remaining(),
                offset: self.pos,
            });
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Skip `n` bytes.
    #[allow(dead_code)]
    pub fn skip(&mut self, n: usize) -> BinResult<()> {
        self.read_bytes(n)?;
        Ok(())
    }

    // ── Integer primitives (little-endian) ───────────────────────────────

    pub fn read_u8(&mut self) -> BinResult<u8> {
        let b = self.read_bytes(1)?;
        Ok(b[0])
    }

    #[allow(dead_code)]
    pub fn read_u16(&mut self) -> BinResult<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn read_u32(&mut self) -> BinResult<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_s32(&mut self) -> BinResult<i32> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_f32(&mut self) -> BinResult<f32> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    // ── Strings ──────────────────────────────────────────────────────────

    /// Read a null-terminated (C) string.  The null byte is consumed but
    /// not included in the result.
    pub fn read_cstring(&mut self) -> BinResult<String> {
        let start = self.pos;
        let rest = &self.data[self.pos..];
        match rest.iter().position(|&b| b == 0) {
            Some(nul_pos) => {
                let s = String::from_utf8_lossy(&rest[..nul_pos]).into_owned();
                self.pos += nul_pos + 1; // skip the NUL
                Ok(s)
            }
            None => Err(BinError::UnterminatedString { offset: start }),
        }
    }

    /// Read a fixed-size raw string (no null terminator expected).
    /// Trailing null bytes are trimmed.
    pub fn read_fixed_string(&mut self, len: usize) -> BinResult<String> {
        let b = self.read_bytes(len)?;
        let trimmed = match b.iter().position(|&c| c == 0) {
            Some(pos) => &b[..pos],
            None => b,
        };
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }

    // ── Warcraft-specific helpers ────────────────────────────────────────

    /// Read a 4-byte rawcode (e.g. `'hfoo'`).
    #[allow(dead_code)]
    pub fn read_rawcode(&mut self) -> BinResult<[u8; 4]> {
        let b = self.read_bytes(4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    /// Read a single `char` (1 byte, as used in hexpat for tileset IDs etc.).
    pub fn read_char(&mut self) -> BinResult<u8> {
        self.read_u8()
    }
}

/// Convenience methods requiring [`BinRead`].
impl<'a> BinReader<'a> {
    /// Read a `u32` count followed by that many `T` items.
    ///
    /// Matches the very common hexpat pattern:
    /// ```text
    /// u32 count;
    /// T items[count];
    /// ```
    pub fn read_vec<T: BinRead>(&mut self) -> BinResult<Vec<T>> {
        let count = self.read_u32()?;
        let mut v = Vec::with_capacity(count as usize);
        for _ in 0..count {
            v.push(T::bin_read(self)?);
        }
        Ok(v)
    }
}

// ─── BinRead trait ───────────────────────────────────────────────────────────

/// Trait for types that can be sequentially read from a little-endian binary
/// stream.
///
/// Implement manually or use the [`bin_struct!`], [`bin_enum!`], and
/// [`bin_bitfield!`] macros to generate implementations automatically.
pub trait BinRead: Sized {
    fn bin_read(r: &mut BinReader) -> BinResult<Self>;
}

impl BinRead for u8  { #[inline] fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_u8()  } }
impl BinRead for u16 { #[inline] fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_u16() } }
impl BinRead for u32 { #[inline] fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_u32() } }
impl BinRead for i32 { #[inline] fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_s32() } }
impl BinRead for f32 { #[inline] fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_f32() } }

/// Reads a null-terminated (C) string — corresponds to hexpat `char name[]`.
impl BinRead for String {
    #[inline]
    fn bin_read(r: &mut BinReader) -> BinResult<Self> { r.read_cstring() }
}

// ─── Rawcode ─────────────────────────────────────────────────────────────────

/// Returns `true` when the byte is a printable ASCII character suitable for
/// display in a rawcode string.  Single-quote (`'`, 0x27) and control / high
/// bytes are treated as non-printable.
#[inline]
fn is_rawcode_printable(b: u8) -> bool {
    b >= 0x21 && b <= 0x7E && b != b'\''
}

/// 4-byte rawcode used throughout Warcraft III data files (e.g. `'hfoo'`,
/// `'Hamg'`).
///
/// Stores two representations:
/// - `raw`  — the 4 bytes as a **little-endian** `u32`.
/// - `text` — human-readable string: either 4 ASCII characters (e.g. `"hfoo"`)
///   or hex format (e.g. `"0x68006F6F"`) when any byte is non-printable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rawcode {
    /// Raw 4-byte value as a little-endian `u32`.
    pub raw: u32,
    /// Human-readable representation.
    pub text: String,
}

impl Rawcode {
    /// Create a `Rawcode` from 4 raw bytes (in file order).
    pub fn from_bytes(b: [u8; 4]) -> Self {
        let raw = u32::from_le_bytes(b);
        let text = if b.iter().all(|&byte| is_rawcode_printable(byte)) {
            // Safety: all bytes are ASCII, so this is valid UTF-8.
            String::from_utf8(b.to_vec()).unwrap()
        } else {
            format!("0x{:02X}{:02X}{:02X}{:02X}", b[0], b[1], b[2], b[3])
        };
        Self { raw, text }
    }
}

impl serde::Serialize for Rawcode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Rawcode", 2)?;
        s.serialize_field("raw", &self.raw)?;
        s.serialize_field("text", &self.text)?;
        s.end()
    }
}

impl BinRead for Rawcode {
    #[inline]
    fn bin_read(r: &mut BinReader) -> BinResult<Self> {
        let b = r.read_bytes(4)?;
        Ok(Self::from_bytes([b[0], b[1], b[2], b[3]]))
    }
}

impl std::ops::Deref for Rawcode {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str { &self.text }
}

impl fmt::Display for Rawcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.text.fmt(f) }
}

// ─── Declarative macros ──────────────────────────────────────────────────────

/// Define a struct with automatic [`BinRead`], `Serialize`, `Debug`, `Clone`.
///
/// Fields are read in declaration order.  Each field type must implement
/// [`BinRead`].
///
/// ```ignore
/// crate::bin_struct! {
///     /// BGRA colour.
///     pub Color { b: u8, g: u8, r: u8, a: u8 }
/// }
/// ```
#[macro_export]
macro_rules! bin_struct {
    (
        $( #[$meta:meta] )*
        pub $name:ident {
            $( $( #[$fmeta:meta] )* $field:ident : $ty:ty ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, ::serde::Serialize)]
        $( #[$meta] )*
        pub struct $name {
            $( $( #[$fmeta] )* pub $field: $ty, )*
        }

        impl $crate::util::bin_reader::BinRead for $name {
            fn bin_read(r: &mut $crate::util::bin_reader::BinReader)
                -> $crate::util::bin_reader::BinResult<Self>
            {
                Ok(Self {
                    $( $field: <$ty as $crate::util::bin_reader::BinRead>::bin_read(r)?, )*
                })
            }
        }
    };
}

/// Define an integer-backed enum with `Unknown(repr)` fallback and automatic
/// [`BinRead`].
///
/// ```ignore
/// crate::bin_enum! {
///     pub PlayerType: u32 {
///         Human = 1,
///         Comp = 2,
///     }
/// }
/// ```
#[macro_export]
macro_rules! bin_enum {
    (
        $( #[$meta:meta] )*
        pub $name:ident : $repr:ty {
            $( $variant:ident = $val:expr ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, Copy, ::serde::Serialize)]
        $( #[$meta] )*
        pub enum $name {
            $( $variant, )*
            Unknown($repr),
        }

        impl $crate::util::bin_reader::BinRead for $name {
            fn bin_read(r: &mut $crate::util::bin_reader::BinReader)
                -> $crate::util::bin_reader::BinResult<Self>
            {
                let v = <$repr as $crate::util::bin_reader::BinRead>::bin_read(r)?;
                Ok(match v {
                    $( $val => Self::$variant, )*
                    _ => Self::Unknown(v),
                })
            }
        }
    };
}

/// Define a bitfield wrapper with boolean accessors and automatic [`BinRead`].
///
/// ```ignore
/// crate::bin_bitfield! {
///     pub MapFlags: u32 {
///         hide_minimap = 0,
///         melee_map = 2,
///     }
/// }
/// ```
#[macro_export]
macro_rules! bin_bitfield {
    (
        $( #[$meta:meta] )*
        pub $name:ident : $repr:ty {
            $( $flag:ident = $bit:expr ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, ::serde::Serialize)]
        $( #[$meta] )*
        pub struct $name {
            pub raw: $repr,
        }

        #[allow(dead_code)]
        impl $name {
            $( pub fn $flag(&self) -> bool { self.raw & (1 << $bit) != 0 } )*
        }

        impl $crate::util::bin_reader::BinRead for $name {
            fn bin_read(r: &mut $crate::util::bin_reader::BinReader)
                -> $crate::util::bin_reader::BinResult<Self>
            {
                Ok(Self { raw: <$repr as $crate::util::bin_reader::BinRead>::bin_read(r)? })
            }
        }
    };
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u32_le() {
        let data = [0x01, 0x00, 0x00, 0x00];
        let mut r = BinReader::new(&data);
        assert_eq!(r.read_u32().unwrap(), 1);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn read_cstring() {
        let data = b"hello\x00world\x00";
        let mut r = BinReader::new(data);
        assert_eq!(r.read_cstring().unwrap(), "hello");
        assert_eq!(r.read_cstring().unwrap(), "world");
    }

    #[test]
    fn read_cstring_unterminated() {
        let data = b"oops";
        let mut r = BinReader::new(data);
        assert!(r.read_cstring().is_err());
    }

    #[test]
    fn read_rawcode() {
        let data = b"hfoo";
        let mut r = BinReader::new(data);
        assert_eq!(r.read_rawcode().unwrap(), *b"hfoo");
    }

    #[test]
    fn rawcode_printable() {
        let rc = Rawcode::from_bytes(*b"hfoo");
        assert_eq!(rc.text, "hfoo");
        assert_eq!(rc.raw, u32::from_le_bytes(*b"hfoo"));
    }

    #[test]
    fn rawcode_non_printable() {
        let rc = Rawcode::from_bytes([0x00, 0x41, 0x42, 0x43]);
        assert_eq!(rc.text, "0x00414243");
    }

    #[test]
    fn rawcode_single_quote_non_printable() {
        let rc = Rawcode::from_bytes(*b"h'oo");
        assert_eq!(rc.text, "0x68276F6F");
    }

    #[test]
    fn rawcode_bin_read() {
        let data = b"Hamg";
        let mut r = BinReader::new(data);
        let rc = Rawcode::bin_read(&mut r).unwrap();
        assert_eq!(rc.text, "Hamg");
        assert_eq!(&*rc, "Hamg"); // Deref to str
        assert_eq!(format!("{}", rc), "Hamg"); // Display
    }

    #[test]
    fn read_f32() {
        let v: f32 = 3.14;
        let bytes = v.to_le_bytes();
        let mut r = BinReader::new(&bytes);
        let read = r.read_f32().unwrap();
        assert!((read - 3.14).abs() < 0.001);
    }

    #[test]
    fn eof_error() {
        let data = [0x01, 0x02];
        let mut r = BinReader::new(&data);
        assert!(r.read_u32().is_err());
    }

    #[test]
    fn seek_and_read() {
        let data = [0x00, 0x00, 0x00, 0x00, 0x2A, 0x00, 0x00, 0x00];
        let mut r = BinReader::new(&data);
        r.seek(4);
        assert_eq!(r.read_u32().unwrap(), 42);
    }

    #[test]
    fn fixed_string() {
        let data = b"AB\x00\x00";
        let mut r = BinReader::new(data);
        assert_eq!(r.read_fixed_string(4).unwrap(), "AB");
    }
}

