//! Canvas-style scale + crop — the ONE image-dimension transform every scaled
//! cell / mip level is built from.
//!
//! This is HTML canvas `drawImage(src, sx,sy,sw,sh, dx,dy,dw,dh)`: optionally
//! crop the source rect, then scale to the destination size. A "thumbnail" is
//! just `dest = a standard size`; a mip chain is a sequence of these. It runs
//! ONCE per (content-hash, spec) and the result is cached + `Arc`-shared
//! thereafter — never 30 times ([[media-is-compute-once-zero-copy-hardware-grade]]).
//!
//! Pure + deterministic: bytes in → bytes out, no cache, no I/O. The caching
//! (via [`SharedCompute`](crate::runtime::SharedCompute) keyed on the content
//! hash) and eager mip-chain prefetch wrap THIS; keeping the transform pure lets
//! it be tested and reused from anywhere (perception, vision, avatar, audio has
//! its own analog).

use std::io::Cursor;

use image::{GenericImageView, ImageFormat};

/// A source rectangle to crop before scaling — canvas `drawImage`'s
/// `(sx, sy, sw, sh)`. All in source pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The requested output size, in pixels — canvas `drawImage`'s `(dw, dh)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestSize {
    pub width: u32,
    pub height: u32,
}

/// Decode `src` (PNG/JPEG/WebP/…), optionally crop the source rect, resize to
/// `dest` (Lanczos3), and re-encode as PNG. Returns the encoded bytes.
///
/// This is the building block, not the cache — call it once per unique
/// (content, crop, dest) and cache the `Arc<Vec<u8>>` result on the content hash.
/// The one decode→resize→encode is the only copy; everything after is shared by
/// reference.
///
/// Fails loud on a decode error, a zero destination, or a crop rect outside the
/// source bounds (never silently clamps the crop — a bad rect is a caller bug).
pub fn scale_crop(src: &[u8], crop: Option<CropRect>, dest: DestSize) -> Result<Vec<u8>, String> {
    if dest.width == 0 || dest.height == 0 {
        return Err(format!(
            "scale_crop: destination must be non-zero, got {}x{}",
            dest.width, dest.height
        ));
    }

    let img = image::load_from_memory(src)
        .map_err(|e| format!("scale_crop: could not decode source image: {e}"))?;

    let source = match crop {
        Some(rect) => {
            let (w, h) = img.dimensions();
            if rect.width == 0 || rect.height == 0 {
                return Err(format!(
                    "scale_crop: crop must be non-zero, got {}x{}",
                    rect.width, rect.height
                ));
            }
            if rect.x + rect.width > w || rect.y + rect.height > h {
                return Err(format!(
                    "scale_crop: crop rect ({},{} {}x{}) is outside the {}x{} source",
                    rect.x, rect.y, rect.width, rect.height, w, h
                ));
            }
            img.crop_imm(rect.x, rect.y, rect.width, rect.height)
        }
        None => img,
    };

    let scaled = source.resize_exact(
        dest.width,
        dest.height,
        image::imageops::FilterType::Lanczos3,
    );

    let mut out = Cursor::new(Vec::new());
    scaled
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| format!("scale_crop: could not encode PNG: {e}"))?;
    Ok(out.into_inner())
}

/// The perceptual-change SIGNATURE grid — a tiny luma (grayscale) downscale. 16×12 ≈
/// 192 bytes: enough to catch a scene/motion change, cheap enough to diff inline at
/// frame rate. The change MONITOR's fingerprint — computed once per content hash and
/// shared, then diffed against the previous frame's signature to decide whether the
/// expensive describe is worth spending (universal, any view).
pub const SIGNATURE_SIZE: DestSize = DestSize {
    width: 16,
    height: 12,
};

/// Decode `src` and reduce it to raw GRAYSCALE (luma) bytes at `size` — a compact
/// perceptual fingerprint for change detection. No PNG re-encode (unlike [`scale_crop`]):
/// the raw luma is what a diff compares directly. Pure + deterministic; the caching on
/// the content hash wraps this, so it runs once per frame and is shared.
pub fn luma_signature(src: &[u8], size: DestSize) -> Result<Vec<u8>, String> {
    if size.width == 0 || size.height == 0 {
        return Err(format!(
            "luma_signature: size must be non-zero, got {}x{}",
            size.width, size.height
        ));
    }
    let img = image::load_from_memory(src)
        .map_err(|e| format!("luma_signature: could not decode source image: {e}"))?;
    // Triangle (bilinear) is plenty for a fingerprint and cheaper than Lanczos3.
    let small = img.resize_exact(
        size.width,
        size.height,
        image::imageops::FilterType::Triangle,
    );
    Ok(small.to_luma8().into_raw())
}

/// Mean absolute per-pixel difference between two equal-length luma signatures (0..=255)
/// — a cheap scalar "how much changed"; the caller thresholds it. Mismatched lengths (or
/// empty) → `u8::MAX`: a resolution / source swap IS a maximal change, never silently
/// treated as "no change".
pub fn luma_mean_abs_delta(a: &[u8], b: &[u8]) -> u8 {
    if a.is_empty() || a.len() != b.len() {
        return u8::MAX;
    }
    let sum: u64 = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum();
    (sum / a.len() as u64) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    // what this catches: the signature is a fixed-size raw luma grid, and the delta
    // reads ~0 for identical content but large for a very different image — the core of
    // the change monitor. Also: mismatched lengths are treated as maximally different.
    #[test]
    fn luma_signature_and_delta_detect_change() {
        let a = two_tone_png(80, 60);
        let a2 = two_tone_png(120, 90); // same red|blue pattern, different dimensions
        let solid = {
            let img = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
            let mut out = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(img)
                .write_to(&mut out, ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };

        let sig_a = luma_signature(&a, SIGNATURE_SIZE).unwrap();
        let sig_a2 = luma_signature(&a2, SIGNATURE_SIZE).unwrap();
        let sig_solid = luma_signature(&solid, SIGNATURE_SIZE).unwrap();
        assert_eq!(
            sig_a.len(),
            (SIGNATURE_SIZE.width * SIGNATURE_SIZE.height) as usize
        );

        // Same visual content (just rescaled) → near-zero delta.
        assert!(
            luma_mean_abs_delta(&sig_a, &sig_a2) < 8,
            "same pattern → small delta"
        );
        // Half-bright pattern vs solid black → large delta.
        assert!(
            luma_mean_abs_delta(&sig_a, &sig_solid) > 32,
            "different scene → large delta"
        );
        // Length mismatch → maximal.
        assert_eq!(luma_mean_abs_delta(&sig_a, &[]), u8::MAX);
    }

    /// A `w`x`h` PNG whose left half is red and right half is blue — enough to
    /// prove crop selects a region and scale changes dimensions.
    fn two_tone_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn decode(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).expect("valid PNG out")
    }

    // what this catches: the core scale — a source is resampled to the requested
    // destination dimensions (a thumbnail is just this with a standard dest).
    #[test]
    fn scales_to_the_requested_destination() {
        let src = two_tone_png(100, 80);
        let out = scale_crop(
            &src,
            None,
            DestSize {
                width: 20,
                height: 16,
            },
        )
        .expect("scale should succeed");
        assert_eq!(decode(&out).dimensions(), (20, 16));
    }

    // what this catches: crop selects a source rect BEFORE scaling — cropping the
    // right half of a red|blue image yields a blue tile (canvas drawImage semantics).
    #[test]
    fn crops_the_source_rect_before_scaling() {
        let src = two_tone_png(100, 100);
        let out = scale_crop(
            &src,
            Some(CropRect {
                x: 50,
                y: 0,
                width: 50,
                height: 100,
            }),
            DestSize {
                width: 10,
                height: 10,
            },
        )
        .expect("crop+scale should succeed");
        let img = decode(&out);
        assert_eq!(img.dimensions(), (10, 10));
        // Center pixel of the cropped (right/blue) half must be blue.
        let px = img.get_pixel(5, 5);
        assert!(
            px[2] > 200 && px[0] < 60,
            "cropped tile should be blue, got {px:?}"
        );
    }

    // what this catches: a zero destination is a loud error, never a panic or a
    // silently-empty image.
    #[test]
    fn a_zero_destination_fails_loud() {
        let src = two_tone_png(10, 10);
        let err = scale_crop(
            &src,
            None,
            DestSize {
                width: 0,
                height: 10,
            },
        )
        .expect_err("zero dest must error");
        assert!(err.contains("non-zero"), "{err}");
    }

    // what this catches: a crop outside the source bounds fails loud (never
    // silently clamps — a bad rect is a caller bug we surface).
    #[test]
    fn an_out_of_bounds_crop_fails_loud() {
        let src = two_tone_png(10, 10);
        let err = scale_crop(
            &src,
            Some(CropRect {
                x: 5,
                y: 5,
                width: 10,
                height: 10,
            }),
            DestSize {
                width: 4,
                height: 4,
            },
        )
        .expect_err("out-of-bounds crop must error");
        assert!(err.contains("outside"), "{err}");
    }

    // what this catches: garbage bytes fail loud at decode, not a panic.
    #[test]
    fn undecodable_bytes_fail_loud() {
        let err = scale_crop(
            b"not an image",
            None,
            DestSize {
                width: 4,
                height: 4,
            },
        )
        .expect_err("garbage must error");
        assert!(err.contains("decode"), "{err}");
    }
}
