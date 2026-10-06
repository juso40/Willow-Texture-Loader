//! The `Mips` field model on `Texture2D`: where the array header lives and
//! what shape it has.
//!
//! ## Layout discipline
//!
//! All `UObject`/`UClass` field access goes through the SDK's offset table
//! (`unrealsdk_rs::load_offsets`).
//! On BL2 the script `Mips` field is a `StructProperty` whose
//! 12-byte payload **is** the `TArray<FTexture2DMipMap*>` header, it is an
//! *indirect* array: elements are 4-byte pointers to individually allocated
//! mip blocks. The block layout itself ([`MipBlock`]) is engine-core
//! (`FUntypedBulkData` + dims) and lives in [`crate::bulkdata`], with
//! the script-mirror derivation.

use std::ffi::c_void;
use std::sync::OnceLock;

pub(crate) use crate::bulkdata::MipBlock;
use unrealsdk_rs::mem::Offset;
use unrealsdk_rs::objects::{Error as ScriptError, Obj};
use unrealsdk_rs::objects::{object_class, objects, property_class_name, property_offset};
use unrealsdk_rs::types::{TArray, UObject};

use crate::error::TextureError;
use crate::ue::props::{prop, prop_class};

/// The `Mips` field offset on `tex`'s class.
pub(crate) fn find_mips_offset(tex: Obj) -> Result<Offset, TextureError> {
    let prop = tex.find_prop(prop::MIPS).map_err(|e| match e {
        ScriptError::NotFound(_) => {
            TextureError::NotFound("Texture2D has no script-visible 'Mips' property".to_owned())
        }
        other => TextureError::from(other),
    })?;
    let offset = property_offset(prop)
        .and_then(Offset::from_reflected)
        .ok_or_else(|| {
            TextureError::Implausible("Texture2D.Mips has an invalid offset".to_owned())
        })?;
    match property_class_name(prop).unwrap_or_default().as_str() {
        prop_class::STRUCT => Ok(offset),
        other => Err(TextureError::UnsupportedLayout(format!(
            "Texture2D.Mips is a '{other}', this build's writer only knows the BL2 \
             StructProperty (indirect pointer array) form."
        ))),
    }
}

/// The `Mips` header as a typed [`TArray`] view over the mip blocks.
pub(crate) unsafe fn mip_array(obj: *mut UObject, offset: Offset) -> TArray<*mut c_void> {
    // SAFETY: caller contract, live object, validated property offset.
    unsafe { TArray::read((obj as *const c_void).byte_add(offset.raw())) }
}

/// The vtable pointer of a live [`MipBlock`], copied from any `Texture2D`
/// in GObjects that has mips. The engine's own `Init` stamps a
/// build-constant there (0x016abb54 on the BL2 binary).
/// Cached on first success.
pub(crate) fn mip_block_vtable(tex: Obj) -> Option<*mut c_void> {
    static CACHE: OnceLock<usize> = OnceLock::new();
    if let Some(v) = CACHE.get() {
        return Some(*v as *mut c_void);
    }
    let offset = find_mips_offset(tex).ok()?;
    let cls = object_class(tex.raw())?;
    for obj in objects()?.iter().copied() {
        if object_class(obj) != Some(cls) {
            continue;
        }
        // SAFETY: live texture; offset from the live property table.
        let array = unsafe { mip_array(obj, offset) };
        if array.data.is_null() || array.count <= 0 {
            continue;
        }
        // SAFETY: live pointer array with `count >= 1` entries.
        let block = MipBlock::from_raw(unsafe { array.element(0) });
        if block.as_ptr().is_null() {
            continue;
        }
        // SAFETY: live mip block; +0 is its vftable.
        let vtable = unsafe { block.vtable() };
        if vtable.is_null() {
            continue;
        }
        let _ = CACHE.set(vtable as usize);
        return Some(vtable);
    }
    None
}
