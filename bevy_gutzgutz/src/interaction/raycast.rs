use avian3d::prelude::*;
use bevy::prelude::*;

/// カーソル位置から出るワールド空間のレイ。Avian3Dには依存しない純粋な
/// Bevyのヘルパーで、grab/dragなどinteraction内の他機能からも使う。
pub fn cursor_ray(camera: &Camera, camera_transform: &GlobalTransform, window: &Window) -> Option<Ray3d> {
    let cursor_pos = window.cursor_position()?;
    camera.viewport_to_world(camera_transform, cursor_pos).ok()
}

/// プライマリウィンドウのカーソル位置から、Avian3Dの最初のレイヒットを返す
/// 薄いヘルパー。Avian3D自体のAPI（`SpatialQuery`/`RayHitData`）はそのまま
/// 呼び出し側に渡し、隠蔽しない。
pub fn cast_ray_from_cursor(
    spatial_query: &SpatialQuery,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    window: &Window,
    max_distance: f32,
    filter: &SpatialQueryFilter,
) -> Option<RayHitData> {
    let ray = cursor_ray(camera, camera_transform, window)?;
    spatial_query.cast_ray(ray.origin, ray.direction, max_distance, true, filter)
}
