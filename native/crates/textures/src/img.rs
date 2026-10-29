//! Pure image pipeline: decode → fit → mip chain → optional BC1 encoding.
//!
//! Produces engine-ready BGRA (`PF_A8R8G8B8`) or BC1 (`PF_DXT1`) payloads.

use image::{DynamicImage, ImageFormat};

use fast_image_resize as fir;
use fir::images::{Image, ImageRef};


/// Storage encoding for generated mip payloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compression {
    /// Uncompressed BGRA8, preserving full colour and alpha.
    #[default]
    None,
    /// BC1/DXT1: eight bytes per 4x4 block, with one-bit alpha.
    Bc1,
}

impl Compression {
    /// Parse the public API spelling (case-insensitive).
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "bc1" | "dxt1" => Ok(Self::Bc1),
            other => Err(format!(
                "unknown compression '{other}' (want 'none', 'bc1' or 'dxt1')"
            )),
        }
    }

    /// Entry name in the engine's `EPixelFormat` enum.
    pub fn pixel_format_name(self) -> &'static str {
        match self {
            Self::None => "PF_A8R8G8B8",
            Self::Bc1 => "PF_DXT1",
        }
    }

    /// Payload size for a mip, including complete BC1 blocks in tiny tails.
    pub fn payload_size(self, width: u32, height: u32) -> usize {
        let (w, h) = (width as usize, height as usize);
        match self {
            Self::None => w * h * 4,
            Self::Bc1 => w.div_ceil(4) * h.div_ceil(4) * 8,
        }
    }
}

/// One mip level in an engine-ready encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mip {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Encoding shared by every mip in the chain.
    pub compression: Compression,
    /// BGRA8 pixels or BC1 blocks, ready to copy to engine bulk data.
    pub data: Vec<u8>,
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
    (resize(img, tw, th), notes)
}

/// Resize a decoded image to exact dimensions (both must be nonzero).
fn resize(img: &Decoded, tw: u32, th: u32) -> Decoded {
    let src = ImageRef::new(img.width, img.height, &img.rgba, fir::PixelType::U8x4)
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

    Decoded {
        width: tw,
        height: th,
        rgba: dst.into_vec(),
    }
}

/// Align BC1's top mip to full 4x4 blocks for D3D9 texture creation.
/// Resizes (rather than padding) so existing UVs still cover the entire art.
pub fn align_for_bc1(img: &Decoded) -> (Decoded, Vec<String>) {
    let (w, h) = (img.width, img.height);
    let (tw, th) = (w.div_ceil(4).max(1) * 4, h.div_ceil(4).max(1) * 4);
    if (w, h) == (tw, th) {
        return (img.clone(), Vec::new());
    }
    (
        resize(img, tw, th),
        vec![format!(
            "resized {w}x{h} -> {tw}x{th} (BC1 requires a block-aligned top mip)"
        )],
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
        compression: Compression::None,
        data: rgba_to_bgra(&img.rgba),
    }];
    if !generate {
        return out;
    }
    while let Some(top) = out.last() {
        if top.width <= 1 && top.height <= 1 {
            break;
        }
        let (data, w, h) = halve_bgra(&top.data, top.width, top.height);
        out.push(Mip {
            width: w,
            height: h,
            compression: Compression::None,
            data,
        });
    }
    out
}

/// Encode an uncompressed mip chain as BC1, discarding the BGRA buffers.
/// Filtering happens before encoding, never on compressed blocks.
pub fn compress_bc1(chain: &mut [Mip]) {
    let format = texpresso::Format::Bc1;
    for mip in chain {
        let rgba = rgba_to_bgra(&mip.data); // the R/B swap is its own inverse
        let (w, h) = (mip.width as usize, mip.height as usize);
        let mut data = vec![0; format.compressed_size(w, h)];
        format.compress(&rgba, w, h, texpresso::Params::default(), &mut data);
        mip.data = data;
        mip.compression = Compression::Bc1;
    }
}

/// Validate payload sizes, encoding and mip geometry before engine allocation.
pub fn validate_chain(chain: &[Mip]) -> Result<(), String> {
    let Some(top) = chain.first() else {
        return Err("empty mip chain".to_owned());
    };
    if top.compression == Compression::Bc1 && (top.width % 4 != 0 || top.height % 4 != 0) {
        return Err("BC1 top mip dimensions must be multiples of four".to_owned());
    }
    let (mut w, mut h) = (top.width, top.height);
    for (i, mip) in chain.iter().enumerate() {
        if w == 0 || h == 0 || w > crate::DEFAULT_MAX_DIM || h > crate::DEFAULT_MAX_DIM {
            return Err(format!("mip {i}: invalid dimensions {w}x{h}"));
        }
        if (mip.width, mip.height) != (w, h) || mip.compression != top.compression {
            return Err(format!("mip {i}: inconsistent geometry or encoding"));
        }
        let expected = mip.compression.payload_size(w, h);
        if mip.data.len() != expected {
            return Err(format!(
                "mip {i}: expected {expected} bytes, got {}",
                mip.data.len()
            ));
        }
        if w == 1 && h == 1 && i + 1 < chain.len() {
            return Err("mip chain continues past 1x1".to_owned());
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Ok(())
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
