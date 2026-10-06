//! textures: load arbitrary PNG/JPEG images
//! into UE3 `Texture2D` objects at runtime.

pub mod bulkdata;
pub mod img;


mod error;
mod names;
mod texture;
mod ue;

pub use error::TextureError;
pub use names::sanitize_name;
pub use texture::{AddressMode, CreateOptions, FilterMode, LoadedTexture};

pub use unrealsdk_rs::logging::install_log_bridge;

/// Upper bound for either texture dimension.
/// Larger sources are downscaled (aspect-preserving) and logged.
pub const DEFAULT_MAX_DIM: u32 = 2048;

/// Read an image file's bytes (shared by the file-based entry points).
fn read_image_file(path: &str) -> Result<Vec<u8>, TextureError> {
    std::fs::read(path).map_err(|e| TextureError::Io(format!("failed to read '{path}': {e}")))
}

/// Decode → fit → mip chain (shared by all load entry points), logging the
/// fit notes. `flip` mirrors the source vertically before resizing (a
/// create-path knob; swaps are pixels-only).
fn prepare_chain(bytes: &[u8], mips: bool, flip: bool) -> Result<Vec<img::Mip>, TextureError> {
    let mut decoded = img::decode_rgba(bytes).map_err(TextureError::Image)?;
    if flip {
        img::flip_vertical(&mut decoded);
    }
    let (fitted, notes) = img::fit(&decoded, DEFAULT_MAX_DIM);
    for note in &notes {
        log::info!("texloader: {note}");
    }
    Ok(img::mip_chain(&fitted, mips))
}

/// Load an image file as a **new** `Texture2D`. Returns the full object path.
pub fn load_image(path: &str, opts: &CreateOptions) -> Result<LoadedTexture, TextureError> {
    let bytes = read_image_file(path)?;
    load_image_bytes(&bytes, opts)
}

/// Load in-memory image bytes (PNG or JPEG) as a **new** `Texture2D`.
pub fn load_image_bytes(bytes: &[u8], opts: &CreateOptions) -> Result<LoadedTexture, TextureError> {
    if opts.name.trim().is_empty() {
        return Err(TextureError::InvalidInput(
            "a texture name is required (opts.name)".to_owned(),
        ));
    }
    let chain = prepare_chain(bytes, opts.mips, opts.flip)?;
    texture::create_texture(opts, &chain)
}

/// Replace the mip chain of an **existing** texture (by object path) with an
/// image file's content.
pub fn load_image_into(
    path: &str,
    target: &str,
    mips: bool,
) -> Result<LoadedTexture, TextureError> {
    let bytes = read_image_file(path)?;
    load_image_bytes_into(&bytes, target, mips)
}

/// Replace the mip chain of an **existing** texture with in-memory image
/// bytes.
pub fn load_image_bytes_into(
    bytes: &[u8],
    target: &str,
    mips: bool,
) -> Result<LoadedTexture, TextureError> {
    if target.trim().is_empty() {
        return Err(TextureError::InvalidInput(
            "a target texture path is required".to_owned(),
        ));
    }
    let chain = prepare_chain(bytes, mips, false)?;
    texture::load_into_texture(target, &chain)
}

/// Release a previously created texture to the garbage collector (clears its
/// `Standalone` flag). Returns whether the path existed.
pub fn unload(path: &str) -> Result<bool, TextureError> {
    texture::unload_texture(path)
}
