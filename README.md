# texloader
Runtime texture loading for **Borderlands 2**: import PNG/JPEG images and
pre-encoded BC1 DDS textures as real, game-ready `Texture2D` objects.

A [pyunrealsdk](https://bl-sdk.github.io/willow2-mod-db/) mod.

## Install

Unpack `texloader.zip` into `Binaries/Win32/Plugins/sdk_mods/` (the folder
must be named `texloader`).

## Build

With Docker installed, run:

```sh
./scripts/build_windows.sh
```

This builds the 32-bit Windows native module and generates its type stub,
placing `_texloader.pyd` and `_texloader.pyi` in `texloader/`.
Both builds always use release mode; there are no parameters.

## How to Use

### For `blcm` Modders
You can use `texloader` by using its provided autocreate feature.  
texloader will automatically create a `Texture2D` object for any PNG/JPEG image or BC1 DDS texture you provide it (with the respective manifest entries).
For that simply place the images you want to use into the `texloader/create_textures` folder.

You can also add any amount of subfolders to the `create_textures` folder. These will be scanned recursively for any `.toml` files.

#### `.toml` Manifest
To describe how the game should load your texture, create a `.toml`.  
```toml

# Texloader manifest. Only "image" and "name" are required.
# Every other key falls back to the default shown below
# (drop the whole [texture] table and you get exactly those).
image = "prototype.png"
name = "prototype"
outer = "Transient"              # outer object (optional, defaults to "Transient" if not specified)

# Optional [texture] table overrides the default texture settings.
[texture]
mips = true                     # full mip chain (false = mip 0 only)
compression = "none"            # "none" (BGRA8) or "bc1" ("dxt1" alias, lossy/one-bit alpha)
flip = false                    # mirror the art vertically at load
srgb = true                     # interpret the art as sRGB (UI/albedo, set to false for normal/comp maps)
filter = "linear"               # "linear" or "nearest" (pixel art)
address = "clamp"               # "wrap", "clamp" or "mirror"
tiling = true                   # true for tileable art (pairs with wrap)

# Per-axis addressing, overrides `address` on one axis
# (the engine keeps AddressX/AddressY separate):
# address_x = "clamp"
# address_y = "clamp"

# Actually not sure how or if it is used, but it is passed to the engine.
# If unsure, remove the line simply!
lod_group = "world"              # any `Engine.Texture:TextureGroup`: "ui", "world", "effects", ...
```
The `Texture2D` created for this specific example will be created as `Texture2D'Transient.prototype'`.

### For Python Developers
You can obviously use the same method to create textures as the `blcm` modders, by using the `create_textures` folder method.

To load a manifest bundled with your own mod, use `load_from_manifest`. You do not
need to copy the manifest or its images into texloader's `create_textures` folder:

```python
from pathlib import Path

import unrealsdk
from texloader import load_from_manifest, unload

# For example: my_mod/textures/logo.toml and my_mod/textures/logo.png.
manifest_path = Path(__file__).resolve().parent / "textures" / "logo.toml"
texture_path = load_from_manifest(manifest_path)  # accepts an absolute str or Path
texture = unrealsdk.find_object("Texture2D", texture_path)

# When your mod no longer needs the texture:
# unload(texture_path)
```

The manifest uses the same schema shown above. Relative `image` paths are resolved
against the TOML file's directory, not the working directory. The function returns
the created texture's full object path, just like `load`. Unreadable or invalid
manifests raise `ValueError` with details logged.
Created textures remain GC immune until `unload` is called.

For programmatically created textures or manual texture lifetime management, you can
also use the `load`/`load_into`/`unload` functions provided by this mod.


### BC1 / DXT1 compression

To reduce resident texture memory, opt into BC1 encoding in the manifest:

```toml
image = "background.png"
name = "background"

[texture]
compression = "bc1"
```

Or use the Python API:

```python
from texloader import load

texture_path = load("/absolute/path/to/background.png", compression="bc1")
```

`load_bytes`, `load_into`, and `load_bytes_into` also accept `compression`.
`"dxt1"` is an alias for `"bc1"`. `"none"` is the default without compression.
External manifests loaded with `load_from_manifest` support the same setting.

BC1 encodes every mip as DXT1 blocks and sets the engine format to `PF_DXT1`.
Both the engine's resident bulk-data buffers and GPU upload use compressed data,
not BGRA buffers with a compression flag. Large mips use **one-eighth** the
uncompressed pixel storage: a 8192x8192 top mip is **32 MiB** instead of **256 MiB**!
Decoding and mip generation still use temporary uncompressed memory during loading.

Trade-offs:
- BC1 is lossy and can introduce colour/block artifacts, especially in UI art.
- Alpha is one-bit (transparent or opaque). Smooth transparency is lost.
  Keep `compression = "none"` for soft shadows,
  gradients, and other smoothly transparent textures.
- For D3D9 compatibility, top-mip dimensions are rounded up to multiples of four
  (minimum 4x4) by resizing the image. Tiny tail mips still store
  at least one complete eight-byte block.
- Compression adds one-time CPU work when loading. The resulting texture stays
  compressed while resident in the engine.

### Pre-encoded BC1 DDS (faster loading)

For textures you ship with a mod, encode them to **BC1/DXT1 DDS** ahead of time
using an exporter such as DirectXTex's `texconv`. Include mipmaps when exporting if you want them in-game.
For example, with DirectXTex:

```sh
texconv -f BC1_UNORM -m 0 image.png
```

Then load the resulting DDS with the same API:

```python
from texloader import load

texture_path = load("/absolute/path/to/image.dds")
```

Or point your existing TOML manifest at it:

```toml
image = "image.dds"
name = "image"

[texture]
mips = true
```

`load_from_manifest`, the automatic manifest scan, `load_bytes`, `load_into`, and
`load_bytes_into` all support BC1 DDS. 


DDS rules:
- Supports legacy `DXT1` DDS and DX10 `BC1_UNORM` / `BC1_UNORM_SRGB` DDS.
- Dimensions must already be divisible by four (minimum 4x4) and no larger than 8192x8192. 
- `mips = true` reuses **all stored mip levels**, including partial chains or a single level. Missing mipmaps are not generated. `mips = false` uploads only mip 0.
- DDS always stays BC1. The `compression` option only selects encoding for
  PNG/JPEG and does not decompress DDS, even when it is `"none"` (the default).
- `flip = true` is rejected for DDS. Flip the source before exporting instead.
- The API/manifest `srgb` option controls colour space, even for a DX10 sRGB
  DDS header. Set `srgb = false` for linear-data textures.
