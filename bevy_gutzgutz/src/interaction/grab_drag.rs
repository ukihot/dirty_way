use avian3d::prelude::*;
use bevy::prelude::*;

use super::impulse::apply_force;
use super::raycast::cursor_ray;

/// grab/dragの調整可能パラメータ。ゲーム側が`insert_resource`で上書きできる。
#[derive(Resource, Clone, Copy)]
pub struct GutzGrabSettings {
    pub mouse_button: MouseButton,
    /// バネの強さ。値を離すと勝手に相手が戻ってくる力の大きさ。
    pub spring_strength: f32,
    /// 速度に対する減衰。大きいほど振動せず素直に追従する。
    pub damping: f32,
    pub max_grab_distance: f32,
}

impl Default for GutzGrabSettings {
    fn default() -> Self {
        Self {
            mouse_button: MouseButton::Left,
            spring_strength: 40.0,
            damping: 8.0,
            max_grab_distance: 50.0,
        }
    }
}

/// 現在ドラッグ中のエンティティと、掴んだ瞬間のカメラからの距離。
/// `None`なら未ドラッグ。
#[derive(Resource, Default)]
pub struct GutzGrabState {
    grabbed: Option<(Entity, f32)>,
}

impl GutzGrabState {
    pub fn grabbed_entity(&self) -> Option<Entity> {
        self.grabbed.map(|(entity, _)| entity)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<GutzGrabSettings>()
        .init_resource::<GutzGrabState>()
        .add_systems(Update, (grab_input, grab_apply).chain());
}

fn grab_input(
    mouse: Res<ButtonInput<MouseButton>>,
    settings: Res<GutzGrabSettings>,
    mut state: ResMut<GutzGrabState>,
    spatial_query: SpatialQuery,
    camera: Single<(&Camera, &GlobalTransform)>,
    window: Single<&Window>,
    rigid_bodies: Query<&RigidBody>,
) {
    if mouse.just_released(settings.mouse_button) {
        state.grabbed = None;
    }
    if !mouse.just_pressed(settings.mouse_button) {
        return;
    }

    // grab_apply（このすぐ下）と同じ、ガード節を並べて早期returnする形に
    // 揃える——「掴めたら`state.grabbed`を更新する」以外は全部脱出条件。
    let (camera, camera_transform) = *camera;
    let Some(ray) = cursor_ray(camera, camera_transform, &window) else { return };
    let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        settings.max_grab_distance,
        true,
        &SpatialQueryFilter::default(),
    ) else {
        return;
    };
    if rigid_bodies.get(hit.entity).is_ok_and(RigidBody::is_dynamic) {
        state.grabbed = Some((hit.entity, hit.distance));
    }
}

fn grab_apply(
    state: Res<GutzGrabState>,
    settings: Res<GutzGrabSettings>,
    camera: Single<(&Camera, &GlobalTransform)>,
    window: Single<&Window>,
    mut bodies: Query<(&GlobalTransform, &LinearVelocity, Forces)>,
) {
    let Some((entity, distance)) = state.grabbed else { return };
    let (camera, camera_transform) = *camera;
    let Some(ray) = cursor_ray(camera, camera_transform, &window) else { return };
    let target = ray.get_point(distance);

    if let Ok((transform, velocity, mut forces)) = bodies.get_mut(entity) {
        let to_target = target - transform.translation();
        let spring = to_target * settings.spring_strength - velocity.0 * settings.damping;
        apply_force(&mut forces, spring);
    }
}
