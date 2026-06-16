//! Image processing utilities.
//!
//! Ported from: `packages/opencode/src/image/image.ts`

/// Image format detection and normalization.
pub fn detect_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
