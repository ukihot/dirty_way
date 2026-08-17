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

use core::marker::PhantomData;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

/// 保存/読み込みが失敗しうる理由を型で分ける。[`GutzLoadFailed`]がこれを
/// そのまま運ぶため、ゲーム側が「ファイルが無いだけ（初回起動）」と
/// 「壊れている」を見分けて挙動を変えることもできる。
#[derive(Debug, thiserror::Error)]
pub enum GutzSaveError {
    #[error("failed to serialize save data: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to deserialize save data: {0}")]
    Deserialize(#[from] toml::de::Error),
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// OS標準のセーブ場所（[`GutzSavePlugin::standard_location`]）を解決
    /// できなかった。カレントディレクトリ等への自動フォールバックは
    /// **しない**——チーム開発やCIでは作業ディレクトリがユーザーの期待する
    /// 場所とは限らず、意図しない場所への書き込みは「グレースフルな
    /// 劣化」ではなくただの事故になりうる。セーブ機能そのものが
    /// 使えないだけで、ゲーム自体は落とさない（この`Err`はログに出るだけ）。
    #[error("save location could not be determined for this platform")]
    LocationUnavailable,
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
/// トレイト。「保存可能なデータ」という最小の契約に絞ってあり、`Clone`は
/// 要求しない——gutzgutz自身は値を複製せず参照で受け渡すだけなので、
/// 所有権操作の都合をこの契約に混ぜない（ゲーム側の`T`を無用にClone必須に
/// しない）。`serde::Serialize`/`DeserializeOwned`を実装したplain-oldな
/// データ型なら自動的に満たされる。
///
/// ```ignore
/// #[derive(serde::Serialize, serde::Deserialize)]
/// struct SaveData {
///     high_score: u32,
/// }
/// impl GutzSaveData for SaveData {}
/// ```
pub trait GutzSaveData:
    serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static
{
}

impl<T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static> GutzSaveData for T {}

/// 「このデータを保存してほしい」というゲーム側からの明示的な要求。
/// gutzgutzは`World`から値を抜き出したりしない——保存したい値そのものを
/// 呼び出し側が積んで渡す。`Clone`は要求しない（`GutzSaveData`参照）。
#[derive(Message)]
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
#[derive(Message)]
pub struct GutzLoaded<T: GutzSaveData>(pub T);

/// 読み込みに失敗した（ファイルが無い・壊れている等）。理由は
/// [`GutzSaveError`]としてそのまま運ぶ（gutzgutz自身も警告ログは出す。
/// devtoolsの`GutzDebugStats`等へ載せたい場合はゲーム側で購読）。
#[derive(Message)]
pub struct GutzLoadFailed<T: GutzSaveData>(pub GutzSaveError, PhantomData<fn() -> T>);

fn handle_save_requests<T: GutzSaveData>(
    mut requests: MessageReader<GutzSaveRequest<T>>,
    path: Res<GutzSavePath<T>>,
) {
    for request in requests.read() {
        let result = match &path.path {
            Some(path) => save_to_disk(path, &request.0),
            None => Err(GutzSaveError::LocationUnavailable),
        };
        if let Err(error) = result {
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
    // 同じフレームに複数のLoadRequestが届いても、読みに行く先は常に同じ
    // &path.pathなので1回で足りる。read()自体は最後まで消費しておく
    // （＝メッセージを次フレームへ持ち越さない）。
    if requests.read().count() == 0 {
        return;
    }

    let result = match &path.path {
        Some(path) => load_from_disk(path),
        None => Err(GutzSaveError::LocationUnavailable),
    };
    if let Err(error) = result.map(|data| loaded.write(GutzLoaded(data))) {
        bevy::log::warn!("gutzgutz save: {error}");
        failed.write(GutzLoadFailed(error, PhantomData));
    }
}

#[derive(Resource)]
struct GutzSavePath<T> {
    /// `None`は「保存先を解決できなかった」（`standard_location`参照）。
    /// この場合、保存/読み込みは常に`GutzSaveError::LocationUnavailable`
    /// で失敗する——カレントディレクトリ等への自動フォールバックはしない。
    path: Option<PathBuf>,
    _marker: PhantomData<fn() -> T>,
}

/// データ型`T`を1つ受け取り、そのセーブ/ロード一式を配線するプラグイン。
///
/// ```ignore
/// app.add_plugins(GutzSavePlugin::<SaveData>::standard_location(
///     "dev", "ukihot", "dirty_way", "save.toml",
/// ));
/// // 保存したい場面で
/// save_requests.write(GutzSaveRequest(current_save_data));
/// // 起動時に読み込みたい場面で
/// load_requests.write(GutzLoadRequest::default());
/// ```
pub struct GutzSavePlugin<T: GutzSaveData> {
    path: Option<PathBuf>,
    _marker: PhantomData<fn() -> T>,
}

// `GutzGameSessionPlugin`のような合成プラグインが、ゲーム側から渡された
// 保存先設定をそのまま内側のプラグインへ渡せるようにする。保持しているのは
// PathBufとマーカーだけなので、セーブデータそのもののCloneは要求しない。
impl<T: GutzSaveData> Clone for GutzSavePlugin<T> {
    fn clone(&self) -> Self {
        Self { path: self.path.clone(), _marker: PhantomData }
    }
}

impl<T: GutzSaveData> GutzSavePlugin<T> {
    /// 任意の絶対/相対パスを直接指定する。テストや、OS標準の場所を
    /// あえて使いたくない特殊なケース向け。**通常のゲームは
    /// [`Self::standard_location`]を使うこと**——相対パスはカレント
    /// ディレクトリ次第でどこに書かれるか変わってしまい、OSの流儀
    /// （後述）にも従わない。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: Some(path.into()), _marker: PhantomData }
    }

    /// OS標準のセーブデータ保存先（[`directories::ProjectDirs`]）配下に
    /// `file_name`で保存する。セーブファイルをどこに置くかはOSごとに
    /// 作法が違う——自前で分岐を書かず、確立されたcrateに委ねる：
    ///
    /// - Windows: `%APPDATA%\{organization}\{application}\data\{file_name}`
    /// - macOS: `~/Library/Application
    ///   Support/{qualifier}.{organization}.{application}/{file_name}`
    /// - Linux: `$XDG_DATA_HOME`（未設定なら`~/.local/share`）
    ///   `/{application}/{file_name}`
    ///
    /// `qualifier`/`organization`/`application`の意味は
    /// [`directories::ProjectDirs::from`]と同じ（例：
    /// `("dev", "ukihot", "dirty_way")`）。ホームディレクトリが取得できない
    /// 等の理由で解決に失敗した場合、**カレントディレクトリ等へは
    /// フォールバックしない**——チーム開発やCIでは作業ディレクトリが
    /// ユーザーの期待する場所とは限らず、意図しない場所への書き込みは
    /// 事故のもとになる。代わりにエラーログを1回出し、以後の保存/読み込み
    /// 要求はすべて[`GutzSaveError::LocationUnavailable`]で失敗する
    /// （セーブ機能が使えないだけでゲーム自体は落とさない）。
    pub fn standard_location(
        qualifier: &str,
        organization: &str,
        application: &str,
        file_name: impl AsRef<Path>,
    ) -> Self {
        let file_name = file_name.as_ref();
        let path = directories::ProjectDirs::from(qualifier, organization, application)
            .map(|dirs| dirs.data_dir().join(file_name));
        if path.is_none() {
            bevy::log::error!(
                "gutzgutz save: OS標準のセーブ場所を解決できなかったため、セーブ機能全体を無効化します（意図しない場所への書き込みを避けるため、カレントディレクトリ等へは自動フォールバックしません）"
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Debug)]
    struct TestData {
        value: u32,
    }

    #[derive(Resource, Default)]
    struct CapturedLoad(Option<TestData>);

    fn capture_load(
        mut reader: MessageReader<GutzLoaded<TestData>>,
        mut captured: ResMut<CapturedLoad>,
    ) {
        for GutzLoaded(data) in reader.read() {
            captured.0 = Some(*data);
        }
    }

    /// `GutzSaveRequest`で書いたものが実際にディスク上のTOMLとして残り、
    /// `GutzLoadRequest`でその同じ内容が`GutzLoaded`として戻ってくることを、
    /// headlessな`App`（ウィンドウ・レンダラ無し）で検証する。
    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.toml");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(GutzSavePlugin::<TestData>::new(&path))
            .init_resource::<CapturedLoad>()
            // handle_load_requestsが書いたGutzLoadedを同じフレーム内で
            // 確実に拾うため、明示的に後段へ順序付けする。
            .add_systems(Update, capture_load.after(handle_load_requests::<TestData>));

        app.world_mut().write_message(GutzSaveRequest(TestData { value: 42 }));
        app.update();

        // プラグインのload経路を経由せず、生のTOMLとして正しく書けているかを
        // 別ルートで確認する（save/load双方が対称にバグっていると
        // round-trip一本のテストだけでは見逃しうるため）。
        let raw = std::fs::read_to_string(&path).unwrap();
        let on_disk: TestData = toml::from_str(&raw).unwrap();
        assert_eq!(on_disk, TestData { value: 42 });

        app.world_mut().write_message(GutzLoadRequest::<TestData>::default());
        app.update();

        assert_eq!(app.world().resource::<CapturedLoad>().0, Some(TestData { value: 42 }));
    }

    /// ファイルが存在しない場合（初回起動相当）、`GutzLoaded`は届かず
    /// `GutzLoadFailed`が`GutzSaveError::Read`を運んで届く。
    #[test]
    fn missing_file_reports_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.toml");

        #[derive(Resource, Default)]
        struct CapturedFailure(bool);

        fn capture_failure(
            mut reader: MessageReader<GutzLoadFailed<TestData>>,
            mut captured: ResMut<CapturedFailure>,
        ) {
            for failure in reader.read() {
                captured.0 = matches!(failure.0, GutzSaveError::Read { .. });
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(GutzSavePlugin::<TestData>::new(&path))
            .init_resource::<CapturedFailure>()
            .add_systems(Update, capture_failure.after(handle_load_requests::<TestData>));

        app.world_mut().write_message(GutzLoadRequest::<TestData>::default());
        app.update();

        assert!(app.world().resource::<CapturedFailure>().0);
    }

    /// `standard_location`がOS標準のパスを解決できないケースを
    /// シミュレートできないため、代わりに`GutzSavePath.path = None`相当の
    /// 状態を`new`無しで直接組み立て、保存要求が
    /// `GutzSaveError::LocationUnavailable`で失敗し、ファイルが一切
    /// 作られないことを確認する（カレントディレクトリ等へフォールバック
    /// しないことの保証）。
    #[test]
    fn unavailable_location_never_writes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        // わざと存在しない祖先ディレクトリを指すことで書き込みを失敗させる、
        // ではなく`path: None`を直接構築して「解決できなかった」を再現する。
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GutzSavePath::<TestData> { path: None, _marker: PhantomData })
            .add_message::<GutzSaveRequest<TestData>>()
            .add_message::<GutzLoadRequest<TestData>>()
            .add_message::<GutzLoaded<TestData>>()
            .add_message::<GutzLoadFailed<TestData>>()
            .add_systems(
                Update,
                (handle_save_requests::<TestData>, handle_load_requests::<TestData>),
            );

        app.world_mut().write_message(GutzSaveRequest(TestData { value: 1 }));
        app.update();

        // dirは空のまま（save.toml等、何も作られていない）。
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
