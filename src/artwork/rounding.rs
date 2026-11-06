use image::imageops::FilterType;
use image::{Rgba, RgbaImage};

use super::COVER_SIZE;

const PIXEL_DENSITY: u32 = 2;
const CORNER_RADIUS: f64 = 14.0;
const PIXEL_CENTER: f64 = 0.5;
const ANTIALIAS_WIDTH: f64 = 0.5;
const OPAQUE: f64 = 1.0;
const TRANSPARENT: f64 = 0.0;
const ALPHA_CHANNEL: usize = 3;

pub(super) fn rounded(bytes: &[u8]) -> Option<RgbaImage> {
    let size = u32::from(COVER_SIZE) * PIXEL_DENSITY;
    let radius = CORNER_RADIUS * f64::from(PIXEL_DENSITY);
    let mut cover = image::load_from_memory(bytes)
        .ok()?
        .resize_to_fill(size, size, FilterType::Lanczos3)
        .to_rgba8();
    for (x, y, Rgba(pixel)) in cover.enumerate_pixels_mut() {
        let coverage = corner_alpha(x, y, size, radius);
        if coverage < OPAQUE {
            pixel[ALPHA_CHANNEL] = scaled_alpha(pixel[ALPHA_CHANNEL], coverage);
        }
    }
    Some(cover)
}

fn corner_alpha(x: u32, y: u32, size: u32, radius: f64) -> f64 {
    let (Some(center_x), Some(center_y)) = (
        corner_center(x, size, radius),
        corner_center(y, size, radius),
    ) else {
        return OPAQUE;
    };
    let dx = pixel_center(x) - center_x;
    let dy = pixel_center(y) - center_y;
    (radius - (dx * dx + dy * dy).sqrt() + ANTIALIAS_WIDTH).clamp(TRANSPARENT, OPAQUE)
}

fn corner_center(coord: u32, size: u32, radius: f64) -> Option<f64> {
    let position = pixel_center(coord);
    let far = f64::from(size) - radius;
    if position < radius {
        return Some(radius);
    }
    if position > far {
        return Some(far);
    }
    None
}

fn pixel_center(coord: u32) -> f64 {
    f64::from(coord) + PIXEL_CENTER
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "coverage is clamped to 0..=1, so the rounded product stays within 0..=255"
)]
fn scaled_alpha(alpha: u8, coverage: f64) -> u8 {
    (f64::from(alpha) * coverage).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artwork::download::download;

    const SIZE: u32 = 128;
    const RADIUS: f64 = 16.0;

    #[test]
    fn corners_are_transparent_and_center_is_opaque() {
        assert!(corner_alpha(0, 0, SIZE, RADIUS) < f64::EPSILON);
        assert!(corner_alpha(SIZE - 1, SIZE - 1, SIZE, RADIUS) < f64::EPSILON);
        assert!((corner_alpha(SIZE / 2, SIZE / 2, SIZE, RADIUS) - OPAQUE).abs() < f64::EPSILON);
        assert!((corner_alpha(0, SIZE / 2, SIZE, RADIUS) - OPAQUE).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore = "needs network; run with `cargo test -- --ignored`"]
    fn rounds_a_real_cover() -> Result<(), Box<dyn std::error::Error>> {
        let url = "https://i.scdn.co/image/ab67616d0000b27371f59b64b173166c29d0e91d";
        let cover = rounded(&download(url).ok_or("download failed")?).ok_or("not an image")?;
        assert_eq!(cover.get_pixel(0, 0)[ALPHA_CHANNEL], 0);
        cover.save(std::env::temp_dir().join("krad-cover-test.png"))?;
        Ok(())
    }
}
