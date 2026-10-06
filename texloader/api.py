"""Thin wrappers over the native `texloader._texloader` extension.

Every engine call in the package goes through here.
"""

from __future__ import annotations

from pathlib import Path

from unrealsdk import unreal

from texloader import _texloader

__all__ = [
    "ObjectLike",
    "load",
    "load_bytes",
    "load_bytes_into",
    "load_into",
    "unload",
]

#: Names an engine object: a full object path, a `Texture2D'...'`-style
#: quoted path, or the object itself (stringified).
ObjectLike = str | unreal.UObject


def _object_path(value: ObjectLike) -> str:
    text = str(value).strip()
    if text.startswith("Texture2D'") and text.endswith("'"):
        return text[len("Texture2D'") : -1]
    return text


def _resolve_name(path: str | Path, name: str | None) -> str:
    """An explicit name wins; otherwise the file stem (sanitized natively)."""
    if name:
        return name
    return Path(path).stem


def _resolve_outer(outer: ObjectLike | None) -> str | None:
    """`None` passes through (-> Transient); objects stringify to their path."""
    if outer is None:
        return None
    if isinstance(outer, str):
        return outer or None
    return str(outer) or None


def _resolve_address(
    address: str,
    address_x: str | None,
    address_y: str | None,
) -> tuple[str, str]:
    """`address` sets both axes; `address_x`/`address_y` override one axis."""
    return (
        address_x if address_x is not None else address,
        address_y if address_y is not None else address,
    )


def load(  # noqa: PLR0913
    path: str | Path,
    name: str | None = None,
    outer: ObjectLike | None = None,
    *,
    mips: bool = True,
    srgb: bool = True,
    address: str = "clamp",
    address_x: str | None = None,
    address_y: str | None = None,
    flip: bool = False,
    tiling: bool = False,
    filter: str = "linear",  # noqa: A002
    lod_group: str | None = None,
) -> str:
    """Load an image file (PNG or JPEG) as a new `Texture2D`.

    Returns the full object path, resolve it with
    `unrealsdk.find_object("Texture2D", path)`.

    - `name`: the object name (`"logo"`).
    - `outer`: object (or path) to use as the outer object, defaults to `Transient`
      (-> `Transient.<name>`).
    - `mips`: generate the full mip chain (leave on unless you know the
      consumer only samples mip 0).
    - `srgb`: interpret the image as sRGB (used for albedo/UI art).
    - `address`: edge addressing - `"wrap"`, `"clamp"` (default) or
      `"mirror"`. `address_x`/`address_y` override one axis (the engine
      keeps `AddressX`/`AddressY` separate).
    - `flip`: mirror the source art vertically at load (bottom-up art).
    - `tiling`: the art tiles (inverse of `bNoTiling` in UE3).
    - `filter`: sampler filtering, `"linear"` (default) or `"nearest"`.
    - `lod_group`: a `TextureGroup` entry (`"ui"`, `"world"`, `"effects"`,
      ...) resolved through the engine's own enum; `None` (default) leaves
      the engine's.

    Sources larger than 2048px in either dimension are downscaled
    (aspect-preserving) with a log note.
    Textures are GC immune, call `unload` to release one.
    """
    axis_x, axis_y = _resolve_address(address, address_x, address_y)
    return _texloader.load_image(
        str(path),
        _resolve_name(path, name),
        _resolve_outer(outer),
        mips,
        srgb,
        flip,
        tiling,
        axis_x,
        axis_y,
        filter,
        lod_group,
    )


def load_bytes(  # noqa: PLR0913
    data: bytes,
    name: str,
    outer: ObjectLike | None = None,
    *,
    mips: bool = True,
    srgb: bool = True,
    address: str = "clamp",
    address_x: str | None = None,
    address_y: str | None = None,
    flip: bool = False,
    tiling: bool = False,
    filter: str = "linear",  # noqa: A002
    lod_group: str | None = None,
) -> str:
    """Like `load` but for in-memory image bytes (PNG or JPEG)."""
    axis_x, axis_y = _resolve_address(address, address_x, address_y)
    return _texloader.load_image_bytes(
        data,
        name,
        _resolve_outer(outer),
        mips,
        srgb,
        flip,
        tiling,
        axis_x,
        axis_y,
        filter,
        lod_group,
    )


def load_into(path: str | Path, target: ObjectLike, *, mips: bool = True) -> str:
    """Replace an existing texture's content with an image file.

    `target` is the games texture itself or its full object path.
    Materials referencing the texture may need rebinding.
    """
    return _texloader.load_image_into(str(path), _object_path(target), mips)


def load_bytes_into(data: bytes, target: ObjectLike, *, mips: bool = True) -> str:
    """Like `load_into` for in-memory image bytes."""
    return _texloader.load_image_bytes_into(data, _object_path(target), mips)


def unload(path: ObjectLike) -> bool:
    """Release a texture created by `load`/`load_bytes` to the garbage
    collector. `False` when the path is gone already."""
    return _texloader.unload_texture(_object_path(path))
