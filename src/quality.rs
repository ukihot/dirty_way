//! 泡表現の品質設定（doc/soap-issues.md S-11a）。
//!
//! Quality Profileが制御するのは「同時に見た目を持てるFoam Aggregateの数」と
//! 「レンダリングの精度」だけで、GPU Foam Instance Poolの物理的な容量
//! （512、`consts::FOAM_INSTANCE_POOL_SIZE`）そのものは変えない。品質を
//! Low→Highへ切り替えてもプールを再構築する必要がないようにするための
//! 意図的な分離（doc/soap-model.md 第31節と同じ「容量と品質を分離する」判断）。
//!
//! 今はキー入力（1/2/3）で切り替えられる暫定UIだけを用意する。将来の設定画面・
//! Auto Dynamic Quality（S-11b）は、このResourceを外側から書き換えるだけで
//! 乗せられる構造にしてある。

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FoamQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl FoamQuality {
    pub fn profile(self) -> FoamQualityProfile {
        match self {
            FoamQuality::Low => FoamQualityProfile {
                max_aggregates: 16,
                raymarch_steps: 32,
                microstructure_quality: MicrostructureQuality::Simple,
            },
            FoamQuality::Medium => FoamQualityProfile {
                max_aggregates: 64,
                raymarch_steps: 64,
                microstructure_quality: MicrostructureQuality::Normal,
            },
            FoamQuality::High => FoamQualityProfile {
                max_aggregates: 256,
                raymarch_steps: 96,
                microstructure_quality: MicrostructureQuality::Detailed,
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FoamQuality::Low => "Low",
            FoamQuality::Medium => "Medium",
            FoamQuality::High => "High",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MicrostructureQuality {
    Simple,
    Normal,
    Detailed,
}

impl MicrostructureQuality {
    /// WGSL側にu32として渡すためのエンコード（soap_render.wgslの同名定数と対応）。
    pub fn as_u32(self) -> u32 {
        match self {
            MicrostructureQuality::Simple => 0,
            MicrostructureQuality::Normal => 1,
            MicrostructureQuality::Detailed => 2,
        }
    }
}

/// Quality 1段階分の具体的な予算。数値は暫定値（doc/soap-issues.md S-11a）。
/// 実機計測してから調整する前提で、数値そのものより「Qualityから
/// このProfileを引ける」という構造の方が重要。
#[derive(Clone, Copy)]
pub struct FoamQualityProfile {
    /// 同時にFoamGpuBindingを持てるBubbleの上限。GPU Instance Poolの
    /// 物理容量（512）とは独立で、これを超えたBubbleは見た目だけ諦める
    /// （ゲームロジックには影響しない）。
    pub max_aggregates: u32,
    pub raymarch_steps: u32,
    pub microstructure_quality: MicrostructureQuality,
}

/// 現在選択中のFoam Quality（Main World）。S-11bのAuto Dynamic Qualityは、
/// このResourceを外側から書き換えるだけで乗せられる。
#[derive(Resource, Default)]
pub struct FoamQualitySetting(pub FoamQuality);

pub struct QualityPlugin;

impl Plugin for QualityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoamQualitySetting>().add_systems(Update, handle_quality_hotkeys);
    }
}

/// 暫定デバッグUI：1/2/3キーでLow/Medium/Highを直接切り替える。実機ベンチマーク
/// （どのGPUでどのQualityが成立するか）を取るための、設定画面ができるまでの代用。
fn handle_quality_hotkeys(keyboard: Res<ButtonInput<KeyCode>>, mut quality: ResMut<FoamQualitySetting>) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        quality.0 = FoamQuality::Low;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        quality.0 = FoamQuality::Medium;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        quality.0 = FoamQuality::High;
    }
}
