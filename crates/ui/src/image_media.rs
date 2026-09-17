//! Bounded decoding for model-generated raster images.
//!
//! Generated images arrive as bytes owned by the chat's host device. Decode
//! them off the UI executor, verify the declared media type against the file
//! signature, flatten animated formats to their first frame, and retain a
//! bounded PNG for gpui.

use std::io::Cursor;
use std::sync::Arc;

use gpui::{Image, ImageFormat};

/// A decoded generated image retained as a bounded static PNG.
pub(crate) struct GeneratedMedia {
    pub image: Arc<Image>,
}

fn expected_format(mime: &str) -> Option<image::ImageFormat> {
    match mime {
        "image/png" => Some(image::ImageFormat::Png),
        "image/jpeg" => Some(image::ImageFormat::Jpeg),
        "image/webp" => Some(image::ImageFormat::WebP),
        "image/gif" => Some(image::ImageFormat::Gif),
        _ => None,
    }
}

/// Validate a generated image's declared raster type and retain one bounded
/// static frame. `max_bytes` is the encoded input ceiling for the relay read.
pub(crate) fn decode_generated_image(
    bytes: Vec<u8>,
    mime: &str,
    max_bytes: usize,
) -> Result<GeneratedMedia, String> {
    let expected =
        expected_format(mime).ok_or_else(|| "Unsupported generated image".to_string())?;
    if bytes.len() > max_bytes {
        return Err("Generated image exceeds attachment size limit".into());
    }
    let actual = image::guess_format(&bytes).map_err(|error| error.to_string())?;
    if actual != expected {
        return Err("Generated image format does not match its metadata".into());
    }

    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| error.to_string())?;
    // `DynamicImage::thumbnail` can enlarge a tiny source when its requested
    // bounds exceed the source dimensions. Clamp the bounds first so a 1×1
    // generated pixel never becomes a 2048×2048 allocation.
    let max_width = decoded.width().min(2048);
    let max_height = decoded.height().min(2048);
    let decoded = decoded.thumbnail(max_width, max_height).to_rgba8();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(decoded)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let bytes = png.into_inner();
    Ok(GeneratedMedia {
        image: Arc::new(Image::from_bytes(ImageFormat::Png, bytes)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn gif(frames: &[[u8; 4]], width: u32, height: u32) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut encoded);
        for pixel in frames {
            let image = image::RgbaImage::from_pixel(width, height, image::Rgba(*pixel));
            encoder
                .encode_frame(image::Frame::new(image))
                .expect("GIF encoder accepts fixture frame");
        }
        drop(encoder);
        encoded.into_inner()
    }

    #[test]
    fn gif_is_accepted_and_normalized_to_png() {
        let media = decode_generated_image(gif(&[[255, 0, 0, 255]], 1, 1), "image/gif", 1024)
            .expect("valid GIF should decode");
        let decoded = image::load_from_memory(&media.image.bytes).expect("PNG should decode");
        assert_eq!((decoded.width(), decoded.height()), (1, 1));
        assert!(media.image.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn animated_gif_uses_the_first_frame_and_bounded_dimensions() {
        let media = decode_generated_image(
            gif(&[[255, 0, 0, 255], [0, 255, 0, 255]], 2, 1),
            "image/gif",
            1024,
        )
        .expect("valid animated GIF should decode");
        let decoded = image::load_from_memory(&media.image.bytes)
            .expect("normalized frame should remain decodable");
        assert_eq!((decoded.width(), decoded.height()), (2, 1));
        let first = decoded.to_rgba8().get_pixel(0, 0).0;
        assert_eq!(first, [255, 0, 0, 255]);
    }

    #[test]
    fn declared_mime_must_match_the_actual_raster() {
        let bytes = gif(&[[255, 0, 0, 255]], 1, 1);
        assert!(decode_generated_image(bytes.clone(), "image/png", 1024).is_err());
        assert!(decode_generated_image(bytes, "image/svg+xml", 1024).is_err());
    }
}
