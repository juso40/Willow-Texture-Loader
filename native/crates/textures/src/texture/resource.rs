//! Pushing a written texture to the rendering resource.

use unrealsdk_rs::objects::{
    Obj, post_edit_change_property, post_edit_change_property_index,
};

use crate::error::TextureError;
use crate::ue::props::prop;

/// Push the texture's current properties + mips to the rendering resource
/// (the native `Texture.Resource` object). Without this step the texture
/// samples as the default white/grey in every material.
///
/// `UObject::PostEditChangeProperty` is a vtable virtual, dispatched the
/// way the SDK dispatches it (`uobject.cpp`, slot 19 on WILLOW,
/// see `unrealsdk_rs::objects::post_edit_change_property`).
/// `UTexture::PostEditChangeProperty` ends in `UpdateResource()`, so the
/// engine releases any old resource and builds a fresh
/// `FTexture2DResource` from the `Mips` we just wrote.
pub(crate) fn call_update_resource(tex: Obj) -> Result<(), TextureError> {
    let Some(prop) = tex
        .find_prop(prop::MIPS)
        .or_else(|_| tex.find_prop(prop::FORMAT))
        .ok()
    else {
        return Err(TextureError::Engine(
            "cannot rebuild the render resource: no property to carry a PostEditChangeProperty"
                .to_owned(),
        ));
    };
    if !post_edit_change_property(tex.raw(), prop) {
        return Err(TextureError::Engine(format!(
            "UObject::PostEditChangeProperty dispatch failed (vtable slot {}); the texture \
             exists but was never pushed to the renderer",
            post_edit_change_property_index(),
        )));
    }
    Ok(())
}
