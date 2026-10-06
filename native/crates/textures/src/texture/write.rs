//! The BL2-style indirect `Mips` writer: allocate mip blocks, fill them
//! ([`MipBlock::init`]), and stamp the `Mips` array header.
//!
//! ## Ownership
//!
//! Mip pixel buffers and the `Mips` array backing store are `u_malloc`'d
//! and then **handed to the engine** (the texture's `TArray`/
//! `TIndirectArray` owns them). They are intentionally never freed by Rust.
//! See [`EngineBuffer`] for the RAII variant used for buffers that may be
//! abandoned on a failure path before the hand-off.

use std::ffi::c_void;

use unrealsdk_rs::mem::{GAME_WORD, Offset, write_ptr};
use unrealsdk_rs::objects::Obj;
use unrealsdk_rs::sys::sys;
use unrealsdk_rs::types::TArray;

use crate::error::TextureError;
use crate::img::Mip;

use super::layout::{MipBlock, mip_block_vtable};

/// A `u_malloc`'d buffer handed to the engine (never freed by Rust).
struct EngineBuffer {
    ptr: *mut c_void,
}

impl EngineBuffer {
    fn alloc(len: usize) -> Result<Self, TextureError> {
        if len == 0 {
            return Err(TextureError::Implausible(
                "refusing to allocate a zero-byte engine buffer".to_owned(),
            ));
        }
        let s = sys().ok_or(TextureError::OffsetsUnavailable)?;
        // SAFETY: resolved export, zeroed by contract (unrealsdk.h).
        let ptr = unsafe { (s.u_malloc)(len) };
        if ptr.is_null() {
            Err(TextureError::Engine(format!(
                "u_malloc failed for {len} bytes"
            )))
        } else {
            Ok(Self { ptr })
        }
    }

    fn as_mut_ptr(&self) -> *mut c_void {
        self.ptr
    }

    /// Leak on purpose: the engine owns this memory from now on.
    fn release(self) -> *mut c_void {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for EngineBuffer {
    fn drop(&mut self) {
        // Only reached when a failure path abandons a buffer before handing
        // it to the engine.
        if let Some(s) = sys() {
            // SAFETY: allocated via `u_malloc`.
            unsafe { (s.u_free)(self.ptr) };
        }
    }
}

/// Build the indirect `Mips` array at `mips_at` (the pointer-array
/// [`TArray`] header): elements are pointers to individually allocated
/// [`MipBlock::SIZE`]-byte mip blocks, the engine's own `UTexture2D::Init`
/// recipe (`new(0x3c)`, bulk-data ctor, `Realloc`), documented on
/// [`MipBlock::init`].
///
/// Every allocation handed to the engine here is engine-owned from then on
/// (see the module docs): the pixel buffers live as long as the texture
/// (`bShouldFreeOnEmpty` lets the engine release them with it), the blocks
/// and the pointer array belong to the engine's `TIndirectArray`.
///
/// # Safety
/// `mips_at` must be the texture's live `Mips` array header (three game
/// words, writable).
pub(crate) unsafe fn write_mips_indirect(
    mips_at: *mut c_void,
    tex: Obj,
    chain: &[Mip],
) -> Result<(), TextureError> {
    let vtable = mip_block_vtable(tex).ok_or_else(|| {
        TextureError::NotFound(
            "No live Texture2D with mips to copy the mip-block vtable from!"
                .to_owned(),
        )
    })?;
    let count = chain.len() as i32;
    let array = EngineBuffer::alloc(chain.len() * GAME_WORD).map_err(|e| {
        TextureError::Engine(format!("failed to allocate the Mips pointer array: {e}"))
    })?;

    for (i, mip) in chain.iter().enumerate() {
        let block = EngineBuffer::alloc(MipBlock::SIZE)
            .map_err(|e| TextureError::Engine(format!("failed to allocate a mip block: {e}")))?;
        let pixels = EngineBuffer::alloc(mip.bgra.len())
            .map_err(|e| TextureError::Engine(format!("failed to allocate a mip payload: {e}")))?
            .release();
        // SAFETY: `block` is a zeroed engine allocation of `MipBlock::SIZE`
        // bytes; `pixels` is engine-heap of `mip.bgra.len()` bytes. The
        // field values reproduce the engine's own `UTexture2D::Init` mip
        // state (documented on `MipBlock::init`).
        unsafe {
            MipBlock::from_raw(block.as_mut_ptr()).init(
                vtable,
                pixels,
                &mip.bgra,
                mip.width as i32,
                mip.height as i32,
            );
            write_ptr(
                array.as_mut_ptr(),
                Offset::bytes(i * GAME_WORD),
                block.as_mut_ptr(),
            );
        }
        // Engine owns the block from here on.
        block.release();
    }

    let data = array.release();
    // SAFETY: caller guarantees `mips_at` is the texture's `Mips` header;
    // `data` holds `count` engine-owned element pointers.
    unsafe { TArray::<*mut c_void>::write(mips_at, data.cast(), count) };
    Ok(())
}
