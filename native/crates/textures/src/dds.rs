//! Narrow DDS reader for direct BC1 uploads. Never decodes or re-encodes pixels.

use crate::img::{Compression, Mip, validate_chain};

const HEADER_BYTES: usize = 128;
const DX10_HEADER_BYTES: usize = 20;
const DDPF_FOURCC: u32 = 0x4;
const DDSD_MIPMAPCOUNT: u32 = 0x20000;
const DDSD_DEPTH: u32 = 0x800000;
const DDSCAPS2_CUBEMAP: u32 = 0xfe00;
const DDSCAPS2_VOLUME: u32 = 0x200000;

/// Read a little-endian header word, including bounds checking.
fn word(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or("truncated DDS header")?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

/// Load one ordinary 2D BC1 DDS texture. With `mips` set, preserve all stored
/// levels (even a partial chain); otherwise upload only mip 0. The entire
/// declared payload is checked before allocating mip buffers in either mode.
pub(crate) fn load_bc1(bytes: &[u8], mips: bool) -> Result<Vec<Mip>, String> {
    if !bytes.starts_with(b"DDS ") {
        return Err("not a DDS file".to_owned());
    }
    if bytes.len() < HEADER_BYTES {
        return Err("truncated DDS header".to_owned());
    }
    if word(bytes, 4)? != 124 || word(bytes, 76)? != 32 {
        return Err("invalid DDS header or pixel-format header size".to_owned());
    }
    let flags = word(bytes, 8)?;
    let height = word(bytes, 12)?;
    let width = word(bytes, 16)?;
    let depth = word(bytes, 24)?;
    let caps2 = word(bytes, 112)?;
    if flags & DDSD_DEPTH != 0 || depth > 1 || caps2 & DDSCAPS2_VOLUME != 0 {
        return Err(
            "DDS volume textures are not supported; use a single 2D BC1 texture".to_owned(),
        );
    }
    if caps2 & DDSCAPS2_CUBEMAP != 0 {
        return Err("DDS cubemaps are not supported; use a single 2D BC1 texture".to_owned());
    }
    if width == 0
        || height == 0
        || width > crate::DEFAULT_MAX_DIM
        || height > crate::DEFAULT_MAX_DIM
    {
        return Err(format!(
            "DDS dimensions must be within 1..={} (got {width}x{height}); resize offline",
            crate::DEFAULT_MAX_DIM
        ));
    }
    if width % 4 != 0 || height % 4 != 0 {
        return Err(format!(
            "DDS BC1 top mip must have dimensions divisible by four (got {width}x{height}); resize offline"
        ));
    }
    if word(bytes, 80)? & DDPF_FOURCC == 0 {
        return Err("unsupported DDS format; only BC1/DXT1 is supported".to_owned());
    }
    let fourcc = bytes.get(84..88).ok_or("truncated DDS pixel format")?;
    let payload_start = match fourcc {
        b"DXT1" => HEADER_BYTES,
        b"DX10" => {
            if bytes.len() < HEADER_BYTES + DX10_HEADER_BYTES {
                return Err("truncated DDS DX10 header".to_owned());
            }
            // DXGI_FORMAT_BC1_UNORM / BC1_UNORM_SRGB. Colour space is still
            // controlled by the API's `srgb` option, just as for PNG/JPEG.
            if !matches!(word(bytes, 128)?, 71 | 72) {
                return Err(
                    "unsupported DDS DX10 format; only BC1_UNORM and BC1_UNORM_SRGB are supported"
                        .to_owned(),
                );
            }
            // D3D10_RESOURCE_DIMENSION_TEXTURE2D = 3; cube flag = 4.
            if word(bytes, 132)? != 3 || word(bytes, 136)? & 4 != 0 || word(bytes, 140)? != 1 {
                return Err(
                    "DDS arrays, cubemaps and non-2D resources are not supported".to_owned(),
                );
            }
            // Unknown, straight and opaque alpha are compatible with the
            // existing image path. Premultiplied/custom alpha is not.
            if !matches!(word(bytes, 144)? & 0x7, 0 | 1 | 3) {
                return Err(
                    "DDS premultiplied/custom alpha is not supported; export straight alpha"
                        .to_owned(),
                );
            }
            HEADER_BYTES + DX10_HEADER_BYTES
        }
        _ => return Err("unsupported DDS format; only BC1/DXT1 is supported".to_owned()),
    };

    let declared_mips = word(bytes, 28)?;
    if flags & DDSD_MIPMAPCOUNT != 0 && declared_mips == 0 {
        return Err("DDS declares a zero-length mip chain".to_owned());
    }
    // Honor a nonzero count even if the writer omitted DDSD_MIPMAPCOUNT.
    let mip_count = declared_mips.max(1);
    let max_mips = u32::BITS - width.max(height).leading_zeros();
    if mip_count > max_mips {
        return Err(format!(
            "DDS has {mip_count} mips, but {width}x{height} permits at most {max_mips}"
        ));
    }

    // Geometry is bounded before computing sizes, including on 32-bit hosts.
    // Validate every level before copying anything, even when mips=false.
    let mut specs = Vec::with_capacity(mip_count as usize);
    let (mut w, mut h, mut end) = (width, height, payload_start);
    for i in 0..mip_count {
        let size = Compression::Bc1.payload_size(w, h);
        let start = end;
        end = start.checked_add(size).ok_or("DDS payload size overflow")?;
        if end > bytes.len() {
            return Err(format!(
                "truncated DDS mip {i}: need {size} bytes at offset {start}"
            ));
        }
        specs.push((w, h, start..end));
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if end != bytes.len() {
        return Err("DDS has trailing data beyond its declared 2D mip chain".to_owned());
    }
    let count = if mips { specs.len() } else { 1 };
    let chain = specs
        .into_iter()
        .take(count)
        .map(|(width, height, range)| Mip {
            width,
            height,
            compression: Compression::Bc1,
            data: bytes[range].to_vec(),
        })
        .collect::<Vec<_>>();
    validate_chain(&chain)?;
    Ok(chain)
}
