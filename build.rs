//! dirty_wayのビルドスクリプト。2つの独立した仕事をする：
//!
//! 1. Steamworks再配布ライブラリ（`libsteam_api.so`）を実行ファイルの隣に配置
//!    （[`sync_steam_redist`]）
//! 2. `assets/character/`配下の連番PNGから、`bevy_gutzgutz::atlas_build`で
//!    アトラス画像+マニフェストを生成（[`pack_character_atlas`]）
//!
//! どちらも「対応する入力が存在しなければ何もしない」——steam
//! feature無効時やWindows/macOS等、あるいは`assets/character/`がまだ無い
//! 段階のビルドを壊さないため。

use std::path::{Path, PathBuf};

fn main() {
    sync_steam_redist();
    pack_character_atlas();
}

/// `steamworks-sys`はビルド時のシンボル解決に必要な`.so`を自身のcrate内
/// （cargoレジストリキャッシュ）から見つけて使うが、そこは実行時の動的
/// リンカの探索パスには含まれない。かつこのcrateはrpathを設定しないため、
/// 素の`cargo run`では「libsteam_api.so: cannot open shared object file」で
/// プロセスがmain()に入る前に落ちる（`GutzSteamPlugin`のグレースフル
/// デグレードより手前で起きる失敗のため、Rust側のResultハンドリングでは
/// 救えない）。
///
/// ここでは`steam_redist/linux64/libsteam_api.so`
/// （開発機で一度だけ手動配置。bevy_gutzgutz/README.md参照）を実際の出力
/// ディレクトリへコピーし、`$ORIGIN`（実行ファイル自身のディレクトリ）を
/// rpathに追加することで、`cargo run`だけで動くようにする。
/// ファイルが未配置なら何もしない（steam feature無効時・Windows/macOS等
/// 他プラットフォーム向けビルド時に無害にスキップされる）。
fn sync_steam_redist() {
    println!("cargo:rerun-if-changed=steam_redist/linux64/libsteam_api.so");

    let redist =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("steam_redist/linux64/libsteam_api.so");
    if !redist.exists() {
        return;
    }

    // OUT_DIR は target/{profile}/build/dirty_way-{hash}/out。
    // 実行ファイル本体が置かれる target/{profile}/ まで3階層上がる。
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let Some(target_dir) = out_dir.ancestors().nth(3) else { return };

    let _ = std::fs::copy(&redist, target_dir.join("libsteam_api.so"));

    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
}

/// `assets/character/`（イラストレーターが`{texture名}_{最大枚数}/`フォルダ
/// ごとに連番PNGを納品する場所。命名規約はbevy_gutzgutz/README.md
/// 「`atlas` / `atlas-build`」参照）を検証・packし、`assets/generated/atlas/`
/// へアトラス画像+`manifest.toml`を出力する。実行時は`GutzAtlasPlugin`
/// （デフォルト設定で`assets/generated/atlas/`を読む）がこれを読み込む。
///
/// `assets/character/`がまだ存在しない場合は何もしない。存在するのに
/// 命名規約違反・番号の歯抜け等があれば、`cargo build`/`cargo run`自体を
/// 失敗させる——存在しないテクスチャを参照するキャラクターがそのまま
/// `cargo run`されてしまう事故を防ぐのが目的なので、ここは黙って
/// スキップしてはいけない。
fn pack_character_atlas() {
    println!("cargo:rerun-if-changed=assets/character");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("assets/character");
    if !src_dir.exists() {
        return;
    }

    let out_dir = manifest_dir.join("assets/generated/atlas");
    if let Err(error) = bevy_gutzgutz::atlas_build::pack(&src_dir, &out_dir) {
        println!("cargo::error=assets/character のアトラス生成に失敗しました: {error}");
        std::process::exit(1);
    }
}
