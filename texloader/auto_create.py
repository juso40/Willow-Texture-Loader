"""Manifest-driven texture creation: `create_textures/*.toml` -> `Texture2D`.

Manifest schema: only `image` and `name` are required, everything else falls back to
the defaults shown here::

    image = "logo.png"          # required, relative to the manifest
    name = "MyMod.logo"         # required; bare name -> Transient.<name>

    [texture]                   # optional; every key defaults
    mips = true
    flip = false
    srgb = true
    filter = "linear"           # "linear" | "nearest"
    address = "clamp"           # "wrap" | "clamp" | "mirror"
    address_x = "clamp"         # per-axis overrides for `address`
    address_y = "clamp"
    tiling = false
    lod_group = "ui"            # any Engine.Texture:TextureGroup ("ui", "world", "effects", ...)

Unknown keys, wrong types or bad values are an error that skips the manifest.
"""

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from unrealsdk import logging

from texloader.api import load

CREATE_TEXTURES = Path(__file__).parent / "create_textures"

_ADDRESSES = ("wrap", "clamp", "mirror")
_FILTERS = ("linear", "nearest")
_BOOL_KEYS = ("mips", "flip", "srgb", "tiling")
_TOP_LEVEL_KEYS = frozenset({"image", "name", "outer", "texture"})
_TEXTURE_KEYS = frozenset(
    {"mips", "flip", "srgb", "filter", "address", "address_x", "address_y", "tiling", "lod_group"},
)


@dataclass(frozen=True)
class TextureConfig:
    """The `[texture]`s optional configurations."""

    mips: bool = True
    flip: bool = False
    srgb: bool = True
    filter: str = "linear"
    address: str = "clamp"
    address_x: str | None = None
    address_y: str | None = None
    tiling: bool = False
    lod_group: str | None = None


@dataclass(frozen=True)
class TextureManifest:
    """One parsed `create_textures/*.toml`."""

    image: Path
    name: str
    outer: str | None
    texture: TextureConfig
    manifest_path: Path


def ensure_create_textures_path() -> None:
    CREATE_TEXTURES.mkdir(exist_ok=True)


def load_manifests() -> list[TextureManifest]:
    """Parse every manifest in `create_textures/` (sorted, errors logged)."""
    ensure_create_textures_path()
    manifests: list[TextureManifest] = []
    for file in sorted(CREATE_TEXTURES.glob("**/*.toml"), key=str):
        if manifest := load_manifest(file):
            manifests.append(manifest)
    logging.misc(f"texloader: found {len(manifests)} manifest(s)")
    return manifests


def load_manifest(file: Path) -> TextureManifest | None:
    """Parse one manifest. Returns `None` on any issues."""
    logging.misc(f"texloader: reading manifest {file.name}")
    try:
        with open(file, "rb") as f:
            data = tomllib.load(f)
    except OSError as exc:
        logging.error(f"texloader: {file.name}: cannot read manifest: {exc}")
        return None
    except tomllib.TOMLDecodeError as exc:
        logging.error(f"texloader: {file.name}: invalid TOML: {exc}")
        return None

    problems: list[str] = []
    manifest = _parse_manifest(file, data, problems)
    log_level = logging.error if manifest is None else logging.warning
    for problem in problems:
        log_level(f"texloader: {file.name}: {problem}")
    return manifest if not problems else None


def create_texture_for_manifest(manifest: TextureManifest) -> str:
    """Create the manifest's texture and return its object path.

    Raises on any load failure (missing image, decode or engine error);
    callers log and move on to the next manifest.
    """
    cfg = manifest.texture

    logging.misc(f"texloader: creating {manifest.name!r} from {manifest.image.name}")
    path = load(
        manifest.image,
        name=manifest.name,
        outer=manifest.outer,
        mips=cfg.mips,
        srgb=cfg.srgb,
        filter=cfg.filter,
        lod_group=cfg.lod_group,
        address=cfg.address,
        address_x=cfg.address_x,
        address_y=cfg.address_y,
        flip=cfg.flip,
        tiling=cfg.tiling,
    )
    logging.info(f"texloader: created {path} for {manifest.manifest_path}")
    return path


def _parse_manifest(file: Path, data: dict[str, Any], problems: list[str]) -> TextureManifest | None:
    for key in sorted(data.keys() - _TOP_LEVEL_KEYS):
        problems.append(_unknown_key(key, "image, name, [texture]"))

    image = _parse_image(file, data, problems)
    name = _parse_name(data, problems)
    outer = _parse_outer(data, problems)
    texture = _parse_texture(data.get("texture"), problems)
    if image is None or name is None:
        return None
    return TextureManifest(image=image, name=name, outer=outer, texture=texture, manifest_path=file)


def _parse_image(file: Path, data: dict[str, Any], problems: list[str]) -> Path | None:
    """The required `image` key: a non-empty relative (or absolute) path."""
    raw = data.get("image")
    if raw is None:
        problems.append('missing required key "image"')
        return None
    if not isinstance(raw, str) or not raw.strip():
        problems.append(f'"image" must be a non-empty string, got {raw!r}')
        return None
    image = Path(raw)
    if not image.is_absolute():
        image = file.parent / image
    image = image.resolve()
    if not image.is_file():
        problems.append(f"image file not found: {image} (resolved to {image.absolute()})")
        return None
    return image


def _parse_name(data: dict[str, Any], problems: list[str]) -> str | None:
    """The required `name` key."""
    raw = data.get("name")
    if raw is None:
        problems.append('missing required key "name"')
        return None
    if not isinstance(raw, str) or not raw.strip():
        problems.append(f'"name" must be a non-empty string, got {raw!r}')
        return None
    name = raw.strip()
    if any(not part for part in name.split(".")):
        problems.append(f'"name" {name!r} has an empty part')
        return None
    return name


def _parse_outer(data: dict[str, Any], problems: list[str]) -> str | None:
    """The optional `outer` key."""
    raw = data.get("outer")
    if raw is None:
        return None
    if not isinstance(raw, str) or not raw.strip():
        problems.append(f'"outer" must be a non-empty string, got {raw!r}')
        return None
    return raw.strip()


def _parse_texture(raw: dict | None, problems: list[str]) -> TextureConfig:
    """The optional `[texture]` table with fallback to defaults."""
    if raw is None:
        logging.misc("texloader: no [texture] table; using all defaults")
        return TextureConfig()

    for key in sorted(raw.keys() - _TEXTURE_KEYS):
        problems.append(_unknown_key(key, f"[texture] keys: {', '.join(sorted(_TEXTURE_KEYS))}"))

    values = _texture_values(raw, problems)
    if "address" in values and ("address_x" in values or "address_y" in values):
        logging.warning('texloader: both "address" and a per-axis override are set, the per-axis value wins')
    return TextureConfig(**values)


def _texture_values(raw: dict[str, Any], problems: list[str]) -> dict[str, Any]:
    """Typed values from a `[texture]` table."""
    values: dict[str, Any] = {}
    for key in _BOOL_KEYS:
        if key in raw:
            if isinstance(raw[key], bool):
                values[key] = raw[key]
            else:
                problems.append(f'[texture] "{key}" must be a boolean, got {raw[key]!r}')
    for key, choices in (
        ("filter", _FILTERS),
        ("address", _ADDRESSES),
        ("address_x", _ADDRESSES),
        ("address_y", _ADDRESSES),
    ):
        if key in raw and (value := _parse_choice(raw[key], key, choices, problems)) is not None:
            values[key] = value
    lod_group = raw.get("lod_group")
    if lod_group is not None:
        if isinstance(lod_group, str) and lod_group.strip():
            values["lod_group"] = lod_group.strip()
        else:
            problems.append(f'[texture] "lod_group" must be a non-empty string, got {lod_group!r}')
    return values


def _parse_choice(raw: object, key: str, choices: tuple[str, ...], problems: list[str]) -> str | None:
    """A string value from a fixed choice set (case-insensitive)."""
    if isinstance(raw, str) and raw.strip().lower() in choices:
        return raw.strip().lower()
    problems.append(f'[texture] "{key}" must be one of {", ".join(choices)}; got {raw!r}')
    return None


def _unknown_key(key: str, valid: str) -> str:
    return f"unknown key {key!r} (valid: {valid})"
