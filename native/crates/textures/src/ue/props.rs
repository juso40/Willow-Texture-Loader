//! Names the writers touch lives here as a const, so their
//! spelling cannot drift apart and a typo is a compile error. A name used
//! once at its call site stays there instead, spelled through the
//! `unrealsdk_rs::objects` macros e.g. `set!(tex, OriginalSizeX, w)`

use unrealsdk_rs::objects::{Error, Obj};

/// `Texture2D`/`Texture` properties the writers set.
pub(crate) mod prop {
    pub(crate) const MIPS: &str = "Mips";
    pub(crate) const FORMAT: &str = "Format";
    pub(crate) const SIZE_X: &str = "SizeX";
    pub(crate) const SIZE_Y: &str = "SizeY";
    pub(crate) const SRGB: &str = "SRGB";
    pub(crate) const MIP_TAIL_BASE_IDX: &str = "MipTailBaseIdx";
    pub(crate) const NEVER_STREAM: &str = "NeverStream";
    pub(crate) const REQUESTED_MIPS: &str = "RequestedMips";
    pub(crate) const RESIDENT_MIPS: &str = "ResidentMips";
    pub(crate) const COMPRESSION_NONE: &str = "CompressionNone";
    pub(crate) const MIP_GEN_SETTINGS: &str = "MipGenSettings";
}

/// Property class names (`UProperty` class name) matched against live
/// reflection data.
pub(crate) mod prop_class {
    pub(crate) const STRUCT: &str = "StructProperty";
}

/// The writers' set policy over a [`Obj::set`] result: a property this
/// build does not have is skipped quietly, a real write failure logs a warning.
/// Returns whether the property exists on this build.
pub(crate) fn optional_set(result: Result<(), Error>) -> bool {
    match result {
        Ok(()) => true,
        Err(Error::NotFound(_)) => false,
        Err(e) => {
            log::warn!("texloader: {e}");
            true
        }
    }
}

/// Set one script property by name if it exists.
/// Same policy as [`optional_set`].
pub(crate) fn set_prop_if_present(
    tex: Obj,
    name: &str,
    value: impl Into<unrealsdk_rs::objects::PropValue>,
) -> bool {
    optional_set(tex.set(name, value))
}
