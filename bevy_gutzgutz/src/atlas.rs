//! `GutzAtlasPlugin` — ビルド時に[`crate::atlas_build::pack`]が生成した
//! アトラス画像+マニフェストを読み込み、texture名+フレーム番号での名前引きを
//! 提供する（README.md「`atlas` — `GutzAtlasPlugin`」参照）。
//!
//! `Sprite`/`TextureAtlas`（2D）を直接組み立てて返すことはしない——dirty_way
//! のような3Dゲームではメッシュ側のUVオフセットとして使いたいこともあるため、
//! 描画方式を問わない生データ（[`GutzAtlasFrame`]）を返し、実際にどう
//! 描画するかは呼び出し側に委ねる。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::Deserialize;

/// マニフェストの読み込みが失敗しうる理由。`load_manifest`（システム）は
/// これを`warn!`するだけなので今のところ内部限りだが、`GutzXxxError`
/// という命名規約（CONTRIBUTION.md）には揃えておく。
#[derive(Debug, thiserror::Error)]
enum GutzAtlasLoadError {
    #[error("{path}を読み込めません: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}のパースに失敗: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// `atlas_build::AtlasManifest`と同じ形。ランタイム側は`toml`を直接読むだけ
/// なので、`atlas_build`本体（`image`crate依存）は要らない。
#[derive(Deserialize)]
struct AtlasManifest {
    textures: HashMap<String, AtlasEntry>,
}

#[derive(Deserialize, Clone)]
struct AtlasEntry {
    image: String,
    frame_count: u32,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
}

/// アトラス内の1フレームの参照。`Sprite`+`TextureAtlas`（2D）でも
/// `StandardMaterial`のUVオフセット（3Dメッシュ）でも、呼び出し側の用途に
/// 合わせて使える最小限のデータだけを持つ。
#[derive(Clone)]
pub struct GutzAtlasFrame {
    pub image: Handle<Image>,
    /// アトラス画像内でのこのフレームのUV矩形（0.0〜1.0正規化、左上原点）。
    pub uv_rect: Rect,
    /// アトラス画像内でのこのフレームのピクセル矩形。Bevyの`Sprite::rect`
    /// （2D）はピクセル空間を取る（0.0〜1.0正規化のuv_rectとは単位が違う）
    /// ので、そのまま渡せる形をここで用意しておく。
    pub pixel_rect: Rect,
}

/// マニフェスト（`manifest.toml`）とアトラス画像群を`assets/`配下から探す
/// ときの基準ディレクトリ。`atlas_build::pack`の`out_dir`と一致させる。
#[derive(Resource, Clone)]
pub struct GutzAtlasManifestPath(pub PathBuf);

impl Default for GutzAtlasManifestPath {
    fn default() -> Self {
        Self(PathBuf::from("assets/generated/atlas"))
    }
}

/// texture名 → (アトラス画像のHandle, マニフェストのエントリ) の名前引き表。
#[derive(Resource, Default)]
pub struct GutzAtlasRegistry {
    entries: HashMap<String, (Handle<Image>, AtlasEntry)>,
}

impl GutzAtlasRegistry {
    /// `name`の`index`番目（0始まり）のフレームを返す。`name`が未登録、または
    /// `index`が`frame_count`を超えている場合は`None`——ゲーム側のtypoは
    /// パニックではなく`Option`として表現する（README.md参照。これを
    /// コンパイル時に防ぐには別途コード生成が要り、v1のスコープ外）。
    pub fn frame(&self, name: &str, index: u32) -> Option<GutzAtlasFrame> {
        let (image, entry) = self.entries.get(name)?;
        if index >= entry.frame_count {
            return None;
        }
        let tile_u = 1.0 / entry.columns as f32;
        let uv_rect = Rect::new(index as f32 * tile_u, 0.0, (index as f32 + 1.0) * tile_u, 1.0);
        let tile_w = entry.tile_width as f32;
        let tile_h = entry.tile_height as f32;
        let pixel_rect =
            Rect::new(index as f32 * tile_w, 0.0, (index as f32 + 1.0) * tile_w, tile_h);
        Some(GutzAtlasFrame { image: image.clone(), uv_rect, pixel_rect })
    }

    /// `name`が持つフレーム数。`name`が未登録なら`None`。
    pub fn frame_count(&self, name: &str) -> Option<u32> {
        self.entries.get(name).map(|(_, entry)| entry.frame_count)
    }
}

pub struct GutzAtlasPlugin;

impl Plugin for GutzAtlasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GutzAtlasManifestPath>()
            .init_resource::<GutzAtlasRegistry>()
            .add_systems(Startup, load_manifest);

        // `atlas`本体は描画方式を問わない設計を保つため、Sprite依存の
        // 自動再生（GutzSpriteAnimation）は独立feature（atlas-sprite2d）
        // 側にのみ存在する。
        #[cfg(feature = "atlas-sprite2d")]
        app.add_systems(Update, crate::atlas_sprite2d::drive_sprite_animation);
    }
}

/// ディスクから`manifest.toml`を読んでパースするだけの関数。`Result`一直線
/// にし、Bevy側（`Res`/`ResMut`/`AssetServer`）とは無関係にしておく。
fn read_manifest(manifest_dir: &Path) -> Result<AtlasManifest, GutzAtlasLoadError> {
    let manifest_file = manifest_dir.join("manifest.toml");
    let contents = std::fs::read_to_string(&manifest_file)
        .map_err(|source| GutzAtlasLoadError::Read { path: manifest_file.clone(), source })?;
    toml::from_str(&contents)
        .map_err(|source| GutzAtlasLoadError::Parse { path: manifest_file, source })
}

fn load_manifest(
    manifest_path: Res<GutzAtlasManifestPath>,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<GutzAtlasRegistry>,
) {
    let manifest = match read_manifest(&manifest_path.0) {
        Ok(manifest) => manifest,
        Err(error) => {
            warn!("gutzgutz atlas: {error}");
            return;
        }
    };

    // AssetServer::loadは`assets/`からの相対パスを取るため、manifest_pathの
    // 先頭が`assets/`であれば取り除く。それ以外の配置（assets外）は
    // 現状サポート外——AssetServerからロードできない。
    let asset_relative_dir =
        manifest_path.0.strip_prefix("assets").unwrap_or(&manifest_path.0).to_path_buf();

    for (name, entry) in manifest.textures {
        let relative_path = asset_relative_dir.join(&entry.image);
        let handle = asset_server.load(relative_path.to_string_lossy().into_owned());
        registry.entries.insert(name, (handle, entry));
    }
}
