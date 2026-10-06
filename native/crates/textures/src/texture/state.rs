//! Property state every writer stamps before the render-resource rebuild.

use unrealsdk_rs::objects::Obj;
use unrealsdk_rs::set;

use crate::error::TextureError;
use crate::img::Mip;
use crate::ue::enums::resolve_bgra_pixel_format;
use crate::ue::props::{optional_set, prop, set_prop_if_present};

use crate::ue::flags::{TMGS_LEAVE_EXISTING_MIPS, TMGS_NO_MIPMAPS};

use super::api::CreateOptions;

/// Sizes, format, mip tail and streaming state, everything both the create
/// and the swap path stamp (before [`super::resource::call_update_resource`]).
/// `no_tiling` is the `bNoTiling` hint: create passes `!opts.tiling`, the
/// swap path keeps passing `true`).
pub(crate) fn apply_common_state(
    tex: Obj,
    chain: &[Mip],
    no_tiling: bool,
) -> Result<(), TextureError> {
    let full = &chain[0];
    set_prop_if_present(tex, prop::SIZE_X, full.width as i32);
    set_prop_if_present(tex, prop::SIZE_Y, full.height as i32);
    set_prop_if_present(tex, prop::FORMAT, resolve_bgra_pixel_format()?);
    // Probe evidence: the engine's textures put the tail one past the last
    // full-size mip (6 mips → MipTailBaseIdx 5, 8 → 7).
    set_prop_if_present(tex, prop::MIP_TAIL_BASE_IDX, chain.len() as i32 - 1);
    // Stop the game from trying to re-stream the texture in (overwriting our changes).
    set_prop_if_present(tex, prop::NEVER_STREAM, true);
    optional_set(set!(tex, bNoTiling, no_tiling));
    set_streaming_and_mipgen_state(tex, chain.len());
    Ok(())
}

/// Fresh-texture-only state: original size, sRGB, address mode, filter,
/// `LODGroup`. `lod_group` is already resolved to its `TextureGroup`
/// value (unknown entries fail before the object exists see
/// [`super::api::create_texture`]).
pub(crate) fn apply_creation_state(
    tex: Obj,
    opts: &CreateOptions,
    chain: &[Mip],
    lod_group: Option<u8>,
) {
    let full = &chain[0];
    optional_set(set!(tex, OriginalSizeX, full.width as i32));
    optional_set(set!(tex, OriginalSizeY, full.height as i32));
    set_prop_if_present(tex, prop::SRGB, opts.srgb);
    optional_set(set!(tex, AddressX, opts.address_x.as_ta()));
    optional_set(set!(tex, AddressY, opts.address_y.as_ta()));
    optional_set(set!(tex, Filter, opts.filter.as_tf()));
    if let Some(group) = lod_group {
        optional_set(set!(tex, LODGroup, group));
    }
}

/// Streaming + mip-gen state shared by every writer, set **before** the
/// resource rebuild ([`super::resource::call_update_resource`]):
///
/// - `RequestedMips`/`ResidentMips` must describe the mip chain we wrote —
///   the resource built by `UpdateResource()` sizes its GPU upload from
///   `Mips` + these counters (`Texture2D.uc`; zero here would mean "no mips
///   resident").
/// - The compression guards mirror what the engine's own `Texture2D.Create`
///   stamps on fresh textures (verified in `UTexture2D::execCreate`):
///   `CompressionNoAlpha` + `CompressionNone` on, `DeferCompression` off —
///   and they keep
///   `PostEditChangeProperty`'s per-property fixups from treating the fresh
///   BGRA as compressible source art (the game ships NVTT and will happily
///   recompress). `CompressionSettings` is deliberately left alone.
/// - `MipGenSettings` documents that the chain is ours:
///   [`TMGS_NO_MIPMAPS`] for a single mip, [`TMGS_LEAVE_EXISTING_MIPS`] when
///   we upload a full chain.
fn set_streaming_and_mipgen_state(tex: Obj, mip_count: usize) {
    let mips = mip_count as i32;
    set_prop_if_present(tex, prop::REQUESTED_MIPS, mips);
    set_prop_if_present(tex, prop::RESIDENT_MIPS, mips);
    set_prop_if_present(tex, prop::COMPRESSION_NONE, true);
    optional_set(set!(tex, CompressionNoAlpha, true));
    optional_set(set!(tex, DeferCompression, false));
    set_prop_if_present(
        tex,
        prop::MIP_GEN_SETTINGS,
        if mip_count > 1 {
            TMGS_LEAVE_EXISTING_MIPS
        } else {
            TMGS_NO_MIPMAPS
        },
    );
}
