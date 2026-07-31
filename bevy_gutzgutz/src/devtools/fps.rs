use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use super::stats::GutzDebugStats;

/// FPS/Frame Timeを標準エントリとして`GutzDebugStats`へ毎フレーム`set`する。
/// `FrameTimeDiagnosticsPlugin`自体は`GutzDevtoolsPlugin`が追加する。
pub(crate) fn update_fps_stats(
    diagnostics: Res<DiagnosticsStore>,
    mut stats: ResMut<GutzDebugStats>,
) {
    let fps =
        diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    stats.set("FPS", format!("{fps:.1}"));
    stats.set("Frame", format!("{frame_ms:.1} ms"));
}
