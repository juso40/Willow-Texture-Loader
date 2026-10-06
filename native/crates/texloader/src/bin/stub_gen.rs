//! Generates `texloader/_texloader.pyi` from the pyo3-stub-gen inventory.
//!
//! Gets invoked by `scripts/build_windows.sh` automatically.
//!
//! The stub describes the `texloader._texloader` module. It renders the
//! inventory gathered by [`_texloader::stub_info`] directly to the explicit
//! output path (no `generate()` + maturin config round-trip).

use _texloader::stub_info;

/// Dotted module name the stub is generated for.
const MODULE: &str = "texloader._texloader";

fn main() {
    let stub = stub_info().expect("failed to gather stub info");
    let module = stub
        .modules
        .get(MODULE)
        .unwrap_or_else(|| panic!("{MODULE} missing from stub info"));
    let text = module.to_string();
    // `<root>/native/crates/texloader` -> `<root>/texloader/_texloader.pyi`.
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../texloader/_texloader.pyi");
    std::fs::write(&out, &text).expect("failed to write _texloader.pyi");
    println!("wrote {} ({} bytes)", out.display(), text.len());
}
