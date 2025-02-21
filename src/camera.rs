use bevy::{
    color::palettes::css::RED,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};

use crate::chunk_storage::POV;
// Spherical camera shenangigans
// Needs to map the camera position to the nearest point on the sphere
// Camera transform gets set to that point
// Mouse drag should move the camerai

#[derive(Component)]
pub(crate) struct CameraTarget {
    pub(crate) radius: f32,
}

pub(crate) struct CameraPlugin;
impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                position_camera,
                mouse_drag.before(position_camera),
                mouse_scroll.before(position_camera),
            ),
        )
        .add_systems(Startup, setup_camera);
    }
}

#[derive(Component)]
pub(crate) struct GameCamera;

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 1.0),
        GameCamera,
        POV(Vec3::ZERO),
    ));
}

pub(crate) fn position_camera(
    mut camera: Query<&mut Transform, With<GameCamera>>,
    target: Query<(&Transform, &CameraTarget), Without<GameCamera>>,
    mut gizmos: Gizmos<DefaultGizmoConfigGroup>,
) {
    if camera.is_empty() || target.is_empty() {
        return;
    }

    let mut camera_transform = camera.single_mut();
    let (target_transform, CameraTarget { radius }) = target.single();

    let tt = target_transform.translation;
    let ct = camera_transform.translation;
    camera_transform.translation = (ct - tt).normalize() * (radius * 2.0);
    camera_transform.look_at(tt, Vec3::Y);

    gizmos.sphere((ct - tt).normalize() * radius, 0.2, RED);
}

pub(crate) fn mouse_drag(
    mut evr_motion: EventReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    target: Query<(&Transform, &CameraTarget), Without<GameCamera>>,
    mut camera: Query<&mut Transform, With<GameCamera>>,
) {
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    for ev in evr_motion.read() {
        let mut transform = camera.single_mut();
        let (target_transform, &CameraTarget { radius }) = target.single();

        let y_axis = Vec3::Y;
        let x_axis = y_axis
            .cross(transform.translation - target_transform.translation)
            .normalize();
        let y_axis = x_axis
            .cross(transform.translation - target_transform.translation)
            .normalize();

        let local_delta = (-ev.delta.x * x_axis - ev.delta.y * y_axis) * 0.1;

        transform.translation += local_delta;
        if transform.translation.y > radius * 1.99 || transform.translation.y < radius * -1.99 {
            transform.translation -= -ev.delta.y * y_axis * 0.1;
        }
    }
}

pub(crate) fn mouse_scroll(
    mut evr_motion: EventReader<MouseWheel>,
    mut camera: Query<&mut Projection, With<GameCamera>>,
) {
    let Projection::Perspective(projection) = camera.single_mut().into_inner() else {
        return;
    };

    for ev in evr_motion.read() {
        match ev.unit {
            bevy::input::mouse::MouseScrollUnit::Line => {
                projection.fov = 0.1f32
                    .max((projection.fov.sqrt() - ev.y * 0.1).powi(2))
                    .min(1.0 * std::f32::consts::PI);
            }
            bevy::input::mouse::MouseScrollUnit::Pixel => {
                todo!();
            }
        };
    }
}
