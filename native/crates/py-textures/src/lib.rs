//! The Python-facing texture surface of the `texloader._texloader` module.
//!
//! Mod authors load PNG/JPEG files (or raw bytes) as real UE3 `Texture2D`
//! objects and resolve them with the SDK they already have:
//!
//! ```python
//! path = load_image("logo.png", name="logo")
//! tex = unrealsdk.find_object("Texture2D", path)
//! mat.SetTextureParameterValue("p_Diffuse", tex)
//! ```
//!
//! `unload_texture` releases a created texture to the garbage collector.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

fn texture_error(e: textures::TextureError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

// The `#[pyfunction]`s below marshal the Python keyword surface one
// parameter each — the arity is the API, not incidental complexity.
#[allow(clippy::too_many_arguments)]
fn make_opts(
    name: String,
    outer: Option<String>,
    mips: bool,
    srgb: bool,
    flip: bool,
    tiling: bool,
    address_x: Option<String>,
    address_y: Option<String>,
    filter: Option<String>,
    lod_group: Option<String>,
) -> PyResult<textures::CreateOptions> {
    Ok(textures::CreateOptions {
        name,
        outer,
        srgb,
        mips,
        address_x: address(address_x)?,
        address_y: address(address_y)?,
        flip,
        tiling,
        filter: filter_mode(filter)?,
        lod_group,
    })
}

/// `None` → the default (`clamp`); otherwise the API spelling is parsed
/// (bad values raise naming the value).
fn address(value: Option<String>) -> PyResult<textures::AddressMode> {
    match value {
        None => Ok(textures::AddressMode::default()),
        Some(v) => textures::AddressMode::parse(&v).map_err(texture_error),
    }
}

/// `None` → the default (`linear`); otherwise the API spelling is parsed
/// (bad values raise naming the value).
fn filter_mode(value: Option<String>) -> PyResult<textures::FilterMode> {
    match value {
        None => Ok(textures::FilterMode::default()),
        Some(v) => textures::FilterMode::parse(&v).map_err(texture_error),
    }
}

/// Loads an image file (PNG or JPEG) as a new `Texture2D` and returns its
/// full object path (e.g. `"Transient.MyMod.logo"` — resolve it with
/// `unrealsdk.find_object("Texture2D", path)`). `name` is required. `outer`
/// optionally names an existing object to construct under (e.g. a `Package`),
/// defaulting to the engine's `Transient` package. Name collisions
/// get an automatic `_1`-style suffix. Textures are sRGB by default,
/// GC-immune and live for the session (`unload_texture` releases one).
/// Edge addressing is `"clamp"` by default — `"wrap"`/`"mirror"` (also per
/// axis via `address_x`/`address_y`) set `AddressX`/`AddressY`; `filter` is
/// `"linear"` (default) or `"nearest"`; `lod_group` names a `TextureGroup`
/// entry (`"ui"`, `"effects"`, …) resolved through the engine's own enum,
/// `None` leaves the engine default; `flip` mirrors the source art
/// vertically at load; `tiling` clears the engine's `bNoTiling` hint for
/// wrap-style art. Images larger than 2048px in either dimension are
/// downscaled (aspect-preserving) with a log note.
#[allow(clippy::too_many_arguments)] // mirrors `load_image`'s keyword surface
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (path, name, outer=None, mips=true, srgb=true, flip=false, tiling=false, address_x=None, address_y=None, filter=None, lod_group=None))]
pub fn load_image(
    path: &str,
    name: String,
    outer: Option<String>,
    mips: bool,
    srgb: bool,
    flip: bool,
    tiling: bool,
    address_x: Option<String>,
    address_y: Option<String>,
    filter: Option<String>,
    lod_group: Option<String>,
) -> PyResult<String> {
    textures::load_image(
        path,
        &make_opts(
             name, outer, mips, srgb, flip, tiling, address_x, address_y, filter, lod_group,
        )?,
    )
    .map(|t| t.path)
    .map_err(texture_error)
}

/// Like [`load_image`] for in-memory image bytes (PNG or JPEG).
#[allow(clippy::too_many_arguments)] // mirrors `load_image`'s keyword surface
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (data, name, outer=None, mips=true, srgb=true, flip=false, tiling=false, address_x=None, address_y=None, filter=None, lod_group=None))]
pub fn load_image_bytes(
    data: Vec<u8>,
    name: String,
    outer: Option<String>,
    mips: bool,
    srgb: bool,
    flip: bool,
    tiling: bool,
    address_x: Option<String>,
    address_y: Option<String>,
    filter: Option<String>,
    lod_group: Option<String>,
) -> PyResult<String> {
    textures::load_image_bytes(
        &data,
        &make_opts(
            name, outer, mips, srgb, flip, tiling, address_x, address_y, filter, lod_group,
        )?,
    )
    .map(|t| t.path)
    .map_err(texture_error)
}

/// Replaces an existing texture's content: decodes `path` (PNG or JPEG) and
/// swaps it into the `Texture2D` at `target` (full object path).
/// Always uploads BGRA (the target's format property may be changed).
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (path, target, mips=true))]
pub fn load_image_into(path: &str, target: &str, mips: bool) -> PyResult<String> {
    textures::load_image_into(path, target, mips)
        .map(|t| t.path)
        .map_err(texture_error)
}

/// Like [`load_image_into`] for in-memory image bytes.
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (data, target, mips=true))]
pub fn load_image_bytes_into(data: Vec<u8>, target: &str, mips: bool) -> PyResult<String> {
    textures::load_image_bytes_into(&data, target, mips)
        .map(|t| t.path)
        .map_err(texture_error)
}

/// Releases a texture created by [`load_image`] to the garbage collector
/// (clears its `Standalone` flag). Returns whether the path was found
#[gen_stub_pyfunction]
#[pyfunction]
pub fn unload_texture(path: &str) -> PyResult<bool> {
    textures::unload(path).map_err(texture_error)
}
