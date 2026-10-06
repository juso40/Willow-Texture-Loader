from mods_base import Library, build_mod

from texloader.api import (
    load,
    load_bytes,
    load_bytes_into,
    load_into,
    unload,
)
from texloader.auto_create import create_texture_for_manifest, load_manifests

__all__ = [
    "load",
    "load_bytes",
    "load_bytes_into",
    "load_into",
    "unload",
]


def _enable() -> None:
    for manifest in load_manifests():
        create_texture_for_manifest(manifest)


_enable()


build_mod(
    cls=Library,
)
