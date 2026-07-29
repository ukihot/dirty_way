//! `GutzSavePlugin` — 「セーブファイル」ではなく「ゲーム状態の永続化」。
//!
//! Saveはゲーム状態をディスクへ出し入れするための**インフラ**であり、
//! ゲームロジックそのものを一切知らない。gutzgutzは生きている`World`から
//! 何を保存するかを決めたり、読み込んだ値を勝手に`World`へ書き戻したり
//! しない——**セーブデータと実行中のWorldを分離する**。
//!
//! ゲーム側は自分の保存したいデータを1つのplain-oldなシリアライズ可能な
//! 型（`GutzSaveData`）として定義し、
//!
//! - 保存したい時に[`GutzSaveRequest`]へその値を積んで送る
//! - 読み込みたい時に[`GutzLoadRequest`]を送り、結果を[`GutzLoaded`]（成功）
//!   または[`GutzLoadFailed`]（失敗）で受け取り、自分のResource/Stateへ
//!   反映するかどうかも含めて自分で決める
//!
//! という一往復だけをgutzgutzに任せる。フォーマットはTOML（人間が読み書き
//! しやすい）。

use bevy::prelude::*;
use core::marker::PhantomData;
use std::path::{Path, PathBuf};

/// 保存/読み込みが失敗しうる理由を型で分ける。`handle_save_requests`/
/// `handle_load_requests`（システム）は、これを`{error}`でログに出す以上の
/// ことをしないため今のところ`pub`である必要は薄いが、`GutzXxxError`という
/// 命名規約（CONTRIBUTION.md）に揃え、将来ゲーム側がエラー種別で分岐したく
/// なったときにそのまま公開APIとして使えるようにしておく。
#[derive(Debug, thiserror::Error)]
pub enum GutzSaveError {
    #[error("failed to serialize save data: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to deserialize save data: {0}")]
    Deserialize(#[from] toml::de::Error),
    #[error("failed to create directory {path}: {source}")]
    CreateDir { path: PathBuf, #[source] source: std::io::Error },
    #[error("failed to write {path}: {source}")]
    Write { path: PathBuf, #[source] source: std::io::Error },
    #[error("failed to read {path}: {source}")]
    Read { path: PathBuf, #[source] source: std::io::Error },
}

/// 実際にディスクへ書く部分だけを切り出した、`Result`一直線の関数。
/// システム（`handle_save_requests`）側はこれを呼んでログに出すだけにし、
/// 「正常系のI/O手順」と「Bevyのメッセージ処理」を混ぜない。
fn save_to_disk<T: GutzSaveData>(path: &Path, data: &T) -> Result<(), GutzSaveError> {
    let contents = toml::to_string_pretty(data)?;

    // OS標準のセーブ場所（`standard_location`）は初回起動時点ではまだ
    // 存在しないディレクトリを指すのが普通なので、書き込み前に親
    // ディレクトリを作る。
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| GutzSaveError::CreateDir { path: parent.to_path_buf(), source })?;
    }

    std::fs::write(path, contents)
        .map_err(|source| GutzSaveError::Write { path: path.to_path_buf(), source })
}

/// ディスクから読んでデシリアライズするだけの関数。`save_to_disk`と対称。
fn load_from_disk<T: GutzSaveData>(path: &Path) -> Result<T, GutzSaveError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| GutzSaveError::Read { path: path.to_path_buf(), source })?;
    Ok(toml::from_str(&contents)?)
}

/// ゲーム側が永続化したいデータ型が満たすべき制約をまとめたマーカー
/// トレイト。`serde::Serialize`/`DeserializeOwned`を実装したplain-oldな
/// データ型なら自動的に満たされる。
///
/// ```ignore
/// #[derive(Clone, serde::Serialize, serde::Deserialize)]
/// struct SaveData {
///     high_score: u32,
/// }
/// impl GutzSaveData for SaveData {}
/// ```
pub trait GutzSaveData: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static {}

impl<T: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static> GutzSaveData for T {}

/// 「このデータを保存してほしい」というゲーム側からの明示的な要求。
/// gutzgutzは`World`から値を抜き出したりしない——保存したい値そのものを
/// 呼び出し側が積んで渡す。
#[derive(Message, Clone)]
pub struct GutzSaveRequest<T: GutzSaveData>(pub T);

/// 「保存済みのデータを読み込んでほしい」という要求。
#[derive(Message)]
pub struct GutzLoadRequest<T: GutzSaveData>(PhantomData<fn() -> T>);

impl<T: GutzSaveData> Default for GutzLoadRequest<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: GutzSaveData> Clone for GutzLoadRequest<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: GutzSaveData> Copy for GutzLoadRequest<T> {}

/// 読み込みに成功した。中身をどう`World`へ反映するかはゲーム側が決める。
#[derive(Message, Clone)]
pub struct GutzLoaded<T: GutzSaveData>(pub T);

/// 読み込みに失敗した（ファイルが無い・壊れている等）。理由の詳細は
/// ログに出す（devtoolsの`GutzDebugStats`等へ載せたい場合はゲーム側で購読）。
#[derive(Message, Clone, Copy)]
pub struct GutzLoadFailed<T: GutzSaveData>(PhantomData<fn() -> T>);

fn handle_save_requests<T: GutzSaveData>(
    mut requests: MessageReader<GutzSaveRequest<T>>,
    path: Res<GutzSavePath<T>>,
) {
    for request in requests.read() {
        if let Err(error) = save_to_disk(&path.path, &request.0) {
            bevy::log::warn!("gutzgutz save: {error}");
        }
    }
}

fn handle_load_requests<T: GutzSaveData>(
    mut requests: MessageReader<GutzLoadRequest<T>>,
    path: Res<GutzSavePath<T>>,
    mut loaded: MessageWriter<GutzLoaded<T>>,
    mut failed: MessageWriter<GutzLoadFailed<T>>,
) {
    for _ in requests.read() {
        match load_from_disk(&path.path) {
            Ok(data) => {
                loaded.write(GutzLoaded(data));
            }
            Err(error) => {
                bevy::log::warn!("gutzgutz save: {error}");
                failed.write(GutzLoadFailed(PhantomData));
            }
        }
    }
}

#[derive(Resource)]
struct GutzSavePath<T> {
    path: PathBuf,
    _marker: PhantomData<fn() -> T>,
}

/// データ型`T`を1つ受け取り、そのセーブ/ロード一式を配線するプラグイン。
/// `path`はセーブファイルの保存先（TOML形式）。
///
/// ```ignore
/// app.add_plugins(GutzSavePlugin::<SaveData>::standard_location(
///     "dev", "ukihot", "dirty_way", "save.toml",
/// ));
/// // 保存したい場面で
/// save_requests.write(GutzSaveRequest(current_save_data.clone()));
/// // 起動時に読み込みたい場面で
/// load_requests.write(GutzLoadRequest::default());
/// ```
pub struct GutzSavePlugin<T: GutzSaveData> {
    path: PathBuf,
    _marker: PhantomData<fn() -> T>,
}

impl<T: GutzSaveData> GutzSavePlugin<T> {
    /// 任意の絶対/相対パスを直接指定する。テストや、OS標準の場所を
    /// あえて使いたくない特殊なケース向け。**通常のゲームは
    /// [`Self::standard_location`]を使うこと**——相対パスはカレント
    /// ディレクトリ次第でどこに書かれるか変わってしまい、OSの流儀
    /// （後述）にも従わない。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), _marker: PhantomData }
    }

    /// OS標準のセーブデータ保存先（[`directories::ProjectDirs`]）配下に
    /// `file_name`で保存する。セーブファイルをどこに置くかはOSごとに
    /// 作法が違う——自前で分岐を書かず、確立されたcrateに委ねる：
    ///
    /// - Windows: `%APPDATA%\{organization}\{application}\data\{file_name}`
    /// - macOS: `~/Library/Application Support/{qualifier}.{organization}.{application}/{file_name}`
    /// - Linux: `$XDG_DATA_HOME`（未設定なら`~/.local/share`）
    ///   `/{application}/{file_name}`
    ///
    /// `qualifier`/`organization`/`application`の意味は
    /// [`directories::ProjectDirs::from`]と同じ（例：
    /// `("dev", "ukihot", "dirty_way")`）。ホームディレクトリが
    /// 取得できない等の理由で解決に失敗した場合は、警告ログを出した上で
    /// カレントディレクトリ直下の`file_name`にフォールバックする
    /// （セーブ機能が使えないだけでゲーム自体は落とさない、という
    /// gutzgutz全体のグレースフルデグレード方針に合わせている）。
    pub fn standard_location(
        qualifier: &str,
        organization: &str,
        application: &str,
        file_name: impl AsRef<std::path::Path>,
    ) -> Self {
        let file_name = file_name.as_ref();
        let path = directories::ProjectDirs::from(qualifier, organization, application)
            .map(|dirs| dirs.data_dir().join(file_name))
            .unwrap_or_else(|| {
                bevy::log::warn!(
                    "gutzgutz save: OS標準のセーブ場所を解決できなかったため、カレントディレクトリの{file_name:?}にフォールバックします"
                );
                file_name.to_path_buf()
            });
        Self { path, _marker: PhantomData }
    }
}

impl<T: GutzSaveData> Plugin for GutzSavePlugin<T> {
    fn build(&self, app: &mut App) {
        app.insert_resource(GutzSavePath::<T> { path: self.path.clone(), _marker: PhantomData })
            .add_message::<GutzSaveRequest<T>>()
            .add_message::<GutzLoadRequest<T>>()
            .add_message::<GutzLoaded<T>>()
            .add_message::<GutzLoadFailed<T>>()
            .add_systems(Update, (handle_save_requests::<T>, handle_load_requests::<T>));
    }
}
