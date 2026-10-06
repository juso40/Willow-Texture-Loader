//! texloader native module — runtime texture loading, nothing else.
//!
//! The `texloader` mod's Python extension (`texloader/_texloader.pyd`). It
//! exposes the shared texture surface ([`py_textures`]).
//!
//! Everything below runs on the calling thread — which is the game thread,
//! where PythonSDK executes mods. No Python callbacks — a load either
//! returns the object path or raises.

use pyo3::prelude::*;

/// Gathers stub metadata for `cargo run --bin stub_gen` (writes
/// `texloader/_texloader.pyi`). Reads no config file; the module name is
/// fixed.
pub fn stub_info() -> pyo3_stub_gen::Result<pyo3_stub_gen::StubInfo> {
    pyo3_stub_gen::StubInfo::from_project_root(
        "texloader._texloader".to_owned(),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        false,
        Default::default(),
    )
}

#[pymodule]
fn _texloader(module: &Bound<'_, PyModule>) -> PyResult<()> {
    // Route the native crates' `log::` output into the SDK log (game
    // console + SDK log file) before anything can emit — the `log` facade
    // drops every record until a logger is registered.
    textures::install_log_bridge();
    module.add_function(wrap_pyfunction!(py_textures::load_image, module)?)?;
    module.add_function(wrap_pyfunction!(py_textures::load_image_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(py_textures::load_image_into, module)?)?;
    module.add_function(wrap_pyfunction!(
        py_textures::load_image_bytes_into,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(py_textures::unload_texture, module)?)?;
    Ok(())
}
