use crate::prelude::XrSystems;
use crate::xr_init::{xr_only, XrCleanup, XrSetup};
use crate::xr_input::{QuatConv, Vec3Conv};
use crate::{locate_views, xr_wait_frame, LEFT_XR_TEXTURE_HANDLE, RIGHT_XR_TEXTURE_HANDLE};
use bevy::core_pipeline::core_3d::graph::Core3d;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::ecs::system::lifetimeless::Read;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::camera::{
    CameraMainTextureUsages, CameraProjection, CameraProjectionPlugin, CameraRenderGraph,
    RenderTarget,
};
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::primitives::Frustum;
use bevy::render::view::{
    update_frusta, ColorGrading, ExtractedView, VisibilitySystems, VisibleEntities,
};
use bevy::render::{Render, RenderApp, RenderSet};
use bevy::transform::TransformSystem;
use openxr::Fovf;
use wgpu::TextureUsages;

use super::trackers::{OpenXRLeftEye, OpenXRRightEye, OpenXRTracker, OpenXRTrackingRoot};

pub struct XrCameraPlugin;

impl Plugin for XrCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CameraProjectionPlugin::<XRProjection>::default());
        app.add_systems(
            PreUpdate,
            xr_camera_head_sync
                .run_if(xr_only())
                .after(xr_wait_frame)
                .after(locate_views),
        );
        // a little late latching
        app.add_systems(
            PostUpdate,
            xr_camera_head_sync
                .before(TransformSystem::TransformPropagate)
                .run_if(xr_only()),
        );
        app.add_systems(
            PostUpdate,
            update_frusta::<XRProjection>
                .after(TransformSystem::TransformPropagate)
                .before(VisibilitySystems::UpdatePerspectiveFrusta),
        );
        app.add_systems(
            PostUpdate,
            update_root_transform_components
                .after(TransformSystem::TransformPropagate)
                .xr_only(),
        );
        app.add_systems(XrSetup, setup_xr_cameras);
        app.add_systems(XrCleanup, cleanup_xr_cameras);
        app.add_plugins(ExtractComponentPlugin::<XrCamera>::default());
        app.add_plugins(ExtractComponentPlugin::<XRProjection>::default());
        app.add_plugins(ExtractComponentPlugin::<RootTransform>::default());
        // app.add_plugins(ExtractComponentPlugin::<TransformExtract>::default());
        // app.add_plugins(ExtractComponentPlugin::<GlobalTransformExtract>::default());
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(
            Render,
            (locate_views, xr_camera_head_sync_render_world)
                .chain()
                .run_if(xr_only())
                .in_set(RenderSet::PrepareAssets),
            // .after(xr_wait_frame)
            // .after(locate_views),
        );
    }
}

// might be unnesesary since it should be parented to the root
fn cleanup_xr_cameras(mut commands: Commands, entities: Query<Entity, With<XrCamera>>) {
    for e in &entities {
        commands.entity(e).despawn_recursive();
    }
}

fn setup_xr_cameras(mut commands: Commands) {
    commands.spawn((
        XrCameraBundle::new(Eye::Right),
        OpenXRRightEye,
        OpenXRTracker,
    ));
    commands.spawn((XrCameraBundle::new(Eye::Left), OpenXRLeftEye, OpenXRTracker));
}

#[derive(Bundle)]
pub struct XrCamerasBundle {
    pub left: XrCameraBundle,
    pub right: XrCameraBundle,
}
impl XrCamerasBundle {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Default for XrCamerasBundle {
    fn default() -> Self {
        Self {
            left: XrCameraBundle::new(Eye::Left),
            right: XrCameraBundle::new(Eye::Right),
        }
    }
}

#[derive(Bundle)]
pub struct XrCameraBundle {
    pub camera: Camera,
    pub camera_render_graph: CameraRenderGraph,
    pub xr_projection: XRProjection,
    pub visible_entities: VisibleEntities,
    pub frustum: Frustum,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub camera_3d: Camera3d,
    pub tonemapping: Tonemapping,
    pub dither: DebandDither,
    pub color_grading: ColorGrading,
    pub main_texture_usages: CameraMainTextureUsages,
    pub xr_camera_type: XrCamera,
    pub root_transform: RootTransform,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Component, ExtractComponent)]
pub struct XrCamera(pub Eye);

#[derive(Component, ExtractComponent, Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct RootTransform(pub GlobalTransform);

fn update_root_transform_components(
    mut component_query: Query<&mut RootTransform>,
    root_query: Query<&GlobalTransform, With<OpenXRTrackingRoot>>,
) {
    let root = match root_query.get_single() {
        Ok(v) => v,
        Err(err) => {
            warn!("No or too many XrTracking Roots: {}", err);
            return;
        }
    };
    component_query
        .par_iter_mut()
        .for_each(|mut root_transform| **root_transform = *root);
}

// #[derive(Component)]
// pub(super) struct TransformExtract;
//
// impl ExtractComponent for TransformExtract {
//     type Query = Read<Transform>;
//
//     type Filter = ();
//
//     type Out = Transform;
//
//     fn extract_component(item: bevy::ecs::query::QueryItem<'_, Self::Query>) -> Option<Self::Out> {
//         Some(*item)
//     }
// }
//
// #[derive(Component)]
// pub(super) struct GlobalTransformExtract;
//
// impl ExtractComponent for GlobalTransformExtract {
//     type Query = Read<GlobalTransform>;
//
//     type Filter = ();
//
//     type Out = GlobalTransform;
//
//     fn extract_component(item: bevy::ecs::query::QueryItem<'_, Self::Query>) -> Option<Self::Out> {
//         Some(*item)
//     }
// }

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Eye {
    Left = 0,
    Right = 1,
}

impl XrCameraBundle {
    pub fn new(eye: Eye) -> Self {
        Self {
            camera: Camera {
                order: -1,
                target: RenderTarget::TextureView(match eye {
                    Eye::Left => LEFT_XR_TEXTURE_HANDLE,
                    Eye::Right => RIGHT_XR_TEXTURE_HANDLE,
                }),
                viewport: None,
                ..default()
            },
            camera_render_graph: CameraRenderGraph::new(Core3d),
            xr_projection: Default::default(),
            visible_entities: Default::default(),
            frustum: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
            camera_3d: Default::default(),
            tonemapping: Default::default(),
            dither: DebandDither::Enabled,
            color_grading: Default::default(),
            xr_camera_type: XrCamera(eye),
            main_texture_usages: CameraMainTextureUsages(
                TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC,
            ),
            root_transform: default(),
        }
    }
}

#[derive(Debug, Clone, Component, Reflect, ExtractComponent)]
#[reflect(Component, Default)]
pub struct XRProjection {
    pub near: f32,
    pub far: f32,
    #[reflect(ignore)]
    pub fov: Fovf,
}

impl Default for XRProjection {
    fn default() -> Self {
        Self {
            near: 0.1,
            far: 1000.,
            fov: Default::default(),
        }
    }
}

impl XRProjection {
    pub fn new(near: f32, far: f32, fov: Fovf) -> Self {
        XRProjection { near, far, fov }
    }
}

impl CameraProjection for XRProjection {
    // =============================================================================
    // math code adapted from
    // https://github.com/KhronosGroup/OpenXR-SDK-Source/blob/master/src/common/xr_linear.h
    // Copyright (c) 2017 The Khronos Group Inc.
    // Copyright (c) 2016 Oculus VR, LLC.
    // SPDX-License-Identifier: Apache-2.0
    // =============================================================================
    fn get_projection_matrix(&self) -> Mat4 {
        //  symmetric perspective for debugging
        // let x_fov = (self.fov.angle_left.abs() + self.fov.angle_right.abs());
        // let y_fov = (self.fov.angle_up.abs() + self.fov.angle_down.abs());
        // return Mat4::perspective_infinite_reverse_rh(y_fov, x_fov / y_fov, self.near);

        let fov = self.fov;
        let is_vulkan_api = false; // FIXME wgpu probably abstracts this
        let near_z = self.near;
        let far_z = -1.; //   use infinite proj
                         // let far_z = self.far;

        let tan_angle_left = fov.angle_left.tan();
        let tan_angle_right = fov.angle_right.tan();

        let tan_angle_down = fov.angle_down.tan();
        let tan_angle_up = fov.angle_up.tan();

        let tan_angle_width = tan_angle_right - tan_angle_left;

        // Set to tanAngleDown - tanAngleUp for a clip space with positive Y
        // down (Vulkan). Set to tanAngleUp - tanAngleDown for a clip space with
        // positive Y up (OpenGL / D3D / Metal).
        // const float tanAngleHeight =
        //     graphicsApi == GRAPHICS_VULKAN ? (tanAngleDown - tanAngleUp) : (tanAngleUp - tanAngleDown);
        let tan_angle_height = if is_vulkan_api {
            tan_angle_down - tan_angle_up
        } else {
            tan_angle_up - tan_angle_down
        };

        // Set to nearZ for a [-1,1] Z clip space (OpenGL / OpenGL ES).
        // Set to zero for a [0,1] Z clip space (Vulkan / D3D / Metal).
        // const float offsetZ =
        //     (graphicsApi == GRAPHICS_OPENGL || graphicsApi == GRAPHICS_OPENGL_ES) ? nearZ : 0;
        // FIXME handle enum of graphics apis
        let offset_z = 0.;

        let mut cols: [f32; 16] = [0.0; 16];

        if far_z <= near_z {
            // place the far plane at infinity
            cols[0] = 2. / tan_angle_width;
            cols[4] = 0.;
            cols[8] = (tan_angle_right + tan_angle_left) / tan_angle_width;
            cols[12] = 0.;

            cols[1] = 0.;
            cols[5] = 2. / tan_angle_height;
            cols[9] = (tan_angle_up + tan_angle_down) / tan_angle_height;
            cols[13] = 0.;

            cols[2] = 0.;
            cols[6] = 0.;
            cols[10] = -1.;
            cols[14] = -(near_z + offset_z);

            cols[3] = 0.;
            cols[7] = 0.;
            cols[11] = -1.;
            cols[15] = 0.;

            //  bevy uses the _reverse_ infinite projection
            //  https://dev.theomader.com/depth-precision/
            let z_reversal = Mat4::from_cols_array_2d(&[
                [1f32, 0., 0., 0.],
                [0., 1., 0., 0.],
                [0., 0., -1., 0.],
                [0., 0., 1., 1.],
            ]);

            return z_reversal * Mat4::from_cols_array(&cols);
        } else {
            // normal projection
            cols[0] = 2. / tan_angle_width;
            cols[4] = 0.;
            cols[8] = (tan_angle_right + tan_angle_left) / tan_angle_width;
            cols[12] = 0.;

            cols[1] = 0.;
            cols[5] = 2. / tan_angle_height;
            cols[9] = (tan_angle_up + tan_angle_down) / tan_angle_height;
            cols[13] = 0.;

            cols[2] = 0.;
            cols[6] = 0.;
            cols[10] = -(far_z + offset_z) / (far_z - near_z);
            cols[14] = -(far_z * (near_z + offset_z)) / (far_z - near_z);

            cols[3] = 0.;
            cols[7] = 0.;
            cols[11] = -1.;
            cols[15] = 0.;
        }

        Mat4::from_cols_array(&cols)
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.far
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        let tan_angle_left = self.fov.angle_left.tan();
        let tan_angle_right = self.fov.angle_right.tan();

        let tan_angle_bottom = self.fov.angle_down.tan();
        let tan_angle_top = self.fov.angle_up.tan();

        // NOTE: These vertices are in the specific order required by [`calculate_cascade`].
        [
            Vec3A::new(tan_angle_right, tan_angle_bottom, 1.0) * z_near, // bottom right
            Vec3A::new(tan_angle_right, tan_angle_top, 1.0) * z_near,    // top right
            Vec3A::new(tan_angle_left, tan_angle_top, 1.0) * z_near,     // top left
            Vec3A::new(tan_angle_left, tan_angle_bottom, 1.0) * z_near,  // bottom left
            Vec3A::new(tan_angle_right, tan_angle_bottom, 1.0) * z_far,  // bottom right
            Vec3A::new(tan_angle_right, tan_angle_top, 1.0) * z_far,     // top right
            Vec3A::new(tan_angle_left, tan_angle_top, 1.0) * z_far,      // top left
            Vec3A::new(tan_angle_left, tan_angle_bottom, 1.0) * z_far,   // bottom left
        ]
    }
}

pub fn xr_camera_head_sync(
    views: Res<crate::resources::XrViews>,
    mut query: Query<(&mut Transform, &XrCamera, &mut XRProjection)>,
) {
    //TODO calculate HMD position
    for (mut transform, camera_type, mut xr_projection) in query.iter_mut() {
        let view_idx = camera_type.0 as usize;
        let view = match views.get(view_idx) {
            Some(views) => views,
            None => continue,
        };
        xr_projection.fov = view.fov;
        transform.rotation = view.pose.orientation.to_quat();
        transform.translation = view.pose.position.to_vec3();
    }
}

pub fn xr_camera_head_sync_render_world(
    views: Res<crate::resources::XrViews>,
    mut query: Query<(&mut ExtractedView, &XrCamera, &RootTransform)>,
) {
    for (mut extracted_view, camera_type, root) in query.iter_mut() {
        let view_idx = camera_type.0 as usize;
        let view = match views.get(view_idx) {
            Some(views) => views,
            None => continue,
        };
        let mut transform = Transform::IDENTITY;
        transform.rotation = view.pose.orientation.to_quat();
        transform.translation = view.pose.position.to_vec3();
        extracted_view.transform = root.mul_transform(transform);
    }
}
