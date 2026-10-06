//! `Texture` enum resolution through the engine's own `UEnum`s.

use unrealsdk_rs::objects::{UEnumInfo, all_uenums, enum_names};
use unrealsdk_rs::types::{UEnum, UObject};

use crate::error::TextureError;

/// The engine's pixel-format enum name.
const ENUM_PIXEL_FORMAT: &str = "EPixelFormat";

/// The engine's texture-group enum name.
const ENUM_TEXTURE_GROUP: &str = "TextureGroup";

/// Class-scoped path: `Engine.Texture:<name>`.
const PATH_CLASS_SCOPED: &str = "Engine.Texture:";

/// BGRA-in-memory entry name.
const BGRA_ENTRY: &str = "PF_A8R8G8B8";

/// Resolve the BGRA pixel-format value from the engine's own `UEnum`.
pub(crate) fn resolve_bgra_pixel_format() -> Result<u8, TextureError> {
    let (enum_obj, source) = texture_enum(ENUM_PIXEL_FORMAT)?;
    bgra_from_enum(enum_obj, &source)
}

/// Resolve a `TextureGroup` entry (the `LODGroup` value) from the engine's
/// own `UEnum`. Accepts the full entry name or its short form,
/// case-insensitive (`"TEXTUREGROUP_UI"`, `"ui"` and `"TF_…"`-style
/// spellings all match, see [`entry_matches`]).
pub(crate) fn resolve_texture_group(value: &str) -> Result<u8, TextureError> {
    resolve_entry(ENUM_TEXTURE_GROUP, value)
}

/// Locate a `Texture`-family `UEnum` by short name with a label naming
/// where it was found (for error messages): the class-scoped path first
/// then a GObjects scan.
fn texture_enum(name: &str) -> Result<(*mut UObject, String), TextureError> {
    let path = format!("{PATH_CLASS_SCOPED}{name}");
    if let Some(enum_obj) = unrealsdk_rs::objects::find_object(None, &path) {
        return Ok((enum_obj, path));
    }
    let found = all_uenums()
        .unwrap_or_default()
        .into_iter()
        .find(|e| e.name == name);
    found
        .map(|UEnumInfo { obj, path, .. }| (obj, format!("GObjects scan: {path}")))
        .ok_or_else(|| TextureError::NotFound(format!("{name} enum not found")))
}

/// The value of enum entry `wanted` (see [`entry_matches`] for accepted
/// spellings), resolved from the live `UEnum`s name list.
fn resolve_entry(enum_name: &str, wanted: &str) -> Result<u8, TextureError> {
    let (enum_obj, source) = texture_enum(enum_name)?;
    let names = enum_names(enum_obj as *const UEnum).ok_or_else(|| {
        TextureError::Engine(format!("{enum_name} enum ({source}) is unreadable"))
    })?;
    names
        .iter()
        .position(|entry| entry_matches(entry, wanted))
        .and_then(|idx| u8::try_from(idx).ok())
        .ok_or_else(|| {
            TextureError::InvalidInput(format!(
                "unknown {enum_name} entry '{wanted}' ({source}); valid: {}",
                short_names(&names).join(", ")
            ))
        })
}

/// Accepted user spellings for one enum entry: the entry's full name, or
/// its name past the first `_` (`"TEXTUREGROUP_UI"`/`"UI"`/`"ui"` name the
/// same entry; `"TF_Nearest"`/`"nearest"` too). Case-insensitive.
fn entry_matches(entry: &str, wanted: &str) -> bool {
    let wanted = wanted.trim();
    let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    if eq(entry, wanted) {
        return true;
    }
    match entry.split_once('_') {
        Some((prefix, rest)) if !prefix.is_empty() => eq(rest, wanted),
        _ => false,
    }
}

/// Entries with their common prefix stripped, for error messages
/// (`TEXTUREGROUP_UI` → `UI`).
fn short_names(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .map(|n| n.split_once('_').map_or(n.as_str(), |(_, rest)| rest))
        .collect()
}

/// BGRA index from a resolved `EPixelFormat` enum object.
fn bgra_from_enum(enum_obj: *mut UObject, source: &str) -> Result<u8, TextureError> {
    let names = enum_names(enum_obj as *const UEnum).ok_or_else(|| {
        TextureError::Engine(format!("EPixelFormat enum ({source}) is unreadable"))
    })?;
    if let Some(idx) = names.iter().position(|n| n == BGRA_ENTRY) {
        return Ok(idx as u8);
    }
    Err(TextureError::UnsupportedFormat(format!(
        "no BGRA entry PF_A8R8G8B8 in EPixelFormat ({source}): {names:?}"
    )))
}
