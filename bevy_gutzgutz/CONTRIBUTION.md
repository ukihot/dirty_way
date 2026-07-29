# gutzgutz 開発原則

このドキュメントは「何を作るか」（README.md）ではなく「どう書くか」を
まとめたもの。gutzgutzは複数のゲームに長期間使い回すコードなので、1機能
実装するたびに書き方がばらつくと、すぐにスパゲッティ化する。特に
エラーハンドリングは放っておくと真っ先に荒れる部分——正常系とエラー処理を
`match`や`if let`で無秩序に絡めると、可読性も保守性も一気に落ちる。

## 1. `Result`を中心に正常系を直線化する

Rustでは、fallibleな処理は`Result`の`?`演算子で縦につなぐのが一番読みやすい。
「正常に進んだ場合に何が起きるか」が上から下へそのまま読めることを目指す。

**避けるもの**：

- 複数の`match`/`if let`をネストさせて、成功パスとエラーパスを行ったり
  来たりするコード
- `Ok`の中でさらに別の`Result`を`match`する（＝ネストの深さが処理の数だけ
  増えていく）

**やること**：

- 個々の失敗しうるステップは`?`でつなぐ
- 早期リターンが必要な箇所は`let ... else { return ... };`（let-else）を使う
- `.map_err(...)`で型を揃えてから`?`する。同じ変換を何度も書くなら
  `#[from]`（後述）でthiserrorに任せる

### Before / After（save.rsより）

Before（実際にあった形。正常系とエラー処理が`match`の中に埋まっている）：

```rust
fn handle_save_requests<T: GutzSaveData>(
    mut requests: MessageReader<GutzSaveRequest<T>>,
    path: Res<GutzSavePath<T>>,
) {
    for request in requests.read() {
        match toml::to_string_pretty(&request.0) {
            Ok(contents) => {
                if let Err(error) = std::fs::write(&path.path, contents) {
                    bevy::log::warn!("failed to write {:?}: {error}", path.path);
                }
            }
            Err(error) => bevy::log::warn!("failed to serialize: {error}"),
        }
    }
}
```

After（「1件保存する」を独立した`Result`関数に切り出し、システムは
その結果をログに出すだけにする）：

```rust
fn save_to_disk<T: GutzSaveData>(path: &Path, data: &T) -> Result<(), GutzSaveError> {
    let contents = toml::to_string_pretty(data)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| GutzSaveError::CreateDir { path: parent.to_path_buf(), source })?;
    }
    std::fs::write(path, contents)
        .map_err(|source| GutzSaveError::Write { path: path.to_path_buf(), source })
}

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
```

`save_to_disk`は上から下まで一直線（`?`が3回）で読める。Bevyの
`System`/`MessageReader`が絡む「オーケストレーション」の部分と、実際の
I/O手順（「正常系のロジック」）が完全に分離されている。

## 2. 型付きエラー：`GutzXxxError` + `thiserror`

公開APIの境界（`pack`、`GutzSavePlugin`の保存/読み込み等）で複数の異なる
失敗理由がありうる場合、`String`や`Box<dyn Error>`で誤魔化さず、
`thiserror`で列挙型として定義する。

- 名前は`Gutz{モジュール}Error`（例：`GutzSaveError`、
  `GutzAtlasBuildError`）
- 各バリアントは`#[error("...")]`でメッセージを持たせる。
  「何が」「どこで」失敗したかを、呼び出し側が追加で文脈を継ぎ足さなくても
  伝わる文面にする（`path`や`texture_name`のような識別情報をフィールドに
  持たせて`{path}`のように埋め込む）
- 下位のエラーは`#[source]`（構造体バリアント）または`#[from]`
  （タプルバリアント、変換元が1種類しかない場合）で包む。`std::io::Error`の
  ように複数箇所で発生し、それぞれ違う文脈を添えたい場合は構造体バリアント
  ＋`#[source]`にする（`#[from]`にすると文脈情報を持てない）

```rust
#[derive(Debug, thiserror::Error)]
pub enum GutzSaveError {
    #[error("failed to serialize save data: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to write {path}: {source}")]
    Write { path: PathBuf, #[source] source: std::io::Error },
    // ...
}
```

### なぜ`String`エラーをやめるか

`atlas_build.rs`は元々全ての失敗を`Result<T, String>`＋`format!()`で
表現していた。動くには動くが、

- 呼び出し側が失敗理由の**種類**で分岐できない（文字列を`contains`で
  照合するしかない——実際、最初に書いたテストコードがまさにこれをやって
  いた）
- どんな失敗パターンがあるか、シグネチャを見ただけでは分からない
  （関数の中身を全部読まないと失敗の全体像が掴めない）
- エラーメッセージの文面が呼び出し側のコード中に散らばり、同じような
  `format!("{}: ...", path.display())`が何十箇所にも重複する

型付きエラーにすると、失敗パターンの一覧がenumの定義として1箇所に集まり、
`match`で網羅チェックもできるようになる（実際に使うかどうかは別として、
「今どんな失敗がありうるか」が型から分かることに価値がある）。

### いつ`String`のままでよいか（過剰適用への戒め）

**全ての`Result`を`GutzXxxError`にする必要はない。** `input/config.rs`の
`load_into`は`toml::de::Error`をそのまま返しているし、`load_into_from_file`
はそれを`std::io::Error`（`ErrorKind::InvalidData`）に包んでいるだけで、
専用のenumは作っていない——失敗パターンが「パースに失敗した」の1種類しか
なく、外部crateの型（`toml::de::Error`）がそのまま呼び出し側にとって
十分な情報を持っているから。同様に`steam.rs`の
`match sdk::SteamworksPlugin::init_app(...)`も、`Ok`/`Err`の2値だけを
その場で処理して終わりなので、わざわざ型を割ることに意味がない。

判断基準：**呼び出し側（またはログを読む人）が「どのケースで失敗したか」を
区別する必要があり、かつその区別を型で表現するだけの複雑さがあるか**。
1〜2種類の失敗しかない・その場で握りつぶして終わりなら、素朴な
`Result<T, ExternalError>`や`match`で十分。無理に`GutzXxxError`を
定義すると、今度は「定義だけあって誰も分岐に使わないenum」という別種の
ノイズになる。

## 3. 「オーケストレーション」と「正常系のロジック」を関数で分ける

Bevyの`System`（`Res`/`ResMut`/`MessageReader`等を引数に取る関数）は、
「いつ・何をトリガーに呼ばれるか」というオーケストレーションの層。
実際の計算やI/O（何をどう保存するか、ディレクトリをどう検証するか）は
別の、Bevyに依存しない純粋な関数へ切り出す。

利点：

- 純粋な関数は`#[test]`で直接呼べる（`System`のまま`App`を組み立てて
  テストするより、はるかに書きやすい・速い）。`atlas_build.rs`の
  10個のテストは全部この形——`pack()`はBevyに一切依存しないので、
  `tempfile`で作った一時ディレクトリに対してただ呼ぶだけで検証できる
- `?`で正常系を一直線に書ける（`System`の中で`?`を使うには戻り値を
  `Result`にする必要があり、Bevyのエラーハンドリング機構と絡んで
  かえって複雑になりがち）
- 「これは仕組み（システムの配線）」と「これは中身（ロジック）」が
  ファイル内で視覚的にも分離される

具体例：`atlas_build.rs`の`pack`は`collect_groups`→`pack_one`→
`write_manifest`という3段の純粋関数に分かれており、`pack_one`はさらに
`index_files`→`check_index_set`→`load_frames`→`compose_atlas`→
`save_atlas_image`に分かれている。1つの関数が背負う責務を「これは何を
検証する/生成する関数か」と一言で言えるサイズに保つ。

## 4. 早すぎる抽象化はしない（README.mdの原則の再掲）

上記のエラーハンドリング規約は「複雑な失敗パターンを持つ、公開APIの
境界」に適用するものであり、全てのコードに機械的に適用するルールでは
ない。`devtools`/`interaction`配下の多くの関数はそもそも`Result`を
返す必要すらない（`Option`で十分、あるいは失敗しようがない）。
「本当に必要になった場所だけ整理する」——これはREADME.mdの
「1作目で一度しか使っていないコードを、将来の可能性だけで抽象化しない」
という原則と同じ発想である。
