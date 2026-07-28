use avian3d::prelude::*;
use bevy::prelude::*;

use super::impulse::apply_impulse;

/// `center`から`radius`以内にある全dynamic RigidBodyへ、中心からの距離に
/// 応じて線形減衰する放射状の力積を与える（doc/gutzgutz-requirements.md
/// 6節）。ゲーム側が対象を絞ったQuery（例：`Query<(&GlobalTransform, Forces),
/// With<RigidBody>>`）を渡すことで、対象範囲はゲーム側が制御する。
pub fn explode(bodies: &mut Query<(&GlobalTransform, Forces)>, center: Vec3, radius: f32, strength: f32) {
    for (transform, mut forces) in bodies.iter_mut() {
        let offset = transform.translation() - center;
        let distance = offset.length();
        if distance > radius || distance < f32::EPSILON {
            continue;
        }
        let falloff = 1.0 - (distance / radius);
        apply_impulse(&mut forces, offset.normalize() * strength * falloff);
    }
}
