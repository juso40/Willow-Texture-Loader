//! textures: load PNG/JPEG images and pre-encoded BC1 DDS textures
//! into UE3 `Texture2D` objects at runtime.

pub mod bulkdata;
pub mod img;

mod dds;
mod error;
mod names;
mod texture;
mod ue;


pub use error::TextureError;
pub use img::Compression;
pub use names::sanitize_name;
pub use texture::{AddressMode, CreateOptions, FilterMode, LoadedTexture};

pub use unrealsdk_rs::logging::install_log_bridge;

/// Upper bound for either texture dimension.
/// Larger PNG/JPEG sources are downscaled (aspect-preserving) and logged.
/// DDS sources must fit the cap already; they are never decoded or resized.
pub const DEFAULT_MAX_DIM: u32 = 8192;

/// Read an image file's bytes (shared by the file-based entry points).
fn read_image_file(path: &str) -> Result<Vec<u8>, TextureError> {
    std::fs::read(path).map_err(|e| TextureError::Io(format!("failed to read '{path}': {e}")))
}

/// DDS: validate and copy stored BC1 blocks. PNG/JPEG: decode → fit → mip
/// chain → optional compression, logging fit notes. `flip` is only supported
/// for decoded images; DDS stays on the no-decode, no-recompression path.
fn prepare_chain(
    bytes: &[u8],
    mips: bool,
    flip: bool,
    compression: Compression,
) -> Result<Vec<img::Mip>, TextureError> {
    if bytes.starts_with(b"DDS ") {
        if flip {
            return Err(TextureError::InvalidInput(
                "flip=true is not supported for pre-encoded DDS; flip the image before exporting DDS".to_owned(),
            ));
        }
        return dds::load_bc1(bytes, mips).map_err(TextureError::Image);
    }
    let mut decoded = img::decode_rgba(bytes).map_err(TextureError::Image)?;
    if flip {
        img::flip_vertical(&mut decoded);
    }
    let (mut fitted, mut notes) = img::fit(&decoded, DEFAULT_MAX_DIM);
    if compression == Compression::Bc1 {
        let (aligned, alignment_notes) = img::align_for_bc1(&fitted);
        fitted = aligned;
        notes.extend(alignment_notes);
        if fitted
            .rgba
            .chunks_exact(4)
            .any(|px| px[3] != 0 && px[3] != 255)
        {
            log::warn!(
                "texloader: BC1 has one-bit alpha; smooth transparency will be lost (use compression='none' to preserve it)"
            );
        }
    }
    for note in &notes {
        log::info!("texloader: {note}");
    }
    let mut chain = img::mip_chain(&fitted, mips);
    if compression == Compression::Bc1 {
        img::compress_bc1(&mut chain);
    }
    Ok(chain)
}

/// Load an image file as a **new** `Texture2D`. Returns the full object path.
pub fn load_image(path: &str, opts: &CreateOptions) -> Result<LoadedTexture, TextureError> {
    let bytes = read_image_file(path)?;
    load_image_bytes(&bytes, opts)
}

/// Load in-memory PNG/JPEG or BC1 DDS bytes as a **new** `Texture2D`.
/// DDS preserves its BC1 encoding regardless of `opts.compression`.
pub fn load_image_bytes(bytes: &[u8], opts: &CreateOptions) -> Result<LoadedTexture, TextureError> {
    if opts.name.trim().is_empty() {
        return Err(TextureError::InvalidInput(
            "a texture name is required (opts.name)".to_owned(),
        ));
    }
    let chain = prepare_chain(bytes, opts.mips, opts.flip, opts.compression)?;
    texture::create_texture(opts, &chain)
}

/// Replace the mip chain of an **existing** texture (by object path) with an
/// image file's content.
pub fn load_image_into(
    path: &str,
    target: &str,
    mips: bool,
    compression: Compression,
) -> Result<LoadedTexture, TextureError> {
    let bytes = read_image_file(path)?;
    load_image_bytes_into(&bytes, target, mips, compression)
}

/// Replace the mip chain of an **existing** texture with in-memory image
/// bytes.
pub fn load_image_bytes_into(
    bytes: &[u8],
    target: &str,
    mips: bool,
    compression: Compression,
) -> Result<LoadedTexture, TextureError> {
    if target.trim().is_empty() {
        return Err(TextureError::InvalidInput(
            "a target texture path is required".to_owned(),
        ));
    }
    let chain = prepare_chain(bytes, mips, false, compression)?;
    texture::load_into_texture(target, &chain)
}

/// Release a previously created texture to the garbage collector (clears its
/// `Standalone` flag). Returns whether the path existed.
pub fn unload(path: &str) -> Result<bool, TextureError> {
    texture::unload_texture(path)
}
