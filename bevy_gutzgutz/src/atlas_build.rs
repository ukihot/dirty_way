//! `atlas`（ランタイム）が読むアトラス画像とマニフェストを、命名規約に従った
//! ディレクトリ構造から生成するビルド時ロジック。ゲーム側の`build.rs`から
//! [`pack`]を呼ぶ（README.md「`atlas` / `atlas-build` — `GutzAtlasPlugin`」
//! 参照）。
//!
//! Bevy本体には依存しない（`image`/`serde`/`toml`のみ）——ビルドスクリプトの
//! 依存グラフを軽量に保つため、このモジュールに`use bevy::`は一切書かない。
//!
//! # ディレクトリ構造
//!
//! ```text
//! character/
//!     └── knight/
//!         ├── idle_8/
//!         │   ├── 1.png
//!         │   ├── 2.png
//!         │   └── ...
//!         ├── walk_6/
//!         │   └── ...
//!         └── attack_4/
//!             └── ...
//! ```
//!
//! 「画像ファイルを直接含むフォルダ」（leaf）だけが1つのtexture（アニメー
//! ション1本）を表し、フォルダ名は`{texture名}_{最大枚数}`でなければ
//! ならない。それ以外のフォルダ（namespace、上の例なら`character/`や
//! `character/knight/`）は自由な名前で何段でもネストでき、最終的なtexture名は
//! namespaceのパス＋leafのtexture名を`/`で連結したもの（例：
//! `character/knight/idle`）になる。
//!
//! 1つのフォルダに画像ファイルとサブフォルダを混在させることはできない
//! （leafかnamespaceかが曖昧になるため）。png以外のファイルが混じっている
//! こともエラーになる。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// `pack`が失敗しうる理由。CONTRIBUTION.mdの方針どおり、`String`で
/// その場限りのメッセージを組み立てるのではなく型で分ける。`Display`
/// （`#[error(...)]`）の文面は、以前`String`で組み立てていた文面と同じにして
/// あるので、`build.rs`側の`println!("cargo::error={error}")`は変更不要。
#[derive(Debug, thiserror::Error)]
pub enum GutzAtlasBuildError {
    #[error("{path}を読み取れません: {source}")]
    ReadDir { path: PathBuf, #[source] source: std::io::Error },

    #[error("{dir}の走査に失敗: {source}")]
    ReadEntry { dir: PathBuf, #[source] source: std::io::Error },

    #[error("{path}: png以外のファイルが混じっています")]
    UnexpectedFile { path: PathBuf },

    #[error(
        "{dir}: 画像ファイルとサブフォルダが同じフォルダに混在しています（このフォルダは`{{texture名}}_{{最大枚数}}`という連番画像フォルダか、サブフォルダだけを持つネームスペースフォルダのどちらかである必要があります）"
    )]
    MixedLeafAndNamespace { dir: PathBuf },

    #[error(
        "{dir}: ルート直下に画像を直接置くことはできません。`{{texture名}}_{{最大枚数}}/`という名前のサブフォルダを作り、その中に画像を置いてください"
    )]
    ImagesDirectlyUnderRoot { dir: PathBuf },

    #[error("{dir}: フォルダ名がUTF-8ではありません")]
    NonUtf8DirName { dir: PathBuf },

    #[error("{dir}: フォルダ名が命名規約 `{{texture名}}_{{最大枚数}}` に一致しません")]
    MalformedLeafDirName { dir: PathBuf },

    #[error("{path}: ファイル名がUTF-8ではありません")]
    NonUtf8FileName { path: PathBuf },

    #[error("{path}: ファイル名が命名規約 `{{番号}}.png` に一致しません")]
    MalformedFrameFileName { path: PathBuf },

    #[error("texture `{texture_name}`: 番号{index}が重複しています（{first}と{second}）")]
    DuplicateIndex { texture_name: String, index: u32, first: PathBuf, second: PathBuf },

    #[error(
        "texture `{texture_name}`: 番号の集合がフォルダ名の宣言（1..={declared_max}）と一致しません。{detail}"
    )]
    IndexSetMismatch { texture_name: String, declared_max: u32, detail: String },

    #[error(
        "texture `{texture_name}`: フレームサイズが一致しません（1番目は{first_width}x{first_height}だが{path}は{width}x{height}）"
    )]
    FrameSizeMismatch {
        texture_name: String,
        first_width: u32,
        first_height: u32,
        path: PathBuf,
        width: u32,
        height: u32,
    },

    #[error("{path}: 読み込みに失敗: {source}")]
    OpenImage { path: PathBuf, #[source] source: image::ImageError },

    #[error("{path}の書き込みに失敗: {source}")]
    SaveImage { path: PathBuf, #[source] source: image::ImageError },

    #[error("{path}を作成できません: {source}")]
    CreateDir { path: PathBuf, #[source] source: std::io::Error },

    #[error("マニフェストのシリアライズに失敗: {0}")]
    SerializeManifest(#[from] toml::ser::Error),

    #[error("{path}の書き込みに失敗: {source}")]
    WriteManifest { path: PathBuf, #[source] source: std::io::Error },

    #[error("{src_dir}: 命名規約に沿ったtextureフォルダが1つもありません")]
    NoTextures { src_dir: PathBuf },
}

/// `manifest.toml`の中身。`atlas`ランタイム側もこの型（の同一定義）を読む。
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AtlasManifest {
    /// キーはtexture名（namespaceを`/`で連結したもの。例：
    /// `character/knight/idle`）。
    pub textures: BTreeMap<String, AtlasEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AtlasEntry {
    /// `manifest.toml`と同じディレクトリからの相対パス（`/`区切り）。
    pub image: String,
    pub frame_count: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    /// 現状は横一列packなので`frame_count`と常に等しいが、将来グリッド化
    /// した場合に備えて分けておく。
    pub columns: u32,
}

/// `src_dir`を再帰的に走査し、命名規約に従ったleafフォルダ群を検証・pack
/// して、`out_dir`へアトラス画像群（namespaceと同じディレクトリ構造で
/// 出力する）と`manifest.toml`を出力する。`out_dir`は実行時に
/// `AssetServer`から見える場所（ゲームの`assets/`配下）を想定している——
/// `OUT_DIR`に書いても`AssetServer`からは読めないので注意。
///
/// 命名規約違反・番号の歯抜けや末尾欠け・番号の付け過ぎ・フレームサイズ
/// 不一致は、どれも最初の1件で`Err`として返す。呼び出し側の`build.rs`は
/// これを`cargo::error=`として出力し、ビルドを止めることを想定している。
pub fn pack(src_dir: impl AsRef<Path>, out_dir: impl AsRef<Path>) -> Result<(), GutzAtlasBuildError> {
    let src_dir = src_dir.as_ref();
    let out_dir = out_dir.as_ref();

    let groups = collect_groups(src_dir)?;

    let mut textures = BTreeMap::new();
    for group in &groups {
        let (texture_name, entry) = pack_one(group, out_dir)?;
        textures.insert(texture_name, entry);
    }

    write_manifest(&AtlasManifest { textures }, out_dir)
}

struct SourceFile {
    index: u32,
    path: PathBuf,
}

/// 1つのleafフォルダ（＝1つのtexture、アニメーション1本分）。
struct TextureGroup {
    /// namespaceパス＋leaf名（`_最大枚数`は除いた状態）。
    /// 例：`["character", "knight", "idle"]`。
    segments: Vec<String>,
    max: u32,
    files: Vec<SourceFile>,
}

fn collect_groups(src_dir: &Path) -> Result<Vec<TextureGroup>, GutzAtlasBuildError> {
    let mut groups = Vec::new();
    // ルート（src_dir自身）は名前をtexture名に含めないので`own_name`は
    // `None`から始める。leafかnamespaceかは中身を見るまで分からないため、
    // 「このディレクトリ自身がsegmentsへ何を足すか」は`walk`の中で、
    // 中身を見た後に決める（親側で先読みして`push`しない）。
    walk(src_dir, None, &[], 0, &mut groups)?;
    if groups.is_empty() {
        return Err(GutzAtlasBuildError::NoTextures { src_dir: src_dir.to_path_buf() });
    }
    Ok(groups)
}

fn walk(
    dir: &Path,
    own_name: Option<&str>,
    parent_segments: &[String],
    depth: usize,
    groups: &mut Vec<TextureGroup>,
) -> Result<(), GutzAtlasBuildError> {
    let (png_files, subdirs) = list_dir(dir)?;

    if !png_files.is_empty() && !subdirs.is_empty() {
        return Err(GutzAtlasBuildError::MixedLeafAndNamespace { dir: dir.to_path_buf() });
    }

    if !png_files.is_empty() {
        return finish_leaf(dir, own_name, parent_segments, depth, png_files, groups);
    }

    // namespace: このディレクトリ自身の生の名前（`own_name`）をsegmentsへ
    // 足してから子ディレクトリへ再帰する。ルート（`own_name`が`None`）では
    // 何も足さない。
    let mut segments = parent_segments.to_vec();
    if let Some(name) = own_name {
        segments.push(name.to_string());
    }

    for subdir in &subdirs {
        let name = subdir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| GutzAtlasBuildError::NonUtf8DirName { dir: subdir.clone() })?;
        walk(subdir, Some(name), &segments, depth + 1, groups)?;
    }

    Ok(())
}

/// `dir`直下のpngファイルとサブディレクトリを分類して返す。png以外の
/// ファイルが混じっていた時点でエラーにする。
fn list_dir(dir: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), GutzAtlasBuildError> {
    let read_dir = std::fs::read_dir(dir)
        .map_err(|source| GutzAtlasBuildError::ReadDir { path: dir.to_path_buf(), source })?;

    let mut png_files = Vec::new();
    let mut subdirs = Vec::new();
    for entry in read_dir {
        let entry =
            entry.map_err(|source| GutzAtlasBuildError::ReadEntry { dir: dir.to_path_buf(), source })?;
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.is_file() {
            if path.extension().and_then(|ext| ext.to_str()) == Some("png") {
                png_files.push(path);
            } else {
                return Err(GutzAtlasBuildError::UnexpectedFile { path });
            }
        }
    }
    Ok((png_files, subdirs))
}

/// `dir`がleaf（画像ファイルを直接持つフォルダ）だと分かった後の処理。
/// フォルダ名・ファイル名を検証し、`TextureGroup`を1つ`groups`へ積む。
fn finish_leaf(
    dir: &Path,
    own_name: Option<&str>,
    parent_segments: &[String],
    depth: usize,
    png_files: Vec<PathBuf>,
    groups: &mut Vec<TextureGroup>,
) -> Result<(), GutzAtlasBuildError> {
    if depth == 0 {
        return Err(GutzAtlasBuildError::ImagesDirectlyUnderRoot { dir: dir.to_path_buf() });
    }

    // `own_name`（生のフォルダ名、例：`idle_4`）をtexture名として使うの
    // ではなく、そこから`_最大枚数`を剥がした`leaf_name`（例：`idle`）だけを
    // segmentsへ足す。
    let dir_name =
        own_name.ok_or_else(|| GutzAtlasBuildError::NonUtf8DirName { dir: dir.to_path_buf() })?;
    let (leaf_name, max) = parse_leaf_dir_name(dir_name)
        .ok_or_else(|| GutzAtlasBuildError::MalformedLeafDirName { dir: dir.to_path_buf() })?;

    let mut files = Vec::with_capacity(png_files.len());
    for path in png_files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| GutzAtlasBuildError::NonUtf8FileName { path: path.clone() })?;
        let index = parse_frame_file_name(file_name)
            .ok_or_else(|| GutzAtlasBuildError::MalformedFrameFileName { path: path.clone() })?;
        files.push(SourceFile { index, path });
    }

    let mut segments = parent_segments.to_vec();
    segments.push(leaf_name);
    groups.push(TextureGroup { segments, max, files });
    Ok(())
}

/// `idle_8` → `("idle", 8)`。`texture名`部分にアンダースコアが含まれていても
/// 最後の`_数値`だけを最大枚数として切り出す。
fn parse_leaf_dir_name(dir_name: &str) -> Option<(String, u32)> {
    let (name, max) = dir_name.rsplit_once('_')?;
    let max: u32 = max.parse().ok()?;
    if name.is_empty() || max == 0 {
        return None;
    }
    Some((name.to_string(), max))
}

/// `1.png`/`001.png` → `1`。ゼロ埋めしてもしなくてもよい（`01.png`と
/// `1.png`が両方存在すると番号の重複として弾かれる）。
fn parse_frame_file_name(file_name: &str) -> Option<u32> {
    let stem = file_name.strip_suffix(".png")?;
    if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let index: u32 = stem.parse().ok()?;
    if index == 0 {
        return None;
    }
    Some(index)
}

fn pack_one(group: &TextureGroup, out_dir: &Path) -> Result<(String, AtlasEntry), GutzAtlasBuildError> {
    let texture_name = group.segments.join("/");

    let by_index = index_files(&texture_name, group)?;
    check_index_set(&texture_name, group.max, &by_index)?;
    let (frames, tile_w, tile_h) = load_frames(&texture_name, group.max, &by_index)?;

    let columns = frames.len() as u32;
    let atlas = compose_atlas(tile_w, tile_h, &frames);

    let image_rel_path: PathBuf = group.segments.iter().collect::<PathBuf>().with_extension("png");
    save_atlas_image(&atlas, &out_dir.join(&image_rel_path))?;

    let entry = AtlasEntry {
        image: image_rel_path.to_string_lossy().replace('\\', "/"),
        frame_count: columns,
        tile_width: tile_w,
        tile_height: tile_h,
        columns,
    };
    Ok((texture_name, entry))
}

/// ファイル名の番号→ファイルの対応表を作る。同じ番号が2回出てきたら
/// `DuplicateIndex`。
fn index_files<'a>(
    texture_name: &str,
    group: &'a TextureGroup,
) -> Result<BTreeMap<u32, &'a SourceFile>, GutzAtlasBuildError> {
    let mut by_index: BTreeMap<u32, &SourceFile> = BTreeMap::new();
    for file in &group.files {
        if let Some(existing) = by_index.insert(file.index, file) {
            return Err(GutzAtlasBuildError::DuplicateIndex {
                texture_name: texture_name.to_string(),
                index: file.index,
                first: existing.path.clone(),
                second: file.path.clone(),
            });
        }
    }
    Ok(by_index)
}

/// 検出された番号の集合が`1..=declared_max`とちょうど一致するかを見る。
/// 中落ち・末尾欠け・番号の付け過ぎをこの1箇所で検出する。
fn check_index_set(
    texture_name: &str,
    declared_max: u32,
    by_index: &BTreeMap<u32, &SourceFile>,
) -> Result<(), GutzAtlasBuildError> {
    let found: BTreeSet<u32> = by_index.keys().copied().collect();
    let expected: BTreeSet<u32> = (1..=declared_max).collect();
    if found == expected {
        return Ok(());
    }

    let missing: Vec<String> = expected.difference(&found).map(|i| format!("{i}.png")).collect();
    let extra: Vec<String> = found.difference(&expected).map(|i| format!("{i}.png")).collect();
    let mut detail = String::new();
    if !missing.is_empty() {
        detail.push_str(&format!("不足（中落ちまたは末尾欠け）: {}。", missing.join(", ")));
    }
    if !extra.is_empty() {
        detail.push_str(&format!("宣言を超える番号: {}。", extra.join(", ")));
    }

    Err(GutzAtlasBuildError::IndexSetMismatch {
        texture_name: texture_name.to_string(),
        declared_max,
        detail,
    })
}

/// `1..=declared_max`の各フレームを読み込み、全フレームが同じサイズで
/// あることを確認する。戻り値は`(フレーム画像列, タイル幅, タイル高さ)`。
fn load_frames(
    texture_name: &str,
    declared_max: u32,
    by_index: &BTreeMap<u32, &SourceFile>,
) -> Result<(Vec<image::RgbaImage>, u32, u32), GutzAtlasBuildError> {
    let mut frames = Vec::with_capacity(declared_max as usize);
    let mut tile_size: Option<(u32, u32)> = None;

    for index in 1..=declared_max {
        let file = by_index[&index];
        let img = image::open(&file.path)
            .map_err(|source| GutzAtlasBuildError::OpenImage { path: file.path.clone(), source })?;
        let (width, height) = (img.width(), img.height());

        let (first_width, first_height) = *tile_size.get_or_insert((width, height));
        if (width, height) != (first_width, first_height) {
            return Err(GutzAtlasBuildError::FrameSizeMismatch {
                texture_name: texture_name.to_string(),
                first_width,
                first_height,
                path: file.path.clone(),
                width,
                height,
            });
        }

        frames.push(img.to_rgba8());
    }

    let (tile_w, tile_h) = tile_size.expect("declared_max >= 1 はparse_leaf_dir_nameで検証済み");
    Ok((frames, tile_w, tile_h))
}

fn compose_atlas(tile_w: u32, tile_h: u32, frames: &[image::RgbaImage]) -> image::RgbaImage {
    let columns = frames.len() as u32;
    let mut atlas = image::RgbaImage::new(tile_w * columns, tile_h);
    for (i, frame) in frames.iter().enumerate() {
        image::imageops::replace(&mut atlas, frame, (i as u32 * tile_w) as i64, 0);
    }
    atlas
}

fn save_atlas_image(atlas: &image::RgbaImage, out_path: &Path) -> Result<(), GutzAtlasBuildError> {
    if let Some(parent) = out_path.parent() {
        create_dir_all(parent)?;
    }
    atlas
        .save(out_path)
        .map_err(|source| GutzAtlasBuildError::SaveImage { path: out_path.to_path_buf(), source })
}

fn write_manifest(manifest: &AtlasManifest, out_dir: &Path) -> Result<(), GutzAtlasBuildError> {
    create_dir_all(out_dir)?;
    let contents = toml::to_string_pretty(manifest)?;
    let path = out_dir.join("manifest.toml");
    std::fs::write(&path, contents)
        .map_err(|source| GutzAtlasBuildError::WriteManifest { path, source })
}

fn create_dir_all(dir: &Path) -> Result<(), GutzAtlasBuildError> {
    std::fs::create_dir_all(dir)
        .map_err(|source| GutzAtlasBuildError::CreateDir { path: dir.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, width: u32, height: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        img.save(path).unwrap();
    }

    #[test]
    fn packs_a_nested_complete_sequence() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let base = src.path().join("character/knight/idle_4");
        for i in 1..=4 {
            write_png(&base.join(format!("{i}.png")), 8, 8);
        }

        pack(src.path(), out.path()).unwrap();

        let manifest_text = std::fs::read_to_string(out.path().join("manifest.toml")).unwrap();
        let manifest: AtlasManifest = toml::from_str(&manifest_text).unwrap();
        let entry = manifest.textures.get("character/knight/idle").unwrap();
        assert_eq!(entry.frame_count, 4);
        assert_eq!(entry.image, "character/knight/idle.png");
        assert!(out.path().join("character/knight/idle.png").exists());
    }

    #[test]
    fn packs_multiple_sequences_under_the_same_namespace() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        for i in 1..=8 {
            write_png(&src.path().join(format!("character/knight/idle_8/{i}.png")), 4, 4);
        }
        for i in 1..=6 {
            write_png(&src.path().join(format!("character/knight/walk_6/{i}.png")), 4, 4);
        }

        pack(src.path(), out.path()).unwrap();

        let manifest_text = std::fs::read_to_string(out.path().join("manifest.toml")).unwrap();
        let manifest: AtlasManifest = toml::from_str(&manifest_text).unwrap();
        assert_eq!(manifest.textures.get("character/knight/idle").unwrap().frame_count, 8);
        assert_eq!(manifest.textures.get("character/knight/walk").unwrap().frame_count, 6);
    }

    #[test]
    fn rejects_middle_gap() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        for i in [1, 2, 4] {
            write_png(&src.path().join(format!("knight/idle_4/{i}.png")), 8, 8);
        }

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("3.png"), "error was: {error}");
    }

    #[test]
    fn rejects_trailing_shortfall() {
        // 1〜3が連続しているだけでは「歯抜け無し」に見えてしまうケース
        // （本来5枚納品のはずが3枚しか届いていない、末尾欠け）。
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        for i in 1..=3 {
            write_png(&src.path().join(format!("knight/idle_5/{i}.png")), 8, 8);
        }

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("4.png"), "error was: {error}");
        assert!(error.contains("5.png"), "error was: {error}");
    }

    #[test]
    fn rejects_numbers_beyond_declared_max() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        for i in 1..=5 {
            write_png(&src.path().join(format!("knight/idle_4/{i}.png")), 8, 8);
        }

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("宣言を超える番号"), "error was: {error}");
        assert!(error.contains("5.png"), "error was: {error}");
    }

    #[test]
    fn rejects_malformed_leaf_dir_name() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&src.path().join("knight/idle/1.png"), 8, 8);

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("命名規約"), "error was: {error}");
    }

    #[test]
    fn rejects_non_numeric_frame_name() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&src.path().join("knight/idle_2/1.png"), 8, 8);
        write_png(&src.path().join("knight/idle_2/a.png"), 8, 8);

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("命名規約"), "error was: {error}");
    }

    #[test]
    fn rejects_size_mismatch() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&src.path().join("knight/idle_2/1.png"), 8, 8);
        write_png(&src.path().join("knight/idle_2/2.png"), 16, 16);

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("フレームサイズが一致しません"), "error was: {error}");
    }

    #[test]
    fn rejects_files_and_subdirs_mixed_in_one_folder() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&src.path().join("knight/idle_1/1.png"), 8, 8);
        write_png(&src.path().join("knight/idle_1/nested_1/1.png"), 8, 8);

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("混在"), "error was: {error}");
    }

    #[test]
    fn rejects_images_directly_under_root() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&src.path().join("1.png"), 8, 8);

        let error = pack(src.path(), out.path()).unwrap_err().to_string();
        assert!(error.contains("ルート直下"), "error was: {error}");
    }
}
