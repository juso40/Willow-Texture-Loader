# texloader
Runtime texture loading for **Borderlands 2**: import arbitrary PNG/JPEG
images as real, game-ready `Texture2D` objects.

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
texloader will automatically create a `Texture2D` object for any PNG/JPEG image you provide it (with the respective manifest entries).   
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

But for programmatically created textures or manual texture lifetime management, you can use the `load`/`load_into`/`unload` functions provided by this mod.


## Notes
Texture resolution is limited to 2048px in either dimension. Larger textures will automatically be scaled down to fit.  
(This is an arbitrary limit by me, the game seems to support up to 8192px in either dimension, but since its only 32bit we are limited on the available memory.)    
I decided to limit it to 2048px, as this mod only supports RGBA textures, that means 4bytes per pixel, since we don't use compression. 
So a 2048px texture takes up 16MB!!! of memory (2048 * 2048 * 4 = 16,777,216 bytes).   
(If mips are enabled, the texture will even take up more memory...)
