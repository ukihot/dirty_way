//! `GutzRampTimer` — 一定間隔で発火する周期タイマーで、経過時間に応じて
//! 間隔自体が線形にランプ（変化）していく。
//!
//! dirty_wayの`enemy.rs`（`EnemySpawnTimer`、敵の出現間隔を時間経過で
//! 短くしていく）で最初に書かれ、`player.rs`（`NozzlePress.spray_cooldown`、
//! ランプなしの固定間隔クールダウンとして）で独立に2度目が書かれた
//! 「周期的に再アームされるタイマー」パターンを抽出したもの。
//! ホラー/タワーディフェンス/パズルのような波状に敵・イベントを発生させる
//! ゲームで繰り返し必要になる想定。
//!
//! `Resource`/`Component`自体は提供しない。ゲーム側が自分のResource/
//! Componentに埋め込んで使う（`bevy::time::Timer`と同じ「素材」の位置づけ。
//! `Plugin`も持たない——`devtools::GutzDebugStats`/`GutzSpawnRegistry`と
//! 同様、状態を持つだけの型で、駆動（`tick`呼び出し）はゲーム側のシステムが
//! 行う）。

use std::time::Duration;

use bevy::time::{Timer, TimerMode};

/// 周期タイマー。間隔は`start`から始まり、経過時間×`ramp_per_sec`だけ
/// 線形に短くなっていき、`min`を下限にクランプされる。`ramp_per_sec`に
/// `0.0`を渡せば（`start == min`にしておけば）、ランプなしの固定間隔
/// クールダウンとしても使える。
pub struct GutzRampTimer {
    timer: Timer,
    elapsed: f32,
    start: f32,
    min: f32,
    ramp_per_sec: f32,
}

impl GutzRampTimer {
    pub fn new(start: f32, min: f32, ramp_per_sec: f32) -> Self {
        Self {
            timer: Timer::from_seconds(start, TimerMode::Repeating),
            elapsed: 0.0,
            start,
            min,
            ramp_per_sec,
        }
    }

    /// 経過時間を進め、現在の間隔を再計算してタイマーを再アームした上で、
    /// 「今回のtickで発火したか」を返す。
    pub fn tick(&mut self, delta: Duration) -> bool {
        self.elapsed += delta.as_secs_f32();
        let interval = (self.start - self.elapsed * self.ramp_per_sec).max(self.min);
        self.timer.set_duration(Duration::from_secs_f32(interval));
        self.timer.tick(delta);
        self.timer.just_finished()
    }

    /// 現在（ランプ後）の発火間隔。デバッグ表示等に。
    pub fn current_interval(&self) -> f32 {
        self.timer.duration().as_secs_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_interval_when_ramp_is_zero() {
        let mut timer = GutzRampTimer::new(1.0, 1.0, 0.0);
        assert!(!timer.tick(Duration::from_millis(999)));
        assert!(timer.tick(Duration::from_millis(1)));
        assert_eq!(timer.current_interval(), 1.0);
    }

    #[test]
    fn interval_ramps_down_and_clamps_to_min() {
        let mut timer = GutzRampTimer::new(2.0, 0.5, 1.0);
        // 1秒経過した時点で間隔は 2.0 - 1.0*1.0 = 1.0 になっているはず。
        timer.tick(Duration::from_secs(1));
        assert_eq!(timer.current_interval(), 1.0);
        // 十分経過すれば min でクランプされる。
        timer.tick(Duration::from_secs(10));
        assert_eq!(timer.current_interval(), 0.5);
    }
}
