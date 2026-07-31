//! `atlas`モジュールの2D `Sprite`向け自動再生ヘルパー（`atlas-sprite2d`
//! feature）。
//!
//! `atlas.rs`自体は描画方式（2D `Sprite` / 3DメッシュのUVオフセット）を
//! 問わない生データ（[`crate::atlas::GutzAtlasFrame`]）だけを返す設計を
//! 保っている（atlas.rsの冒頭コメント参照）。`Sprite`への直接依存はここ
//! （独立feature）へ分離し、`Sprite`を使わない消費側（3Dゲーム等）が
//! `atlas`だけを有効化した場合に`bevy_sprite`への依存が発生しないようにする
//! （`devtools`/`devtools-physics3d`の分離と同じパターン）。
//!
//! dirty_wayの`enemy.rs`が持っていた`WalkAnimation`（連番フレームを
//! 一定間隔でループ再生する）を一般化したもの。

use bevy::prelude::*;

use crate::atlas::GutzAtlasRegistry;

/// 名前付きアトラスの連番フレームを一定間隔でループ再生する。フレーム数は
/// 呼び出し側で持たず、都度`GutzAtlasRegistry`（マニフェスト由来）から
/// 引く——2箇所に独立した値を置いて食い違わせないため。
///
/// このComponentを`Sprite`と同じEntityへinsertするだけで、`GutzAtlasPlugin`
/// （`atlas-sprite2d`有効時）が毎フレーム`Sprite.rect`を自動更新する。
#[derive(Component)]
pub struct GutzSpriteAnimation {
    pub name: String,
    pub frame_duration: f32,
    frame: u32,
    timer: f32,
}

impl GutzSpriteAnimation {
    pub fn new(name: impl Into<String>, frame_duration: f32) -> Self {
        Self { name: name.into(), frame_duration, frame: 0, timer: 0.0 }
    }
}

pub(crate) fn drive_sprite_animation(
    time: Res<Time>,
    registry: Res<GutzAtlasRegistry>,
    mut query: Query<(&mut GutzSpriteAnimation, &mut Sprite)>,
) {
    for (mut anim, mut sprite) in &mut query {
        anim.timer += time.delta_secs();
        if anim.timer < anim.frame_duration {
            continue;
        }
        anim.timer -= anim.frame_duration;

        let Some(frame_count) = registry.frame_count(&anim.name) else { continue };
        anim.frame = (anim.frame + 1) % frame_count.max(1);
        if let Some(frame) = registry.frame(&anim.name, anim.frame) {
            sprite.rect = Some(frame.pixel_rect);
        }
    }
}
