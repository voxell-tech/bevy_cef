//! `bevy_ui` webview display path: renders a webview into a `MaterialNode<WebviewUiMaterial>`.

use crate::prelude::{WebviewDpr, WebviewSize, WebviewSource};
#[cfg(not(target_os = "macos"))]
use crate::prelude::{WebviewFrame, WebviewSurface, new_webview_image};
use crate::webview::ui::input::WebviewUiInputPlugin;
#[cfg(not(target_os = "macos"))]
use bevy::asset::AssetEventSystems;
#[cfg(not(target_os = "macos"))]
use bevy::asset::RenderAssetUsages;
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::ui::UiSystems;
#[cfg(not(target_os = "macos"))]
use bevy_cef_core::prelude::{RenderPaintElementType, RenderTextureMessage};

mod input;
mod material;
#[cfg(not(target_os = "macos"))]
mod upload;

pub use material::WebviewUiMaterial;

/// Adds the `bevy_ui` webview display path. Sibling to `MeshWebviewPlugin`.
pub(in crate::webview) struct UiWebviewPlugin;

impl Plugin for UiWebviewPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ui/webview_ui.wesl");

        if !app.is_plugin_added::<UiPickingPlugin>() {
            app.add_plugins(UiPickingPlugin);
        }

        app.add_plugins((
            UiMaterialPlugin::<WebviewUiMaterial>::default(),
            WebviewUiInputPlugin,
        ))
        .add_systems(
            PostUpdate,
            (update_webview_ui_size, update_webview_ui_scale).after(UiSystems::Layout),
        );

        // CPU `OnPaint` consumer: Linux/Windows only. On macOS the GPU path
        // (`gpu_surface::allocate_ui_webview_surfaces` + injection) handles UI
        // webview surfaces; `RenderTextureMessage` is never emitted.
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(upload::plugin).add_systems(
            PostUpdate,
            render_ui_surface
                .run_if(on_message::<RenderTextureMessage>)
                .before(AssetEventSystems),
        );
    }
}

/// Converts a node's physical-pixel `ComputedNode` size to the logical DIP size
/// `WebviewSize` expects. Returns `None` for a pre-layout / sub-pixel size so a
/// 0-area surface is never requested.
pub(crate) fn webview_size_from_computed(
    physical_size: Vec2,
    inverse_scale_factor: f32,
) -> Option<Vec2> {
    let logical = physical_size * inverse_scale_factor;
    if logical.x < 1.0 || logical.y < 1.0 {
        None
    } else {
        Some(logical)
    }
}

/// Puts each incoming CEF frame into the corresponding entity's
/// `WebviewUiMaterial` surface. Runs in `PostUpdate`.
///
/// A frame of a new size, the first included, makes a new image holding all of
/// it. After that only the rectangles CEF repainted are queued for [`upload`] to
/// write into the image's texture; the image itself is left unchanged, since a
/// changed image is cloned and uploaded whole. It keeps no pixels in the main
/// world: alpha hit-testing reads the frame through [`WebviewFrame`].
#[cfg(not(target_os = "macos"))]
fn render_ui_surface(
    mut commands: Commands,
    mut er: MessageReader<RenderTextureMessage>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WebviewUiMaterial>>,
    mut uploads: ResMut<upload::Uploads>,
    webviews: Query<(&MaterialNode<WebviewUiMaterial>, Option<&WebviewFrame>)>,
) {
    for texture in er.read() {
        let Ok((handle, kept)) = webviews.get(texture.webview) else {
            continue;
        };
        let view = texture.ty == RenderPaintElementType::View;
        if view
            && kept.is_none_or(|kept| {
                kept.width != texture.width
                    || kept.height != texture.height
                    || !std::sync::Arc::ptr_eq(&kept.frame, &texture.frame)
            })
        {
            commands.entity(texture.webview).insert(WebviewFrame {
                frame: texture.frame.clone(),
                width: texture.width,
                height: texture.height,
            });
        }
        if view
            && let Some(surface) = materials
                .get(handle.id())
                .and_then(|material| material.surface.as_ref())
                .map(Handle::id)
            && let Some(image) = images.get(surface)
            && image.texture_descriptor.size.width == texture.width
            && image.texture_descriptor.size.height == texture.height
        {
            uploads.0.push(upload::Upload {
                image: surface,
                frame: texture.frame.clone(),
                width: texture.width,
                height: texture.height,
                rects: texture.dirty.clone(),
            });
            continue;
        }
        if let Some(mut material) = materials.get_mut(handle.id()) {
            let image_handle = material
                .surface
                .get_or_insert_with(|| images.add(Image::default()))
                .clone();
            commands
                .entity(texture.webview)
                .insert(WebviewSurface(image_handle.clone()));
            if let Some(mut image) = images.get_mut(image_handle.id()) {
                *image = new_webview_image(texture, RenderAssetUsages::RENDER_WORLD);
            }
        }
    }
}

/// Writes `WebviewSize` (logical DIP) from each UI webview node's `ComputedNode`
/// (physical px). Runs in `PostUpdate` after layout; `set_if_neq` avoids a
/// needless CEF resize when the size is unchanged. Re-runs on DPR change too.
fn update_webview_ui_size(
    mut webviews: Query<
        (&ComputedNode, &mut WebviewSize),
        (
            With<WebviewSource>,
            With<MaterialNode<WebviewUiMaterial>>,
            Or<(Changed<ComputedNode>, Changed<WebviewDpr>)>,
        ),
    >,
) {
    for (computed, mut size) in webviews.iter_mut() {
        if let Some(logical) =
            webview_size_from_computed(computed.size(), computed.inverse_scale_factor())
        {
            size.set_if_neq(WebviewSize(logical));
        }
    }
}

/// Keeps each UI webview material's [`WebviewUiMaterial::scale`] in step with
/// its node's scale factor and its `WebviewDpr`. A resize alone leaves it as it
/// is, so the material is not touched while a node is dragged to a new size.
fn update_webview_ui_scale(
    webviews: Query<
        (&ComputedNode, &WebviewDpr, &MaterialNode<WebviewUiMaterial>),
        Or<(
            Changed<ComputedNode>,
            Changed<WebviewDpr>,
            Changed<MaterialNode<WebviewUiMaterial>>,
        )>,
    >,
    mut materials: ResMut<Assets<WebviewUiMaterial>>,
) {
    for (computed, dpr, handle) in webviews.iter() {
        let Some(scale) = surface_scale(computed.inverse_scale_factor(), dpr.0) else {
            continue;
        };
        if materials
            .get(handle.id())
            .is_some_and(|material| material.scale != scale)
            && let Some(mut material) = materials.get_mut(handle.id())
        {
            material.scale = scale;
        }
    }
}

/// A node's pixels per texel of a surface CEF paints at `dpr`, and per logical
/// pixel, for a node whose `ComputedNode` has `inverse_scale_factor`.
fn surface_scale(inverse_scale_factor: f32, dpr: f32) -> Option<Vec2> {
    if inverse_scale_factor <= 0.0 || dpr <= 0.0 {
        return None;
    }
    let per_logical = inverse_scale_factor.recip();
    Some(Vec2::new(per_logical / dpr, per_logical))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_drawn_at_a_zoom_finds_its_node_pixels() {
        // A 2x window with the UI at 0.8: a logical pixel is 1.6 node pixels,
        // and a texel of a 2x surface 0.8.
        let scale = surface_scale(1.0 / 1.6, 2.0).unwrap();
        assert!((scale - Vec2::new(0.8, 1.6)).abs().max_element() < 1e-6);
        assert_eq!(surface_scale(0.0, 2.0), None);
    }

    #[test]
    fn converts_physical_to_logical() {
        // 1600x1200 physical at 2x DPI (inverse 0.5) -> 800x600 logical.
        let out = webview_size_from_computed(Vec2::new(1600.0, 1200.0), 0.5);
        assert_eq!(out, Some(Vec2::new(800.0, 600.0)));
    }

    #[test]
    fn identity_at_1x() {
        let out = webview_size_from_computed(Vec2::new(640.0, 480.0), 1.0);
        assert_eq!(out, Some(Vec2::new(640.0, 480.0)));
    }

    #[test]
    fn zero_and_subpixel_return_none() {
        assert_eq!(webview_size_from_computed(Vec2::ZERO, 1.0), None);
        assert_eq!(webview_size_from_computed(Vec2::new(0.0, 600.0), 1.0), None);
        assert_eq!(webview_size_from_computed(Vec2::new(0.5, 0.5), 1.0), None);
    }
}
