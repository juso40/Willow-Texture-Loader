//! `FUntypedBulkData` / `FTexture2DMipMap` interop: the cooked mip block
//! layout.
//!
//! - [`MipBlock`] — the live `FTexture2DMipMap` block (its
//!   `FUntypedBulkData` head plus `SizeX`/`SizeY`), field by field.
//!
//! ## Layout
//!
//! `FTexture2DMipMap` and its `FByteBulkData` head are not `UObject`s, so
//! the SDK offset table does not describe them. The game itself publishes
//! their layout as script mirrors: `UntypedBulkData_Mirror` in
//! `Core/Classes/Object.uc` and `struct native Texture2DMipMap` in
//! `Engine/Classes/Texture2D.uc`. On BL2 the script `Mips` field is a
//! `StructProperty` whose 12-byte payload **is** the
//! `TArray<FTexture2DMipMap*>` header, so it is an *indirect* array: elements are
//! 4-byte pointers to individually allocated mip blocks. The block layout
//! ([`MipBlock`]) follows those mirrors.

use std::ffi::c_void;

use unrealsdk_rs::mem::{Offset, read_ptr, write_i32, write_ptr, write_u32};

/*
FTexture2DMipMap block layout on BL2:
`UTexture2D::Init` allocates mip blocks with `new(0x3c)`
(`FByteBulkData Data` at +0x00 = the `UntypedBulkData_Mirror` of
`Core/Classes/Object.uc`, then `SizeX`, `SizeY`) and fills the payload with
`Lock(LOCK_READ_WRITE); Realloc(bytes); memcpy; Unlock();`.
The values [`MipBlock::init`] writes reproduce exactly the state
that sequence leaves behind (the `FUntypedBulkData` ctor + Realloc;
Realloc only ever touches ElementCount and BulkData).
*/

/// A live, individually allocated `FTexture2DMipMap` block. Every accessor
/// documents the field it reads.
///
/// (`LockStatus` +0x28 and `AttachedAr` +0x2c stay zero: unlocked, no
/// linker.) `BulkDataFlags` **must** stay 0: `FUntypedBulkData::Unlock`
/// frees the payload and NULLs `BulkData` whenever
/// [`BULKDATA_SINGLE_USE`](unrealsdk_rs::flags::bulkdata_flags::BULKDATA_SINGLE_USE)
/// is set or an archive is attached.
/// One wrong Lock/Unlock and the pixels are seem to be gone.
pub struct MipBlock(*mut c_void);

impl MipBlock {
    /// The block's footprint (`UTexture2D::Init` allocates `new(0x3c)`).
    pub const SIZE: usize = 0x3c;

    /// FTexture2DMipMap vftable (0x016abb54 on the BL2 binary - copied from
    /// a live block in this crate's `mip_block_vtable`).
    const OFF_VTABLE: Offset = Offset::bytes(0x00);
    /// `BulkDataFlags` = 0 (ctor) — see the white-texture note above.
    const OFF_BULK_FLAGS: Offset = Offset::bytes(0x04);
    /// `ElementCount` = payload bytes (`Realloc`), FByteBulkData elems are 1B.
    const OFF_ELEMENT_COUNT: Offset = Offset::bytes(0x08);
    /// `BulkDataOffsetInFile` = -1 (ctor), no file/TFC backing.
    const OFF_FILE_OFFSET: Offset = Offset::bytes(0x0c);
    /// `BulkDataSizeOnDisk` = -1 (ctor).
    const OFF_SIZE_ON_DISK: Offset = Offset::bytes(0x10);
    /// `SavedBulkDataFlags` = 0 (ctor).
    const OFF_SAVED_FLAGS: Offset = Offset::bytes(0x14);
    /// `SavedElementCount` = -1 (ctor).
    const OFF_SAVED_COUNT: Offset = Offset::bytes(0x18);
    /// `SavedBulkDataOffsetInFile` = -1 (ctor).
    const OFF_SAVED_OFFSET: Offset = Offset::bytes(0x1c);
    /// `SavedBulkDataSizeOnDisk` = -1 (ctor).
    const OFF_SAVED_SIZE: Offset = Offset::bytes(0x20);
    /// `BulkData` = the pixel buffer pointer (where
    /// `FUntypedBulkData::Lock()` reads the payload).
    const OFF_PAYLOAD: Offset = Offset::bytes(0x24);
    /// `bShouldFreeOnEmpty` = 1 (ctor), engine frees the payload with the
    /// block.
    const OFF_FREE_ON_EMPTY: Offset = Offset::bytes(0x30);
    /// `SizeX`.
    const OFF_SIZE_X: Offset = Offset::bytes(0x34);
    /// `SizeY`.
    const OFF_SIZE_Y: Offset = Offset::bytes(0x38);

    /// The four `Saved*` / offset-on-disk ctor sentinels (`-1`).
    const CTOR_SENTINEL: i32 = -1;
    /// `BulkDataFlags` init value, must keep
    /// [`BULKDATA_SINGLE_USE`](unrealsdk_rs::flags::bulkdata_flags::BULKDATA_SINGLE_USE)
    /// clear (compile-time guard below).
    const INIT_BULK_FLAGS: u32 = 0;
    /// `bShouldFreeOnEmpty` init value.
    const INIT_FREE_ON_EMPTY: i32 = 1;

    /// Wrap a raw block pointer.
    pub fn from_raw(ptr: *mut c_void) -> Self {
        Self(ptr)
    }

    /// The raw block pointer.
    pub fn as_ptr(&self) -> *mut c_void {
        self.0
    }

    /// The block's vftable pointer.
    ///
    /// # Safety
    /// Live mip block
    pub unsafe fn vtable(&self) -> *mut c_void {
        // SAFETY: caller contract (see the safety docs above).
        unsafe { read_ptr(self.0 as *const c_void, Self::OFF_VTABLE) }
    }

    /// Stamp the engine's own `UTexture2D::Init` mip state (bulk-data ctor +
    /// `Realloc`) and copy `bgra` into `pixels`:
    ///
    /// ```text
    /// 0x00 vftable                  (copied from a live block)
    /// 0x04 BulkDataFlags            (0 — NOT BULKDATA_ForceSingleElementPayload)
    /// 0x08 ElementCount             (payload bytes — FByteBulkData elems are 1B)
    /// 0x0c/0x10 Offset/SizeOnDisk   (-1 — no file/TFC backing)
    /// 0x14..0x20 Saved*             (0, -1, -1, -1 — ctor state)
    /// 0x24 BulkData                 (THE pixel buffer pointer)
    /// 0x28/0x2c zero                (LockStatus / AttachedAr)
    /// 0x30 bShouldFreeOnEmpty       (1 — engine frees the payload with the block)
    /// 0x34/0x38 SizeX/SizeY
    /// ```
    ///
    /// In particular `BulkDataFlags` stays 0 so
    /// `FUntypedBulkData::Unlock()` keeps the payload, and the pointer goes
    /// where `Lock()` reads it (`BulkData`).
    ///
    /// # Safety
    /// Live mip block of [`Self::SIZE`] bytes; `pixels` is engine-heap
    /// (`u_malloc`) memory of at least `bgra.len()` bytes.
    pub unsafe fn init(
        &self,
        vtable: *mut c_void,
        pixels: *mut c_void,
        bgra: &[u8],
        width: i32,
        height: i32,
    ) {
        let b = self.0;
        // SAFETY: live block of `SIZE` bytes and an engine-heap payload
        // (caller contract); field values per the docs above.
        unsafe {
            write_ptr(b, Self::OFF_VTABLE, vtable);
            write_u32(b, Self::OFF_BULK_FLAGS, Self::INIT_BULK_FLAGS);
            write_i32(b, Self::OFF_ELEMENT_COUNT, bgra.len() as i32);
            write_i32(b, Self::OFF_FILE_OFFSET, Self::CTOR_SENTINEL);
            write_i32(b, Self::OFF_SIZE_ON_DISK, Self::CTOR_SENTINEL);
            write_u32(b, Self::OFF_SAVED_FLAGS, Self::INIT_BULK_FLAGS);
            write_i32(b, Self::OFF_SAVED_COUNT, Self::CTOR_SENTINEL);
            write_i32(b, Self::OFF_SAVED_OFFSET, Self::CTOR_SENTINEL);
            write_i32(b, Self::OFF_SAVED_SIZE, Self::CTOR_SENTINEL);
            write_ptr(b, Self::OFF_PAYLOAD, pixels);
            // Payload is engine-heap (`u_malloc`), so let the engine free it
            // with the bulk data.
            write_i32(b, Self::OFF_FREE_ON_EMPTY, Self::INIT_FREE_ON_EMPTY);
            write_i32(b, Self::OFF_SIZE_X, width);
            write_i32(b, Self::OFF_SIZE_Y, height);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), pixels as *mut u8, bgra.len());
        }
    }
}

/// Compile-time guard for the white-texture bug: the bulk header's init
/// flags must never set `BULKDATA_ForceSingleElementPayload`.
const _: () = assert!(
    MipBlock::INIT_BULK_FLAGS & unrealsdk_rs::flags::bulkdata_flags::BULKDATA_SINGLE_USE == 0,
    "init flags must never set BULKDATA_ForceSingleElementPayload (white-texture bug)"
);
