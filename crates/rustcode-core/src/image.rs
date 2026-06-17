//! Image MIME detection and validation utilities.
//!
//! Ported from:
//! - `packages/opencode/src/util/media.ts` lines 1–26 — [`sniff_attachment_mime`],
//!   [`is_image_attachment`], [`is_media`], [`is_pdf_attachment`]
//! - `packages/core/src/tool/read-filesystem.ts` lines 97–103 — `imageMime`
//! - `packages/core/src/fs-util.ts` line 202 — `FSUtil.mimeType` (extension lookup)
//! - `packages/opencode/src/image/image.ts` lines 1–206 — image normalization service
//! - `packages/core/src/image.ts` lines 1–100 — core image service interface
//!
//! Provides MIME type detection from file paths (extension) and magic bytes,
//! image-type classification, and size-limit validation used before passing
//! image data to LLM providers.

use std::path::Path;

use crate::error::ImageError;

// ── Constants (matching TS defaults) ─────────────────────────────────

/// Maximum base64-encoded image bytes accepted (5 MB).
///
/// # Source
/// `packages/opencode/src/image/image.ts` line 25.
pub const MAX_BASE64_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum image width in pixels.
///
/// # Source
/// `packages/opencode/src/image/image.ts` line 27.
pub const MAX_WIDTH: u32 = 2000;

/// Maximum image height in pixels.
///
/// # Source
/// `packages/opencode/src/image/image.ts` line 28.
pub const MAX_HEIGHT: u32 = 2000;

// ── Extension → MIME mapping ─────────────────────────────────────────

/// Map a file extension (without the dot) to its MIME type.
///
/// This is a curated subset of the `mime-types` npm package used in the
/// TS source via `FSUtil.mimeType()` at `packages/core/src/fs-util.ts:202`.
/// Covers common image formats plus a few document types.
///
/// # Source
/// `packages/core/src/fs-util.ts` line 202 — delegates to the `mime-types`
/// npm package's `lookup()`.
fn extension_mime(ext: &str) -> Option<&'static str> {
    match ext {
        // Images
        "png" => Some("image/png"),
        "jpg" | "jpeg" | "jpe" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "tiff" | "tif" => Some("image/tiff"),
        "avif" => Some("image/avif"),
        "heic" => Some("image/heic"),
        "heif" => Some("image/heif"),
        // Documents
        "pdf" => Some("application/pdf"),
        // Fallbacks for unknown extensions
        _ => None,
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Detect MIME type from a file path using its extension.
///
/// Falls back to `"application/octet-stream"` when the extension is
/// unrecognized, matching the TS behaviour.
///
/// # Source
/// `packages/core/src/fs-util.ts` line 202 — `FSUtil.mimeType(p)`.
///
/// # Examples
/// ```ignore
/// assert_eq!(detect_mime(Path::new("photo.jpg")), "image/jpeg");
/// assert_eq!(detect_mime(Path::new("diagram.png")), "image/png");
/// assert_eq!(detect_mime(Path::new("unknown.xyz")), "application/octet-stream");
/// ```
pub fn detect_mime(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some(e) => extension_mime(e).unwrap_or("application/octet-stream").to_string(),
        None => "application/octet-stream".to_string(),
    }
}

/// Detect MIME type from raw file bytes (magic number sniffing).
///
/// Inspects the first few bytes of a file to identify common image and
/// document formats. Returns the provided `fallback` when no known
/// signature is matched.
///
/// # Source
/// Ported from `sniffAttachmentMime` in `packages/opencode/src/util/media.ts`
/// lines 15–26 (with additional WebP detection from
/// `packages/core/src/tool/read-filesystem.ts` lines 97–103).
///
/// # Examples
/// ```ignore
/// let png = detect_mime_from_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "unknown");
/// assert_eq!(png, "image/png");
///
/// let unknown = detect_mime_from_bytes(b"hello", "application/octet-stream");
/// assert_eq!(unknown, "application/octet-stream");
/// ```
pub fn detect_mime_from_bytes(data: &[u8], fallback: &str) -> String {
    if data.len() >= 8
        && data[0] == 0x89
        && data[1] == 0x50
        && data[2] == 0x4E
        && data[3] == 0x47
        && data[4] == 0x0D
        && data[5] == 0x0A
        && data[6] == 0x1A
        && data[7] == 0x0A
    {
        return "image/png".to_string();
    }

    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return "image/jpeg".to_string();
    }

    if data.len() >= 4 && data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38 {
        return "image/gif".to_string();
    }

    if data.len() >= 2 && data[0] == 0x42 && data[1] == 0x4D {
        return "image/bmp".to_string();
    }

    // PDF: starts with "%PDF-"
    if data.len() >= 5
        && data[0] == 0x25
        && data[1] == 0x50
        && data[2] == 0x44
        && data[3] == 0x46
        && data[4] == 0x2D
    {
        return "application/pdf".to_string();
    }

    // WebP: "RIFF....WEBP"
    if data.len() >= 12
        && data[0] == 0x52
        && data[1] == 0x49
        && data[2] == 0x46
        && data[3] == 0x46
        && data[8] == 0x57
        && data[9] == 0x45
        && data[10] == 0x42
        && data[11] == 0x50
    {
        return "image/webp".to_string();
    }

    fallback.to_string()
}

/// Check whether a MIME type string represents an image (excluding SVG).
///
/// SVG (`image/svg+xml`) is excluded because it is an XML vector format,
/// not a raster image that can be passed to vision-capable LLMs.
/// `image/vnd.fastbidsheet` is also excluded (vendor-specific).
///
/// # Source
/// Ported from `isImageAttachment` in `packages/opencode/src/util/media.ts`
/// lines 11–13 and `packages/core/src/tool/webfetch.ts` lines 109–110.
///
/// # Examples
/// ```ignore
/// assert!(is_image_mime("image/png"));
/// assert!(is_image_mime("image/jpeg"));
/// assert!(!is_image_mime("image/svg+xml"));
/// assert!(!is_image_mime("text/plain"));
/// ```
pub fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
        && mime != "image/svg+xml"
        && mime != "image/vnd.fastbidsheet"
}

/// Check whether a MIME type is displayable media (image or PDF).
///
/// # Source
/// Ported from `isMedia` in `packages/opencode/src/util/media.ts`
/// lines 7–9.
///
/// # Examples
/// ```ignore
/// assert!(is_media("image/png"));
/// assert!(is_media("application/pdf"));
/// assert!(!is_media("text/plain"));
/// ```
pub fn is_media(mime: &str) -> bool {
    is_image_mime(mime) || mime == "application/pdf"
}

/// Check whether a MIME type is a PDF document.
///
/// # Source
/// Ported from `isPdfAttachment` in `packages/opencode/src/util/media.ts`
/// lines 3–5.
///
/// # Examples
/// ```ignore
/// assert!(is_pdf_mime("application/pdf"));
/// assert!(!is_pdf_mime("image/png"));
/// ```
pub fn is_pdf_mime(mime: &str) -> bool {
    mime == "application/pdf"
}

/// Validate that an image fits within configured size limits.
///
/// Returns `Ok(())` when all dimensions and byte counts pass. Returns an
/// [`ImageError::Size`] variant when any limit is exceeded.
///
/// # Source
/// Ported from the size checks in `packages/opencode/src/image/image.ts`
/// and `packages/core/src/image.ts` — the `SizeError` class and the
/// validation logic in the `normalize` functions.
///
/// # Examples
/// ```ignore
/// assert!(image_size_ok(800, 600, 100_000).is_ok());
/// assert!(image_size_ok(4096, 2160, 100_000).is_err()); // too wide
/// assert!(image_size_ok(800, 600, 100_000_000).is_err()); // too many bytes
/// ```
pub fn image_size_ok(
    width: u32,
    height: u32,
    base64_bytes: u64,
) -> Result<(), ImageError> {
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(ImageError::Size { width, height });
    }
    if base64_bytes > MAX_BASE64_BYTES {
        return Err(ImageError::Size { width, height });
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_mime (extension-based) ──────────────────────────────

    #[test]
    fn test_detect_mime_common_images() {
        assert_eq!(detect_mime(Path::new("photo.png")), "image/png");
        assert_eq!(detect_mime(Path::new("photo.jpg")), "image/jpeg");
        assert_eq!(detect_mime(Path::new("photo.jpeg")), "image/jpeg");
        assert_eq!(detect_mime(Path::new("photo.gif")), "image/gif");
        assert_eq!(detect_mime(Path::new("photo.webp")), "image/webp");
        assert_eq!(detect_mime(Path::new("photo.bmp")), "image/bmp");
    }

    #[test]
    fn test_detect_mime_case_insensitive() {
        assert_eq!(detect_mime(Path::new("photo.PNG")), "image/png");
        assert_eq!(detect_mime(Path::new("photo.JPG")), "image/jpeg");
        assert_eq!(detect_mime(Path::new("photo.GIF")), "image/gif");
    }

    #[test]
    fn test_detect_mime_no_extension() {
        assert_eq!(
            detect_mime(Path::new("no_extension")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_detect_mime_unknown_extension() {
        assert_eq!(
            detect_mime(Path::new("file.xyz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_detect_mime_pdf() {
        assert_eq!(detect_mime(Path::new("document.pdf")), "application/pdf");
    }

    #[test]
    fn test_detect_mime_svg() {
        assert_eq!(
            detect_mime(Path::new("icon.svg")),
            "image/svg+xml"
        );
    }

    #[test]
    fn test_detect_mime_hidden_file() {
        // Files starting with a dot but having an extension
        assert_eq!(
            detect_mime(Path::new(".hidden.png")),
            "image/png"
        );
    }

    // ── detect_mime_from_bytes (magic bytes) ───────────────────────

    #[test]
    fn test_sniff_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(
            detect_mime_from_bytes(&png_header, "unknown"),
            "image/png"
        );
    }

    #[test]
    fn test_sniff_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(
            detect_mime_from_bytes(&jpeg_header, "unknown"),
            "image/jpeg"
        );
    }

    #[test]
    fn test_sniff_gif87a() {
        let gif_header = b"GIF87a";
        assert_eq!(
            detect_mime_from_bytes(gif_header, "unknown"),
            "image/gif"
        );
    }

    #[test]
    fn test_sniff_gif89a() {
        let gif_header = b"GIF89a";
        assert_eq!(
            detect_mime_from_bytes(gif_header, "unknown"),
            "image/gif"
        );
    }

    #[test]
    fn test_sniff_bmp() {
        let bmp_header = [0x42, 0x4D, 0x00, 0x00];
        assert_eq!(
            detect_mime_from_bytes(&bmp_header, "unknown"),
            "image/bmp"
        );
    }

    #[test]
    fn test_sniff_pdf() {
        let pdf_header = [0x25, 0x50, 0x44, 0x46, 0x2D];
        assert_eq!(
            detect_mime_from_bytes(&pdf_header, "unknown"),
            "application/pdf"
        );
    }

    #[test]
    fn test_sniff_webp() {
        let webp_header = [
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0x00, 0x00, 0x00, 0x00, // size (ignored)
            0x57, 0x45, 0x42, 0x50, // "WEBP"
        ];
        assert_eq!(
            detect_mime_from_bytes(&webp_header, "unknown"),
            "image/webp"
        );
    }

    #[test]
    fn test_sniff_short_data() {
        // Too short for any signature
        assert_eq!(
            detect_mime_from_bytes(&[0x89], "fallback"),
            "fallback"
        );
        assert_eq!(
            detect_mime_from_bytes(&[], "fallback"),
            "fallback"
        );
    }

    #[test]
    fn test_sniff_unknown() {
        assert_eq!(
            detect_mime_from_bytes(b"Hello, world!", "application/octet-stream"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_sniff_webp_with_valid_size_field() {
        // A properly formed WebP: "RIFF" + 4-byte LE size + "WEBP"
        let mut header = vec![0x52, 0x49, 0x46, 0x46]; // "RIFF"
        header.extend_from_slice(&[0x0C, 0x00, 0x00, 0x00]); // size = 12
        header.extend_from_slice(&[0x57, 0x45, 0x42, 0x50]); // "WEBP"
        assert_eq!(
            detect_mime_from_bytes(&header, "unknown"),
            "image/webp"
        );
    }

    // ── is_image_mime ──────────────────────────────────────────────

    #[test]
    fn test_is_image_mime_common() {
        assert!(is_image_mime("image/png"));
        assert!(is_image_mime("image/jpeg"));
        assert!(is_image_mime("image/gif"));
        assert!(is_image_mime("image/webp"));
        assert!(is_image_mime("image/bmp"));
        assert!(is_image_mime("image/avif"));
        assert!(is_image_mime("image/tiff"));
    }

    #[test]
    fn test_is_image_mime_excludes_svg() {
        assert!(!is_image_mime("image/svg+xml"));
    }

    #[test]
    fn test_is_image_mime_excludes_non_image() {
        assert!(!is_image_mime("text/plain"));
        assert!(!is_image_mime("application/pdf"));
        assert!(!is_image_mime("application/json"));
        assert!(!is_image_mime("video/mp4"));
    }

    #[test]
    fn test_is_image_mime_excludes_fastbidsheet() {
        assert!(!is_image_mime("image/vnd.fastbidsheet"));
    }

    // ── is_media ───────────────────────────────────────────────────

    #[test]
    fn test_is_media_images() {
        assert!(is_media("image/png"));
        assert!(is_media("image/jpeg"));
    }

    #[test]
    fn test_is_media_pdf() {
        assert!(is_media("application/pdf"));
    }

    #[test]
    fn test_is_media_excludes_other() {
        assert!(!is_media("text/plain"));
        assert!(!is_media("image/svg+xml"));
        assert!(!is_media("video/mp4"));
    }

    // ── is_pdf_mime ────────────────────────────────────────────────

    #[test]
    fn test_is_pdf_mime_true() {
        assert!(is_pdf_mime("application/pdf"));
    }

    #[test]
    fn test_is_pdf_mime_false() {
        assert!(!is_pdf_mime("image/png"));
        assert!(!is_pdf_mime("text/plain"));
        assert!(!is_pdf_mime("application/json"));
    }

    // ── image_size_ok ──────────────────────────────────────────────

    #[test]
    fn test_image_size_ok_within_limits() {
        assert!(image_size_ok(800, 600, 100_000).is_ok());
        assert!(image_size_ok(MAX_WIDTH, MAX_HEIGHT, MAX_BASE64_BYTES).is_ok());
    }

    #[test]
    fn test_image_size_ok_too_wide() {
        let result = image_size_ok(MAX_WIDTH + 1, 100, 1_000);
        assert!(result.is_err());
        if let Err(ImageError::Size { width, height }) = result {
            assert_eq!(width, MAX_WIDTH + 1);
            assert_eq!(height, 100);
        }
    }

    #[test]
    fn test_image_size_ok_too_tall() {
        let result = image_size_ok(100, MAX_HEIGHT + 1, 1_000);
        assert!(result.is_err());
        if let Err(ImageError::Size { width, height }) = result {
            assert_eq!(width, 100);
            assert_eq!(height, MAX_HEIGHT + 1);
        }
    }

    #[test]
    fn test_image_size_ok_too_many_bytes() {
        let result = image_size_ok(100, 100, MAX_BASE64_BYTES + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_constants_match_ts() {
        // These must match the constants in the TS source
        assert_eq!(MAX_BASE64_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_WIDTH, 2000);
        assert_eq!(MAX_HEIGHT, 2000);
    }

    #[test]
    fn test_extension_mime_all_variants_covered() {
        // All JPEG variants
        assert_eq!(extension_mime("jpg"), Some("image/jpeg"));
        assert_eq!(extension_mime("jpeg"), Some("image/jpeg"));
        assert_eq!(extension_mime("jpe"), Some("image/jpeg"));
        // TIFF variants
        assert_eq!(extension_mime("tiff"), Some("image/tiff"));
        assert_eq!(extension_mime("tif"), Some("image/tiff"));
    }

    // ── Additional extension tests ──────────────────────────────────

    #[test]
    fn test_detect_mime_ico() {
        assert_eq!(detect_mime(Path::new("favicon.ico")), "image/x-icon");
    }

    #[test]
    fn test_detect_mime_avif() {
        assert_eq!(detect_mime(Path::new("photo.avif")), "image/avif");
    }

    #[test]
    fn test_detect_mime_heic_heif() {
        assert_eq!(detect_mime(Path::new("photo.heic")), "image/heic");
        assert_eq!(detect_mime(Path::new("photo.heif")), "image/heif");
    }

    // ── PDF detection edge cases ────────────────────────────────────

    #[test]
    fn test_detect_mime_pdf_uppercase() {
        assert_eq!(
            detect_mime(Path::new("DOCUMENT.PDF")),
            "application/pdf"
        );
    }

    #[test]
    fn test_sniff_pdf_with_binary_after_header() {
        // PDF header followed by arbitrary binary data — should still detect
        let mut data = vec![0x25, 0x50, 0x44, 0x46, 0x2D]; // "%PDF-"
        data.extend_from_slice(&[0x00, 0xFF, 0xAB, 0xCD, 0xEF, 0x01, 0x02]);
        assert_eq!(
            detect_mime_from_bytes(&data, "unknown"),
            "application/pdf"
        );
    }

    // ── Magic byte edge cases ───────────────────────────────────────

    #[test]
    fn test_sniff_png_empty_fallback() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(
            detect_mime_from_bytes(&png_header, ""),
            "image/png"
        );
    }

    #[test]
    fn test_sniff_jpeg_exif() {
        // EXIF JPEG: starts with 0xFF 0xD8 0xFF 0xE1
        let jpeg_exif = [0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x01, 0x02, 0x03];
        assert_eq!(
            detect_mime_from_bytes(&jpeg_exif, "unknown"),
            "image/jpeg"
        );
    }

    #[test]
    fn test_sniff_webp_riff_no_webp() {
        // RIFF header present but bytes 8-11 are not "WEBP" — should fall back
        let riff_not_webp = [
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0x00, 0x00, 0x00, 0x00, // size
            0x41, 0x56, 0x49, 0x20, // "AVI " (not WEBP)
        ];
        assert_eq!(
            detect_mime_from_bytes(&riff_not_webp, "application/octet-stream"),
            "application/octet-stream"
        );
    }

    // ── image_size_ok boundary tests ────────────────────────────────

    #[test]
    fn test_image_size_ok_at_max_width() {
        // Exactly at MAX_WIDTH should be Ok
        assert!(image_size_ok(MAX_WIDTH, 100, 100).is_ok());
    }

    #[test]
    fn test_image_size_ok_at_max_height() {
        // Exactly at MAX_HEIGHT should be Ok
        assert!(image_size_ok(100, MAX_HEIGHT, 100).is_ok());
    }

    #[test]
    fn test_image_size_ok_at_max_base64_bytes() {
        // Exactly at MAX_BASE64_BYTES should be Ok
        assert!(image_size_ok(100, 100, MAX_BASE64_BYTES).is_ok());
    }

    #[test]
    fn test_image_size_ok_zero_dimensions() {
        // Zero dimensions are within limits (0 ≤ MAX)
        assert!(image_size_ok(0, 0, 100).is_ok());
        assert!(image_size_ok(0, 100, 100).is_ok());
        assert!(image_size_ok(100, 0, 100).is_ok());
    }

    // ── Combined limit boundary test ────────────────────────────────

    #[test]
    fn test_image_size_ok_all_at_max() {
        // All dimensions exactly at their MAX limits should be Ok
        assert!(image_size_ok(MAX_WIDTH, MAX_HEIGHT, MAX_BASE64_BYTES).is_ok());
    }

    // ── detect_mime edge cases ──────────────────────────────────────

    #[test]
    fn test_detect_mime_multiple_dots() {
        // "gz" extension is not in our mapping — falls back to octet-stream
        assert_eq!(
            detect_mime(Path::new("archive.tar.gz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_detect_mime_dotfile_secret_png() {
        // Hidden dotfiles with a known extension should be detected correctly
        assert_eq!(
            detect_mime(Path::new(".secret.png")),
            "image/png"
        );
    }
}
