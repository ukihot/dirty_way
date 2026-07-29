use avian3d::dynamics::rigid_body::forces::ForcesItem;
use avian3d::prelude::*;
use bevy::prelude::*;

/// 継続的な力（毎フレーム呼び、Avian3Dが物理stepのdtで積分する）を与える。
/// grab/dragのようなバネ追従に向く。Avian3Dの`Forces` QueryData
/// （`Query<Forces>`）をそのまま薄くラップしただけ。
pub fn apply_force(forces: &mut ForcesItem<'_, '_>, force: Vec3) {
    forces.apply_force(force);
}

/// 瞬間的な力積（1フレームだけ・dtに依存しない一撃）を与える。爆発や
/// 弾丸のヒットのような「一発だけ効果が欲しい」ケースに向く。
pub fn apply_impulse(forces: &mut ForcesItem<'_, '_>, impulse: Vec3) {
    forces.apply_linear_impulse(impulse);
}

/// 指定したワールド座標に力積を与える版（重心からずれるため回転も生じる）。
pub fn apply_impulse_at_point(forces: &mut ForcesItem<'_, '_>, impulse: Vec3, world_point: Vec3) {
    forces.apply_linear_impulse_at_point(impulse, world_point);
}
