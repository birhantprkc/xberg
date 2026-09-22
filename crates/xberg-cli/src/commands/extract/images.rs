//! Writing extracted images to disk.

use anyhow::{Context, Result};
use std::path::Path;
use xberg::ExtractedImage;

/// Write extracted images to `output_dir`, using the same `image_{index}.{format}` naming
/// convention the markdown renderer uses for its `![](image_N.ext)` references.
///
/// Images with empty data (placeholder `.bin` entries) are skipped — they have no bytes to write.
pub(super) fn write_extracted_images(images: &[ExtractedImage], output_dir: &Path) -> Result<()> {
    for img in images {
        if img.data.is_empty() {
            continue;
        }
        let filename = format!("image_{}.{}", img.image_index, img.format);
        let dest = output_dir.join(&filename);
        std::fs::write(&dest, &img.data).with_context(|| format!("Failed to write image file '{}'", dest.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::borrow::Cow;
    use tempfile::tempdir;

    fn make_image(index: u32, format: &'static str, data: &[u8]) -> ExtractedImage {
        ExtractedImage {
            data: Bytes::copy_from_slice(data),
            format: Cow::Borrowed(format),
            image_index: index,
            ..Default::default()
        }
    }

    #[test]
    fn write_extracted_images_creates_files_with_correct_names() {
        let dir = tempdir().unwrap();
        let images = vec![
            make_image(0, "png", b"\x89PNG\r\n"),
            make_image(1, "jpeg", b"\xff\xd8\xff"),
        ];

        write_extracted_images(&images, dir.path()).unwrap();

        assert!(dir.path().join("image_0.png").exists());
        assert!(dir.path().join("image_1.jpeg").exists());
        assert_eq!(std::fs::read(dir.path().join("image_0.png")).unwrap(), b"\x89PNG\r\n");
    }

    #[test]
    fn write_extracted_images_skips_empty_data() {
        let dir = tempdir().unwrap();
        let images = vec![make_image(0, "bin", b"")];

        write_extracted_images(&images, dir.path()).unwrap();

        assert!(
            !dir.path().join("image_0.bin").exists(),
            "empty-data image must not be written"
        );
    }

    #[test]
    fn write_extracted_images_uses_image_index_not_position() {
        let dir = tempdir().unwrap();
        let images = vec![make_image(3, "png", b"abc"), make_image(7, "png", b"def")];

        write_extracted_images(&images, dir.path()).unwrap();

        assert!(dir.path().join("image_3.png").exists());
        assert!(dir.path().join("image_7.png").exists());
        assert!(!dir.path().join("image_0.png").exists());
        assert!(!dir.path().join("image_1.png").exists());
    }
}
