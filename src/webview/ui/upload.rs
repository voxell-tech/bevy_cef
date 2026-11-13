//! Writes the dirty rectangles of UI webviews' frames straight into their GPU
//! textures, CPU `OnPaint` path only.
//!
//! An `Image` marked changed is cloned whole into the render world and uploaded
//! whole, which for a page the size of a window is tens of megabytes a frame. So
//! once a webview's image has its size, `render_ui_surface` updates its pixels
//! without marking it changed and queues an [`Upload`] of the part CEF repainted,
//! which [`write_uploads`] copies from the frame into the texture.

use bevy::prelude::*;
use bevy::render::render_asset::{RenderAssets, prepare_assets};
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy_cef_core::prelude::SharedFrame;

/// How many frames an upload waits for its texture before it is dropped.
const WAIT_MOST: u8 = 120;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Uploads>();
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<Pending>()
            .add_systems(ExtractSchedule, take_uploads)
            .add_systems(
                Render,
                write_uploads
                    .in_set(RenderSystems::PrepareResources)
                    .after(prepare_assets::<GpuImage>),
            );
    }
}

/// A part of a webview's frame to write into its image's texture.
pub(crate) struct Upload {
    pub image: AssetId<Image>,
    pub frame: SharedFrame,
    pub width: u32,
    pub height: u32,
    pub rects: Vec<URect>,
}

/// The uploads queued in the main world since the last extraction.
#[derive(Resource, Default)]
pub(crate) struct Uploads(pub Vec<Upload>);

/// The uploads in the render world, with the frames each has waited for its
/// texture.
#[derive(Resource, Default)]
struct Pending(Vec<(Upload, u8)>);

fn take_uploads(mut main: ResMut<MainWorld>, mut pending: ResMut<Pending>) {
    let mut uploads = main.resource_mut::<Uploads>();
    pending
        .0
        .extend(uploads.0.drain(..).map(|upload| (upload, 0)));
}

/// Writes each upload whose texture exists, from the frame as it is now, which
/// is at least as new as when the upload was queued.
fn write_uploads(
    mut pending: ResMut<Pending>,
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
) {
    pending.0.retain_mut(|(upload, waited)| {
        let Some(gpu) = images.get(upload.image) else {
            *waited += 1;
            return *waited < WAIT_MOST;
        };
        let size = gpu.texture_descriptor.size;
        // A texture of another size was replaced by a resize, which uploads the
        // whole frame itself.
        if size.width != upload.width || size.height != upload.height {
            return false;
        }
        let Ok(frame) = upload.frame.lock() else {
            return false;
        };
        if frame.len() != (upload.width * upload.height * 4) as usize {
            return false;
        }
        for rect in &upload.rects {
            if rect.max.x > upload.width || rect.max.y > upload.height || rect.is_empty() {
                continue;
            }
            write_rect(&queue, gpu, &frame, upload.width, *rect);
        }
        false
    });
}

/// Writes the rectangle `rect` of `frame`, a BGRA frame `width` pixels wide,
/// into `gpu`'s texture.
fn write_rect(queue: &RenderQueue, gpu: &GpuImage, frame: &[u8], width: u32, rect: URect) {
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &gpu.texture,
            mip_level: 0,
            origin: Origin3d {
                x: rect.min.x,
                y: rect.min.y,
                z: 0,
            },
            aspect: TextureAspect::All,
        },
        frame,
        TexelCopyBufferLayout {
            offset: u64::from((rect.min.y * width + rect.min.x) * 4),
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        Extent3d {
            width: rect.width(),
            height: rect.height(),
            depth_or_array_layers: 1,
        },
    );
}
