//! `GutzSteamPlugin` — Steamworks SDK連携の土台。
//!
//! Bevy側のボイラープレート（`Client`のResource化、コールバックの毎フレーム
//! ポンプ、Message化）は`bevy-steamworks`crateが既によく解決している。
//! gutzgutzがそこを薄くラップし直すのは、Bevy/Avian3D本体を薄くラップしない
//! というこのcrate全体の方針に反する。
//!
//! gutzgutzがここに足す価値は2つだけ：
//! 1. Steamクライアントが起動していない・App IDが未登録、といった状況でも
//!    ゲームを絶対に落とさないグレースフルデグレード（`FoamSlotAllocator`が
//!    枠を使い切っても見た目だけ諦めるのと同じ思想。`bevy_steamworks::
//!    SteamworksPlugin::init_app`は失敗し得るResultを返すが、呼び出し側が
//!    `unwrap`する設計になっているため、生のまま`add_plugins`すると
//!    Steam未接続の環境でゲームごとパニックする）。
//! 2. devtoolsオーバーレイへの接続状態表示（他のgutzgutz機能と同じ
//!    `GutzDebugStats`チャンネル経由）。
//!
//! # 重要：この`Result`グレースフルデグレードだけでは足りない
//!
//! 実機検証で判明した点として、`steam` featureを有効にすると
//! `steamworks-sys`がOS動的リンカレベルで`libsteam_api.so`
//! （Windowsは`steam_api64.dll`、macOSは`libsteam_api.dylib`）を
//! **実行ファイル起動時の必須共有ライブラリ**として要求するようになる。
//! これはRustの`Result`より手前、プロセスがmain()に入る前の
//! OSローダーの段階で起きる失敗なので、`GutzSteamPlugin::build`内の
//! `match`は一切関与できない。ファイルが無ければ
//! 「error while loading shared libraries: libsteam_api.so: cannot open
//! shared object file」でプロセスごと即終了する。
//!
//! この`.so`/`.dll`はValveのSteamworks SDK配布物（`lib/steam/
//! redistributable_bin`配下）に含まれる。実は`steamworks-sys`crate自体が
//! ビルド時リンク用にこれを同梱している（cargoレジストリキャッシュ内、
//! Steamworksパートナーアカウント無しで`cargo build`するだけで手に入る）ので、
//! 開発中の動作確認に限ってはそこからコピーしてくるだけで十分——実際の
//! Steamリリース用App ID登録・ストアページ設定にはパートナーアカウントが
//! 別途要るが、それはこのライブラリ自体の必須条件ではない。
//!
//! `dirty_way`側の`build.rs`が、`steam_redist/linux64/libsteam_api.so`
//! （開発機で一度だけ手動配置。配置手順はbevy_gutzgutz/README.md参照）を
//! 実際の出力ディレクトリへ自動コピーし、`$ORIGIN`をrpathに追加することで、
//! 素の`cargo run`だけで動くようにしてある。
//!
//! 実績・統計・リーダーボード・フレンド・Workshopなど「Steamの何を使うか」
//! はゲーム固有なので、gutzgutzはそれらを個別にラップしない。ゲーム側は
//! `bevy_gutzgutz::steam::sdk`（`bevy-steamworks`の再エクスポート）経由で
//! `Res<sdk::Client>`を直接使う。

use bevy::prelude::*;
/// `bevy-steamworks`（ひいてはその先の`steamworks`クレート）への窓口。
/// 実績・統計・リーダーボード・フレンド等、gutzgutzが個別にラップしない
/// Steam APIはすべてここ経由で使う。
///
/// 生の`sdk::SteamworksPlugin`を直接`add_plugins`しないこと。
/// `init_app`が返す`Result`を`unwrap`する設計のため、Steam未接続の環境で
/// ゲームごとパニックする。必ず[`GutzSteamPlugin`]経由で初期化する。
pub use bevy_steamworks as sdk;

/// Steamへの接続状態。ゲーム側はこれを見て、実績解除・フレンド表示といった
/// 「Steamが前提の機能」を出し分ける。`Unavailable`でもゲーム自体は
/// 普段どおりプレイできなければならない。
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GutzSteamStatus {
    #[default]
    Unavailable,
    Connected,
}

impl GutzSteamStatus {
    pub fn is_connected(self) -> bool {
        matches!(self, GutzSteamStatus::Connected)
    }
}

/// ゲーム固有のApp IDを1つ受け取り、Steamworks SDKの初期化を試みる。
pub struct GutzSteamPlugin {
    app_id: u32,
}

impl GutzSteamPlugin {
    /// 実際のApp IDが割り当てられるまでの開発用。SpaceWar
    /// （Valveが開発・検証用に配布しているテストApp ID）。Steamworks公式
    /// ドキュメント・`bevy-steamworks`の使用例もこの値を使っている。
    pub const DEV_APP_ID: u32 = 480;

    pub fn new(app_id: u32) -> Self {
        Self { app_id }
    }
}

impl Plugin for GutzSteamPlugin {
    fn build(&self, app: &mut App) {
        match sdk::SteamworksPlugin::init_app(self.app_id) {
            Ok(plugin) => {
                app.add_plugins(plugin).insert_resource(GutzSteamStatus::Connected);
            }
            Err(err) => {
                warn!("Steamworks SDKの初期化に失敗したため、Steam機能なしで続行します: {err:?}");
                app.insert_resource(GutzSteamStatus::Unavailable);
            }
        }

        #[cfg(feature = "devtools")]
        app.init_resource::<crate::devtools::GutzDebugStats>()
            .add_systems(Update, update_steam_debug_stats);
    }
}

#[cfg(feature = "devtools")]
fn update_steam_debug_stats(
    status: Res<GutzSteamStatus>,
    mut stats: ResMut<crate::devtools::GutzDebugStats>,
) {
    stats.set("Steam", if status.is_connected() { "Connected" } else { "Unavailable" });
}
