//! Pure image pipeline: decode → fit (max dimension) → BGRA mip chain.
//!
//! Produces Engine-specific byte layout (`PF_A8R8G8B8` = B,G,R,A per pixel on little-endian)
//! so the engine layer can copy the buffers unchanged.

use image::{DynamicImage, ImageFormat};

use fast_image_resize as fir;
use fir::images::{Image, ImageRef};

/// One mip level in BGRA byte order (UE3 `PF_A8R8G8B8` memory layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mip {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `BGRA8`, `width * height * 4` bytes.
    pub bgra: Vec<u8>,
}

/// A decoded RGBA8 image (straight alpha; JPEG decodes opaque).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `RGBA8`, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Decode PNG or JPEG bytes (sniffed by magic; anything else is rejected with
/// a clear error so mod authors see *why*, not a decoder panic).
pub fn decode_rgba(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.is_empty() {
        return Err("empty image data".to_owned());
    }
    let format = image::guess_format(bytes).map_err(|e| format!("unrecognized image data: {e}"))?;
    match format {
        ImageFormat::Png | ImageFormat::Jpeg => {}
        other => {
            return Err(format!(
                "unsupported image format ({other:?}); supported: PNG, JPEG"
            ));
        }
    }
    let img: DynamicImage = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| format!("image decode failed: {e}"))?;
    let rgba = img.to_rgba8();
    Ok(Decoded {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// Scale the image down so neither dimension exceeds `max_dim`, preserving
/// aspect ratio (`max_dim < 1` is clamped to 1). Images within the cap are
/// returned untouched.
/// Returns the fitted image plus human-readable notes for the log.
pub fn fit(img: &Decoded, max_dim: u32) -> (Decoded, Vec<String>) {
    let mut notes = Vec::new();
    let cap = max_dim.max(1);
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || (w <= cap && h <= cap) {
        return (img.clone(), notes);
    }

    let scale = f64::from(cap) / f64::from(w.max(h));
    let tw = ((f64::from(w) * scale).round() as u32).max(1);
    let th = ((f64::from(h) * scale).round() as u32).max(1);
    notes.push(format!(
        "resized {w}x{h} -> {tw}x{th} (larger than the {cap}px max dimension)"
    ));
    let src = ImageRef::new(w, h, &img.rgba, fir::PixelType::U8x4)
        .expect("rgba buffer matches dimensions");
    let mut dst = Image::new(tw, th, fir::PixelType::U8x4);
    let options = fir::ResizeOptions {
        algorithm: fir::ResizeAlg::Convolution(fir::FilterType::Lanczos3),
        cropping: fir::SrcCropping::None,
        mul_div_alpha: true,
    };
    let mut resizer = fir::Resizer::new();
    resizer
        .resize(&src, &mut dst, &options)
        .expect("sizes are non zero");

    (
        Decoded {
            width: tw,
            height: th,
            rgba: dst.into_vec(),
        },
        notes,
    )
}

/// Full mip chain in BGRA. `mip 0` is the fitted image; when `generate` is
/// set the chain continues to 1x1 by 2x2 box filtering (odd edges average
/// their surviving pixels). BGRA conversion happens once, up front, so every
/// level is engine-ready.
pub fn mip_chain(img: &Decoded, generate: bool) -> Vec<Mip> {
    let mut out = vec![Mip {
        width: img.width,
        height: img.height,
        bgra: rgba_to_bgra(&img.rgba),
    }];
    if !generate {
        return out;
    }
    while let Some(top) = out.last() {
        if top.width <= 1 && top.height <= 1 {
            break;
        }
        let (bgra, w, h) = halve_bgra(&top.bgra, top.width, top.height);
        out.push(Mip {
            width: w,
            height: h,
            bgra,
        });
    }
    out
}

/// Flip the image vertically in place (row order — source-art orientation,
/// for callers whose art is bottom-up).
pub fn flip_vertical(img: &mut Decoded) {
    let row = img.width as usize * 4;
    let rows = img.height as usize;
    for y in 0..rows / 2 {
        let (top, bottom) = (y * row, (rows - 1 - y) * row);
        for i in 0..row {
            img.rgba.swap(top + i, bottom + i);
        }
    }
}

/// RGBA8 → BGRA8 (R and B swap).
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    out
}

/// One 2x2 box-filter step. Output is `(w/2, h/2)` (never below 1x1); odd
/// source edges contribute their single pixel at full weight
fn halve_bgra(src: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        let sy0 = (2 * y).min(h - 1) as usize;
        let sy1 = (2 * y + 1).min(h - 1) as usize;
        for x in 0..nw {
            let sx0 = (2 * x).min(w - 1) as usize;
            let sx1 = (2 * x + 1).min(w - 1) as usize;
            let corners = [
                sy0 * w as usize + sx0,
                sy0 * w as usize + sx1,
                sy1 * w as usize + sx0,
                sy1 * w as usize + sx1,
            ];
            let dst = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                let sum: u32 = corners.iter().map(|&i| src[i * 4 + c] as u32).sum();
                out[dst + c] = (sum / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}
