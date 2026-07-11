//! amber_byteorder — Replacement for the `byteorder` crate
//!
//! Use the endianness methods on integer types (stable since Rust 1.32+)
//! instead of `ReadBytesExt`/`WriteBytesExt`.

/// Read a little-endian u16 from a byte slice.
pub fn read_u16_le(bytes: &[u8]) -> Option<u16> {
    bytes.get(0..2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// Read a big-endian u16 from a byte slice.
pub fn read_u16_be(bytes: &[u8]) -> Option<u16> {
    bytes.get(0..2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// Read a little-endian u32 from a byte slice.
pub fn read_u32_le(bytes: &[u8]) -> Option<u32> {
    bytes.get(0..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Read a big-endian u32 from a byte slice.
pub fn read_u32_be(bytes: &[u8]) -> Option<u32> {
    bytes.get(0..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Write a u16 as little-endian bytes into a buffer.
pub fn write_u16_le(buf: &mut [u8], value: u16) -> Option<()> {
    let target = buf.get_mut(0..2)?;
    target.copy_from_slice(&value.to_le_bytes());
    Some(())
}

/// Write a u32 as big-endian bytes into a buffer.
pub fn write_u32_be(buf: &mut [u8], value: u32) -> Option<()> {
    let target = buf.get_mut(0..4)?;
    target.copy_from_slice(&value.to_be_bytes());
    Some(())
}

