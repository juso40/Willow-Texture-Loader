//! Public engine operations: create, swap, unload.

use std::ffi::c_void;

use unrealsdk_rs::flags::object_flags::{RF_PUBLIC, RF_ROOT_SET, RF_STANDALONE};
use unrealsdk_rs::objects::{
    Obj, construct_object, find_engine_class, find_object, is_kind_of, object_path_text,
};
use unrealsdk_rs::types::{UClass, UObject};

use crate::error::TextureError;
use crate::img::Mip;
use crate::names::sanitize_name;
use crate::ue::enums::resolve_texture_group;
use crate::ue::flags::{TA_CLAMP, TA_MIRROR, TA_WRAP, TF_LINEAR, TF_NEAREST};

use super::{CLASS_TEXTURE_2D, TRANSIENT_PACKAGE};

use super::layout::find_mips_offset;
use super::resource::call_update_resource;
use super::state::{apply_common_state, apply_creation_state};
use super::write::write_mips_indirect;

/// Auto-suffix attempts on name collision (`logo`, `logo_1`, ...) before
/// giving up.
const MAX_NAME_SUFFIX: u32 = 10;

/// A successfully created or patched texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedTexture {
    /// Full object path, e.g. `Transient.MyMod.logo`.
    pub path: String,
    /// Width of mip 0 in pixels.
    pub width: u32,
    /// Height of mip 0 in pixels.
    pub height: u32,
    /// Number of mips written.
    pub mip_count: usize,
}

/// Edge addressing for created textures (`ETextureAddressMode`,
/// Engine/Classes/Texture.uc: `TA_Wrap = 0`, `TA_Clamp = 1`,
/// `TA_Mirror = 2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddressMode {
    /// `TA_Wrap`: tile the texture (tiling art).
    Wrap,
    /// `TA_Clamp`: repeat the edge pixels (UI/one-shot art). The default.
    #[default]
    Clamp,
    /// `TA_Mirror`: mirror every repeat.
    Mirror,
}

impl AddressMode {
    /// The `TA_*` value written to `AddressX`/`AddressY`.
    pub fn as_ta(self) -> u8 {
        match self {
            Self::Wrap => TA_WRAP,
            Self::Clamp => TA_CLAMP,
            Self::Mirror => TA_MIRROR,
        }
    }

    /// Parse the API spelling (`"wrap"`, `"clamp"`, `"mirror"`;
    /// case-insensitive). Errors name the offending value.
    pub fn parse(value: &str) -> Result<Self, TextureError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "wrap" => Ok(Self::Wrap),
            "clamp" => Ok(Self::Clamp),
            "mirror" => Ok(Self::Mirror),
            other => Err(TextureError::InvalidInput(format!(
                "unknown address mode '{other}' (want 'wrap', 'clamp' or 'mirror')"
            ))),
        }
    }
}

/// Sampler filtering for created textures (`ETextureFilter`,
/// Engine/Classes/Texture.uc: `TF_Nearest = 0`, `TF_Linear = 1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterMode {
    /// `TF_Nearest`: point sampling (pixel-art icons).
    Nearest,
    /// `TF_Linear`: bilinear sampling. The default.
    #[default]
    Linear,
}

impl FilterMode {
    /// The `TF_*` value written to `Filter`.
    pub fn as_tf(self) -> u8 {
        match self {
            Self::Nearest => TF_NEAREST,
            Self::Linear => TF_LINEAR,
        }
    }

    /// Parse the API spelling (`"linear"`, `"nearest"`;
    /// case-insensitive). Errors name the offending value.
    pub fn parse(value: &str) -> Result<Self, TextureError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "linear" => Ok(Self::Linear),
            "nearest" => Ok(Self::Nearest),
            other => Err(TextureError::InvalidInput(format!(
                "unknown filter mode '{other}' (want 'linear' or 'nearest')"
            ))),
        }
    }
}

/// Options for `create_texture`. `name` is required and sanitized;
/// `outer` is an object path (a `Package` is typical). `None` falls back to
/// the engine's `Transient` package.
#[derive(Clone, Debug)]
pub struct CreateOptions {
    /// Object name; sanitized ([`sanitize_name`]) and auto-suffixed on
    /// collision.
    pub name: String,
    /// Outer object path (a `Package` is typical). `None` = `Transient`.
    pub outer: Option<String>,
    /// Mark the texture sRGB.
    pub srgb: bool,
    /// Generate a full mip chain (else a single mip).
    pub mips: bool,
    /// Edge addressing of the X axis (`AddressX`).
    pub address_x: AddressMode,
    /// Edge addressing of the Y axis (`AddressY`).
    pub address_y: AddressMode,
    /// Flip the source art vertically at load (image-side, not UE data).
    pub flip: bool,
    /// The art tiles (wrap-style). Clears the engine's `bNoTiling` hint.
    /// Defaults to `false` (similar to the game dumps show).
    pub tiling: bool,
    /// Sampler filtering (`Filter`).
    pub filter: FilterMode,
    /// `LODGroup` entry name (e.g. `"ui"`, `"effects"`) resolved through
    /// the live `TextureGroup` UEnum at creation; `None` leaves the
    /// engine default.
    pub lod_group: Option<String>,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            outer: None,
            srgb: true,
            mips: true,
            address_x: AddressMode::default(),
            address_y: AddressMode::default(),
            flip: false,
            tiling: false,
            filter: FilterMode::default(),
            lod_group: None,
        }
    }
}

/// The `Texture2D` class (see [`find_engine_class`] for the lookup order).
pub(crate) fn texture2d_class() -> Result<*mut UClass, TextureError> {
    find_engine_class(CLASS_TEXTURE_2D)
        .ok_or_else(|| TextureError::NotFound("Class Texture2D not found!".to_owned()))
}

/// Check that `obj` is a `Texture2D`.
pub(crate) fn ensure_texture2d(obj: *mut UObject, path: &str) -> Result<Obj, TextureError> {
    let cls = texture2d_class()?;
    match is_kind_of(obj, cls) {
        Some(true) => Obj::new(obj).map_err(TextureError::from),
        Some(false) => Err(TextureError::NotATexture(format!(
            "'{path}' is not a Texture2D"
        ))),
        None => Err(TextureError::OffsetsUnavailable),
    }
}

/// Resolve `path` to a live `Texture2D` scripting handle.
pub(crate) fn require_texture2d(path: &str) -> Result<Obj, TextureError> {
    let obj = find_object(None, path)
        .ok_or_else(|| TextureError::NotFound(format!("texture '{path}' not found")))?;
    ensure_texture2d(obj, path)
}

/// Construct a fresh `Texture2D` under `outer` (default `Transient`) with the
/// given mip chain.
pub fn create_texture(opts: &CreateOptions, chain: &[Mip]) -> Result<LoadedTexture, TextureError> {
    if chain.is_empty() {
        return Err(TextureError::NoMips);
    }
    let cls = texture2d_class()?;

    // Resolve the CreationOptions settings: a bad value fails the call
    // before an object exists.
    let lod_group = opts
        .lod_group
        .as_deref()
        .map(resolve_texture_group)
        .transpose()?;

    let outer = match &opts.outer {
        Some(path) => find_object(None, path)
            .ok_or_else(|| TextureError::NotFound(format!("outer object '{path}' not found")))?,
        None => find_object(None, TRANSIENT_PACKAGE).ok_or_else(|| {
            TextureError::NotFound(
                "the engine's Transient package was not found; pass an explicit outer".to_owned(),
            )
        })?,
    };
    let outer_path = object_path_text(outer).ok_or_else(|| {
        TextureError::Engine("failed to resolve the outer object's path".to_owned())
    })?;

    let base_name = sanitize_name(&opts.name);
    // Auto-suffix on collision (`logo`, `logo_1`, ...)
    let mut candidate = base_name.clone();
    let mut suffix = 1;
    while find_object(None, &format!("{outer_path}.{candidate}")).is_some() {
        if suffix > MAX_NAME_SUFFIX {
            return Err(TextureError::InvalidInput(format!(
                "could not find a free name under '{outer_path}'"
            )));
        }
        candidate = format!("{base_name}_{suffix}");
        suffix += 1;
    }

    let obj = construct_object(
        cls,
        outer,
        Some(&candidate),
        RF_PUBLIC | RF_STANDALONE | RF_ROOT_SET,
        None,
    )
    .ok_or_else(|| TextureError::Engine("construct_object(Texture2D) failed".to_owned()))?;
    let tex = Obj::new(obj).map_err(TextureError::from)?;
    // Setting the flags after construction does not seem to be required actually!
    // tex.with_flags(|flags| flags | RF_PUBLIC | RF_STANDALONE | RF_ROOT_SET)?;

    let mips_at = mips_header_at(tex)?;

    apply_creation_state(tex, opts, chain, lod_group);
    apply_common_state(tex, chain, !opts.tiling)?;

    // SAFETY: `mips_at` is the texture's `Mips` header (freshly constructed
    // object, zeroed array).
    unsafe { write_mips_indirect(mips_at, tex, chain) }?;
    call_update_resource(tex)?;

    Ok(LoadedTexture {
        path: format!("{outer_path}.{candidate}"),
        width: chain[0].width,
        height: chain[0].height,
        mip_count: chain.len(),
    })
}

/// Replace the mip chain (and sizes/format) of an existing `Texture2D`.
/// We always uploads BGRA, so any Texture with a cooked format (DXT1/DXT5/…)
/// reads from our new data after only after its next `UpdateResource`.
pub fn load_into_texture(target_path: &str, chain: &[Mip]) -> Result<LoadedTexture, TextureError> {
    if chain.is_empty() {
        return Err(TextureError::NoMips);
    }
    let tex = require_texture2d(target_path)?;

    let mips_at = mips_header_at(tex)?;

    // Swap stays pixels-only: state (addressing, sRGB, tiling) keeps
    // stamping exactly what it always did — `bNoTiling` on included.
    apply_common_state(tex, chain, true)?;

    // SAFETY: `mips_at` is the texture's `Mips` header. The previous array
    // header is overwritten.
    // We create our own mip chain in place, so no memory is freed by us.
    unsafe { write_mips_indirect(mips_at, tex, chain) }?;
    call_update_resource(tex)?;

    Ok(LoadedTexture {
        path: target_path.to_owned(),
        width: chain[0].width,
        height: chain[0].height,
        mip_count: chain.len(),
    })
}

/// Release a previously created texture back to the garbage collector by
/// clearing its `Standalone` flag (the engine's next GC pass collects it).
/// Returns whether the object was found and patched.
pub fn unload_texture(path: &str) -> Result<bool, TextureError> {
    let Some(obj) = find_object(None, path) else {
        return Ok(false);
    };
    let tex = ensure_texture2d(obj, path)?;
    tex.with_flags(|flags| flags & !(RF_STANDALONE | RF_ROOT_SET | RF_PUBLIC))
        .map_err(|err| TextureError::Implausible(err.to_string()))?;

    Ok(true)
}

/// The address of the texture's `Mips` array header, from the live layout.
fn mips_header_at(tex: Obj) -> Result<*mut c_void, TextureError> {
    let offset = find_mips_offset(tex)?;
    // SAFETY: `tex` is a live object; `offset` a validated property offset.
    Ok(unsafe { (tex.raw() as *mut c_void).byte_add(offset.raw()) })
}
