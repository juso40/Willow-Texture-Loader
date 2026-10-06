//! Engine-side `Texture2D` writer: build or patch a UE3 `Texture2D` from a
//! BGRA mip chain ([`crate::img`]).
//!
//! **Every function here must run on the game thread**, they construct
//! objects, mutate native structs and rebuild the render resource.
//! PythonSDK runs mods on the game thread. A mod that spawns its
//! own threads must marshal engine calls back to the game thread itself.
//!
//! - [`layout`]: the `Mips` field model (the mip block layout itself lives
//!   in [`crate::bulkdata`]).
//! - [`write`]: the indirect `Mips` writer and the engine-owned allocations.
//! - [`state`]: the property state every writer stamps.
//! - [`resource`]: the render-resource rebuild.
//! - [`api`]: create / swap / unload ([`LoadedTexture`], [`CreateOptions`]).

/// The `Texture2D` class name (short form, looked up via
/// `unrealsdk_rs::objects::find_engine_class`).
pub(crate) const CLASS_TEXTURE_2D: &str = "Texture2D";

/// The engine's `Transient` package, the default outer for created textures.
pub(crate) const TRANSIENT_PACKAGE: &str = "Transient";


pub(crate) mod api;
pub(crate) mod layout;
pub(crate) mod resource;
pub(crate) mod state;
pub(crate) mod write;

pub use api::{AddressMode, CreateOptions, FilterMode, LoadedTexture};

pub(crate) use api::{
    create_texture, load_into_texture, unload_texture,
};
