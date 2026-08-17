# gutzgutz（グツグツ）

自社ゲーム開発の共通基盤。1作目（`dirty_way`）の実装から抽出した、複数の
Bevyゲームで繰り返し必要になる「便利機能」「開発基盤」だけを、Cargo feature
単位のプラグインとして提供する。

```text
Bevy
 ↑
gutzgutz
 ↑
Game（dirty_way, 2作目, 3作目, ...）
```

## 位置づけ

gutzgutzは「自社フレームワーク」ではなく、**自社ゲーム開発共通基盤**である。
Bevy/Avian3D本体を隠したりラップしたりしない。ゲームのアーキテクチャも
規定しない。実態は「Bevyの上に、自社ゲーム制作で繰り返し使う知識と実装を
蓄積していく層」であり、ゲーム側は必要に応じてAvian3D・Bevyを直接使ってよい。

**判断基準**：「2作目を作るときにコピペしそうなコードか？」で切り分ける。
Yesならgutzgutz行き、ゲーム固有のドメインロジック（dirty_wayで言えば
泡・敵・チャージ）はゲーム側に残す。1作目で一度しか使っていないコードを、
将来の可能性だけで抽象化することもしない——抽象化は「もう一度同じものを
書いた」瞬間に行う、というのが原則。

**2026-07-30追記**：`camera`/`atlas-sprite2d`の2つだけは、この原則を
意図的に前倒しで上書きした——gutzgutzが今後多数の2Dアーケードゲーム
（ホラー・物理シミュレーション・レース・タワーディフェンス・パズル等）を
量産する土台になる方針が明確になったため、1作目の実装のみを根拠に
先行抽出している（詳細は各featureの節参照）。逆に、同じタイミングで
「まだ抽象化すべきでない」と判断した`dirty_way`側のコード（品質設定の
tier切り替え・泡の積み上げ物理）はそのまま手を付けていない——原則自体を
撤回したわけではなく、個別に判断した2件だけの例外。

## アーキテクチャ：ゲームセッションの共通基盤

gutzgutzは便利機能を横並びで増やすのではなく、ゲームが起動してから終了する
までの「ゲームセッション」を支える薄い共通基盤として育てる。機能は次の3層に
分類する。

```text
Session Core       lifecycle ──► input
                    │             │
                    ├──────────► UI
                    └──────────► save

Production Accelerators  atlas / camera / pacing / interaction
Integrations             devtools / steam
```

- **Session Core** は、タイトル・プレイ・ポーズ・メニュー・保存という共通の
  流れを支える。各プラグインは小さな契約（実行コンテキスト、Pause、UI画面の
  開閉、保存要求と結果）だけを共有し、ゲーム固有の意味は知らない。
- **Production Accelerators** は制作を速くする独立機能であり、Session Coreへ
  依存しない。
- **Integrations** は開発環境・外部サービスとの接続であり、ゲームのルールを
  持ち込まない。

「シナジー」とはプラグイン同士を強結合にすることではない。共有契約を通じて
一方向に組み合わせられ、同じセッションの縦断シナリオで自然に機能することを
指す。ゲーム側はState/Action/UI/SaveDataの意味と実装を持ち続ける。

## リポジトリ構成（現状）

今は`dirty_way`のサブディレクトリとして、path依存（`bevy_gutzgutz = { path
= "bevy_gutzgutz" }`）で参照している。2作目以降が実際に始まったら、独立
リポジトリへ切り出し、`git`依存（tag/rev固定）に切り替える想定。非公開の
自社ライブラリのためcrates.io公開もSemVer厳守も必須ではない——各ゲームが
git依存をtag/rev固定する前提なので、gutzgutz側の破壊的変更は自由に行える。

## 使い方

各機能はCargo featureでopt-inする（`default-features = false`運用）。
使うプラグインのfeatureだけ有効化すれば、依存クレート数・ビルド時間を
最小限に抑えられる。

```toml
[dependencies]
bevy_gutzgutz = { path = "../gutzgutz", features = ["devtools", "lifecycle", "input"] }
```

公開プラグインは`GutzXxxPlugin`、公開Resourceは`GutzXxx`、公開Messageは
`GutzXxxEvent`/`GutzXxxRequest`のように統一する。

通常のゲームは、Session Coreを個別に4つ並べず、`session` featureと
`GutzGameSessionPlugin`を使う。ゲーム側で宣言するのは固有の型と保存先だけで
よい。

```toml
bevy_gutzgutz = { path = "../gutzgutz", default-features = false, features = ["session"] }
```

```rust
app.add_plugins(GutzGameSessionPlugin::<GameState, PlayerAction, UiScreen, SaveData>::
    standard_save_location("dev", "example", "my_game", "save.toml"),
);
```

これは`lifecycle`・`input`・`ui`・`save`を追加する**合成入口**であり、Bevyや
各下位プラグインを隠すフレームワークではない。セーブ不要のミニゲームなどは、
必要な下位featureと`GutzXxxPlugin`だけを直接追加してよい。

## feature一覧

### `session` — `GutzGameSessionPlugin<S, A, U, D>`

ゲームセッションの標準構成。`S`（ゲームState）、`A`（入力Action）、`U`（UI
画面）、`D`（保存データ）をゲーム側が渡すと、`lifecycle`・`input`・`ui`・
`save`を一方向に配線する。各下位プラグインのAPIやゲームのアーキテクチャを
隠蔽しないため、個別利用との混在・置き換えも可能。

### `devtools` — `GutzDevtoolsPlugin`

開発者向けオーバーレイ（egui、F3でトグル）。FPS/Frame Time表示・
Physics Debugトグル（Avian3Dの`PhysicsDebugPlugin`を薄く配線するだけ）・
Time Scale・God Mode・Spawn Entity（登録した生成関数を一覧して呼び出す）・
Skip Level/Reload Sceneフック（`GutzDevtoolsEvent`を発行するだけで、実際の
遷移処理はゲーム側が購読して実装する）・Screenshotを持つ。

任意のゲーム固有デバッグ値は`GutzDebugStats`（`set(key, value)`で上書きする
汎用チャンネル）経由でオーバーレイに載せられる。`devtools` feature自体を
落とせば、`bevy_egui`依存ごとリリースビルドから消せる。

### `interaction` — `GutzInteractionPlugin`

Avian3Dの上に乗る、複数ゲームで繰り返し書く物理インタラクション。
Avian3D自体のAPI（`RigidBody`/`Collider`等）はラップせず、`RigidBody`/
`Forces`のような型をそのまま受け渡す薄いユーティリティのみ：

- raycast helpers（カーソル位置→ワールドレイ、`SpatialQuery::cast_ray`の
  薄いラップ）
- grab / drag（マウスでRigidBodyを掴んで動かす）
- explosion（範囲内の全RigidBodyへ放射状の力積）
- impulse utilities（`Forces` QueryDataの薄いラップ）

共通コリジョンレイヤーの既定値は持たない（ゲームごとにレイヤー体系が違い
すぎ、既定値を用意する価値がないため）。

### `atlas` / `atlas-build` — `GutzAtlasPlugin`

**課題**：`asset_server.load("textures/player_001.png")`のような生パス文字列を
ゲームコードのあちこちに埋め込むと、typoやファイル移動があっても
コンパイルは通ってしまい、そのコードパスが実際に実行されるまで（例えば
めったに出現しない敵の初回スポーン時まで）気づけない。最悪の場合
`cargo run`してキャラクターが生成された瞬間に、テクスチャ抜けやpanicで
初めて発覚する。

もう一つの課題は素材の納品形態。イラストレーター視点では、スプライト
シートやアトラステクスチャのような「GPUが読みやすい形」に組んだ状態で
納品するより、連番PNGのまま渡す方が圧倒的に楽（アニメーションツールの
標準的な書き出し形式でもある）。アトラス化はGPUサンプリング効率のための
機械的な変換であって、制作側の作業ではない。

**設計方針**：`GutzAtlasPlugin`はBevyの`AssetServer`を置き換えない
（Bevy本体の資産ローダーそのものはラップしない）。足すのは以下の2段階だけ：

1. **ビルド時のアトラス生成＋命名規約の強制**（`atlas-build` feature、
   ゲーム側の`build.rs`から呼ぶビルド専用ヘルパー。既存の`steam`
   feature向けdirty_way `build.rs`と同じ「ゲーム側build.rsから
   gutzgutzのビルド専用ロジックを呼ぶ」形）
2. **ビルド時に生成されたマニフェストを介した、名前引きのランタイム
   ルックアップ**（`atlas` feature、`GutzAtlasPlugin`本体）

#### ビルド時：ディレクトリ構造の強制とアトラス化

ソースディレクトリ（例：`assets/character/`）は、キャラ・アニメーション
単位で再帰的にフォルダ分けする。**画像ファイルを直接含むフォルダ
（leaf）だけが1つのtexture（アニメーション1本）を表し、フォルダ名は
`{texture名}_{最大枚数}`でなければならない。** それ以外のフォルダ
（namespace）は自由な名前で何段でもネストできる：

```text
assets/character/
    └── knight/
        ├── idle_10/
        │   ├── 1.png
        │   ├── 2.png
        │   └── ...（10.pngまで）
        └── walk_10/
            ├── 1.png
            └── ...（10.pngまで）
```

最終的なtexture名はnamespaceのパス＋leafのtexture名を`/`で連結した
もの（上の例なら`knight/idle`, `knight/walk`）になる。leaf内のファイル名は
`{番号}.png`のみ（ゼロ埋めしてもしなくてもよい）。

当初はディレクトリを使わず`{texture名}_{番号}_{最大枚数}.png`という
1階層フラットな命名（例：`hoge_1_4.png`）で検討していたが、キャラ数・
アニメーション数が増えると1フォルダに大量の連番PNGが並んで見通しが
悪くなるため、フォルダ単位で分離できる今の形にリファクタリングした。
「フォルダ名自体に`最大枚数`を持たせる」という発想は変わっていない——
これにより次の2種類の納品ミスを検出できる：

- **中落ち**：`1.png`, `2.png`, `4.png`（`3.png`が無い）
- **末尾欠け**：本来`idle_10`のつもりが5枚しか届いていない場合、
  **手元にあるファイルだけを見ると1〜5が連続しているので歯抜け無しに
  見えてしまう**。フォルダ名が「全部で10枚のはず」と自己申告しているため、
  これも検出できる

`build.rs`から呼ぶ`bevy_gutzgutz::atlas_build::pack(src_dir, out_dir)`が
`src_dir`を再帰的に走査し、次のいずれかに該当すれば`Err`を返す
（呼び出し側の`build.rs`はこれを`cargo::error=`として出力し、
`cargo build`/`cargo run`自体を止めることを想定している）：

- 1つのフォルダに画像ファイルとサブフォルダが混在している
  （leafかnamespaceかが曖昧になるため）
- png以外のファイルが混じっている
- leafフォルダの名前が`{texture名}_{最大枚数}`に一致しない
  （`idle`のように`_最大枚数`が無い、等）
- leaf内のファイル名が`{番号}.png`に一致しない（`a.png`のような
  意味不明な番号）
- 検出された番号の集合が`1..=最大枚数`と完全一致しない（中落ち・
  末尾欠け・重複・番号の付け過ぎを全部同じチェックで検出でき、
  欠けている/超過しているファイル名まで具体的に報告する）
- 同じleaf内でフレームごとに画像サイズが不一致（誤った素材の混入を疑う）
- ルート直下に画像を直接置いている（`{texture名}_{最大枚数}/`という
  名前のサブフォルダを作ることを強制する）

`pack`のこれら全パターンの検証ロジックは`bevy_gutzgutz/src/atlas_build.rs`
の`#[cfg(test)]`で（`tempfile`で作った一時ディレクトリ＋合成PNGを使い）
テスト済み。`dirty_way/assets/character/knight/`（実際に納品された
587×707の連番PNG、`idle_10`/`walk_10`各10枚）でも`cargo build`から
実際に生成し、動作確認済み。

各leafは1枚のアトラス画像へ横一列でpackされ、namespaceと同じ
ディレクトリ構造のまま`out_dir`へ出力される（例：
`out_dir/knight/idle.png`）。マニフェスト（`out_dir/manifest.toml`：
texture名 → アトラス画像パス・フレーム数・タイルサイズ）も同時に
出力する。**`out_dir`は`OUT_DIR`（`target/.../build/.../out/`）ではなく、
実行時に`AssetServer`が読める場所（ゲームの`assets/`配下、例：
`assets/generated/atlas/`）を指定すること**——`OUT_DIR`はビルド
スクリプト専用の一時置き場で、`AssetServer`からは見えない。

複数textureをまたいだ1枚の巨大アトラスへの集約（ドローコール削減の
最適化）はv1のスコープ外——2作目以降で実際にドローコールがボトルネックに
なってから検討する（本READMEの一貫した方針：早すぎる最適化はしない）。

**さらに強い保証（検討中）**：ここまでの検査は「納品されたフォルダが
それ自体として整合しているか」までしか見ておらず、「ゲームコードが
実際に期待している枚数と一致しているか」は別問題として残る
（例：コードは4枚前提だが、イラストレーターが自己整合的に6枚
納品してしまったケースはここまでの検査ではエラーにならない）。
これを埋めるには、ゲーム側が「`texture名`Xはフレーム数Nを要求する」と
宣言できる仕組み（マニフェストファイル、または`gutz_atlas_requires!`
的なマクロ）を用意し、突き合わせてズレていればビルドエラーにする。
フォルダ名から`texture名`ごとの`最大枚数`は既に抽出済みなので、
この突き合わせ自体は単純な等値比較で済む。これができれば「コードが
参照しているキャラのテクスチャが実は無かった」を`cargo run`より前に
検出できる（要件の核心）。

`atlas-build`はビルドスクリプトの依存として使われるため、コード上は
Bevy本体をimportしない——PNGのデコード・パッキングに`image`crateだけを
使う（`bevy_gutzgutz/src/atlas_build.rs`に`use bevy::`は無い）。ただし
`bevy_gutzgutz`自体は`bevy`crateを常時（feature非依存で）依存に持つため、
`atlas-build`だけを`[build-dependencies]`として使ってもビルド時間への
影響が完全にゼロにはならない——実測でボトルネックになった場合は
`atlas_build`を別crateへ切り出す選択肢もある（現時点では見送り。
本READMEの一貫した方針：早すぎる最適化はしない）。

#### ランタイム：名前引きレジストリ

```rust
app.add_plugins(GutzAtlasPlugin);

fn spawn_knight(mut commands: Commands, atlases: Res<GutzAtlasRegistry>) {
    // ビルド時に生成されたマニフェストをStartupで読み込み済み。
    // 生パス文字列はゲームコードに一切登場しない。
    if let Some(frame) = atlases.frame("knight/idle", 0) {
        // frame.image: Handle<Image>、frame.uv_rect: Rect（0.0〜1.0正規化）。
        // Sprite+TextureAtlas（2D）でもStandardMaterialのUVオフセット
        // （3Dメッシュ）でも、呼び出し側の描画方式に合わせて使う。
        let _ = (frame.image, frame.uv_rect);
    }
}
```

`GutzAtlasRegistry`はStartupで、ビルド時生成マニフェスト（`manifest.toml`
自体は`std::fs`+`toml`で直接読む。Bevyのアセットパイプラインは経由しない）
と、そこが指すアトラス画像（こちらは通常の`AssetServer::load`）を読み込んで
構築する。`frame(name, index)`は`Option<GutzAtlasFrame>`を返し、
`Sprite`/`TextureAtlas`（2D）を直接組み立てて返すことはしない——dirty_way
のような3Dゲームではメッシュ側のUVオフセットとして使いたいこともあるため。
名前・フレーム番号の指定ミスはパニックではなく`Option`で表現し、コンパイル
エラーにはしない——それを静的に防ぐには別途コード生成
（マクロで`texture名`を型として持たせる等）が要り、v1のスコープ外とする。

**スコープ外**：アトラスパイプラインに乗らない任意画像の動的ロード
（MOD・ユーザーコンテンツ等、`AssetServer`を直接使う）。

**2026-07-30追記**：アニメーション再生（どのフレームを今表示するかの
タイミング制御）は、当初「2作目以降で同じロジックが必要になってから
`GutzSpriteAnimation`的な形で追加を検討する」としていたが、gutzgutzが
今後多数の2Dアーケードゲーム（ホラー・物理シミュレーション・レース・
タワーディフェンス・パズル等）を量産する土台になる方針が明確になった
ため、dirty_wayでの最初の実装（`enemy.rs`の`WalkAnimation`）を機に前倒しで
`atlas-sprite2d` featureとして抽出した。`atlas`本体は描画方式（2D Sprite /
3DメッシュのUVオフセット）を問わない設計のまま維持し、`Sprite`への依存
（`GutzSpriteAnimation`とその駆動システム）だけを独立featureへ分離している
——3Dゲーム等`Sprite`を使わない消費側は`atlas`のみ有効化すれば
`bevy_sprite`への依存は発生しない。

```rust
// GutzSpriteAnimation を Sprite と同じ Entity へ insert するだけで、
// GutzAtlasPlugin（atlas-sprite2d有効時）が毎フレーム Sprite.rect を
// 自動更新する。フレーム数はマニフェスト（GutzAtlasRegistry）から
// 自動で引くため、呼び出し側では持たない。
commands.spawn((
    Sprite { image: frame.image, rect: Some(frame.pixel_rect), ..default() },
    GutzSpriteAnimation::new("knight/walk", 0.1), // 名前, 1フレームあたりの秒数
));
```

### `lifecycle` — `GutzLifeCyclePlugin<S>`

ゲームが起動してから終了するまでの「ライフサイクル」の共通化。**gutzgutzは
具体的なゲームStateを規定しない**——ゲーム固有の`States`型（`Title`/
`Playing`/`Pause`のような3値でも、`Playing`/`GameOver`のような2値でもよい）
はゲーム側が定義し、`GutzLifecycleState`トレイトを実装して各バリアントを
`GutzExecutionContext::InGame`/`OutGame`へ分類することだけを教える。

```rust
impl GutzLifecycleState for GameState {
    fn execution_context(&self) -> GutzExecutionContext {
        match self {
            GameState::Playing => GutzExecutionContext::InGame,
            GameState::GameOver => GutzExecutionContext::OutGame,
        }
    }
}

app.add_plugins(GutzLifeCyclePlugin::<GameState>::default());
```

提供するのは「状態を扱うための仕組み」——

- **State Transition Helpers**：`in_game::<S>()`/`out_game::<S>()`/
  `in_context::<S>(ctx)`という実行条件
- **OnEnter/OnExit Helpers**：`OnEnterContext(ctx)`/`OnExitContext(ctx)`
  スケジュール。Bevy標準の`OnEnter<S>`は厳密に1つのState値にしかフックでき
  ないが、こちらは複数の具体的StateがどれもOutGameに属するような場合でも
  「OutGameになった瞬間」にフックできる
- **Pause/Resume**：`GutzPaused`リソース＋`paused`/`not_paused`実行条件。
  トグルすると`Time<Virtual>`が連動して止まる/再開する
- **Transition Events**：`GutzExecutionContextChanged`（`S`型に依存しない
  汎用メッセージ）
- **State Debugger**：`devtools` feature併用時、現在のState/Context/
  Pausedを`GutzDebugStats`へ自動で載せる

### `input` — `GutzInputPlugin<A, S>`

「Action」と「Device」の分離。`keyboard.pressed(KeyCode::KeyW)`のような
生入力をそのまま薄くラップすることは**しない**——それはBevyのAPIを隠して
いるだけで価値が薄い。核心は3点：

1. プレイヤーが何をしたいか（`GutzAction`）と、何のデバイスで操作したか
   （`GutzInputSource`：Key/MouseButton/GamepadButton）を分離する
2. デバイスではなくActionを抽象化する（ゲーム側は`GutzActionState`経由で
   `pressed(Action::Fire)`のように見る）
3. `lifecycle`によってActionの有効範囲（実行コンテキスト）を制御する
   （`GutzInputMap::restrict_to(action, GutzExecutionContext::InGame)`）

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum PlayerAction { RotateLeft, RotateRight, Charge, Restart }

app.add_plugins(GutzInputPlugin::<PlayerAction, GameState>::default());
```

バインディングはTOML設定ファイルから読み込める（`load_into`/
`load_into_from_file`）。gutzgutzはゲームのAction型の文字列表現を知らない
ため、名前解決は呼び出し側の関数で行う。未知のアクション名・Device名は
警告ログを出してスキップする（typoでゲーム全体が起動不能になるのを防ぐ）。

```toml
[bindings]
rotate_left = ["KeyA"]
restart = ["KeyR", "GamepadStart"]
```

### `ui` — `GutzUiPlugin<T>`

「UIを作る」のではなく「UIのライフサイクル」を作る。HealthBar・Score・
Ammoのようなゲーム固有のHUD部品はここへ持ち込まない——GutzUiPluginの本質は
UIの「見た目」ではなく、**UIを操作可能な状態機械として扱うための基盤**。

「今どのUI画面が開いているか」を`GutzUiStack<T>`（スタック、一番上だけが
アクティブという前提）で管理し、一番上が変わった瞬間に`GutzUiScreenOpened`/
`GutzUiScreenClosed`を発行する。実際にどんな見た目のUIをスポーン/despawn
するかはゲーム側がこれらを購読して決める。

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum UiScreen { GameOver }

app.add_plugins(GutzUiPlugin::<UiScreen>::default());
// OnEnter(GameState::GameOver) で stack.push(UiScreen::GameOver) するだけ。
// 実際のUIスポーンはGutzUiScreenOpened<UiScreen>を購読して行う。
```

`GutzUiStack`とは独立に、Title/Pause/GameOverのようなモーダル画面が
共通して持つ「全画面・中央寄せ・縦積み・背景オーバーレイ」の枠だけを
組み立てる`spawn_modal_panel`も同じfeatureに含む（ゲージ・ダイアログ等の
**部品**はスコープ内、ゲーム固有のHUD構成そのものはスコープ外という
方針のまま。中身の`Text`はゲーム側が`.with_children`で足す）：

```rust
spawn_modal_panel(&mut commands, GutzModalPanelStyle::default())
    .insert(ScreenUi) // ゲーム側のマーカーComponent
    .with_children(|root| {
        root.spawn((Text::new("PAUSED"), ...));
    });
```

### `save` — `GutzSavePlugin<T>`

「セーブファイル」ではなく「ゲーム状態の永続化」。Saveはゲーム状態を
ディスクへ出し入れするための**インフラ**であり、ゲームロジックそのものを
知らない。**セーブデータと実行中のWorldを分離する**——gutzgutzは生きている
`World`から何を保存するか決めたり、読み込んだ値を勝手に`World`へ書き戻し
たりしない。

ゲーム側は保存したいデータを1つのplain-oldな型（`serde`実装）として定義し、
保存したい時に値そのものを`GutzSaveRequest`へ積んで送り、読み込んだ結果は
`GutzLoaded`/`GutzLoadFailed`で受け取って自分のResourceへ反映するかどうかも
含めて自分で決める。フォーマットはTOML。

```rust
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SaveData { high_score: u32 }

app.add_plugins(GutzSavePlugin::<SaveData>::standard_location(
    "dev", "ukihot", "dirty_way", "save.toml",
));

// 保存
save_requests.write(GutzSaveRequest(current_data.clone()));
// 読み込み（結果はGutzLoaded<SaveData>で受け取る）
load_requests.write(GutzLoadRequest::default());
```

**保存先はOSごとに作法が違う**ため、自前で分岐を書かず`directories`crate
（`ProjectDirs`）に委ねる。`standard_location(qualifier, organization,
application, file_name)`が実際に書き込む場所：

| OS | 場所 |
|---|---|
| Windows | `%APPDATA%\{organization}\{application}\data\{file_name}` |
| macOS | `~/Library/Application Support/{qualifier}.{organization}.{application}/{file_name}` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/{application}/{file_name}` |

親ディレクトリが無い（初回起動）場合は保存時に自動で作成する。
`ProjectDirs::from`がホームディレクトリ等を解決できなかった場合は、
警告ログを出してカレントディレクトリの`file_name`にフォールバックする
（セーブが使えないだけでゲーム自体は落とさない、他のfeatureと同じ
グレースフルデグレード方針）。

任意の絶対/相対パスを直接指定したい場合（テスト等）は`new(path)`も
残してあるが、ゲームの実運用では`standard_location`を使うこと——
相対パスはカレントディレクトリ次第で書き込み先が変わってしまう。

### `steam` — `GutzSteamPlugin`

`Client`のResource化・コールバックの毎フレームポンプといったBevy側の
ボイラープレートは、既に`bevy-steamworks`crateがよく解決している
（Bevy 0.19系に追随済み）。gutzgutzがそこを薄く再発明する意味は無いので、
そのまま依存し、gutzgutzは以下の2点だけを足す：

1. Steamクライアント未起動・App ID未登録でもゲームを落とさない
   グレースフルデグレード（`GutzSteamStatus::Connected`/`Unavailable`）
2. devtoolsオーバーレイへの接続状態表示（`GutzDebugStats`経由）

実績・統計・リーダーボード・フレンド・Workshopなど「Steamの何を使うか」は
ゲーム固有なので個別にはラップしない。`bevy_gutzgutz::steam::sdk`
（`bevy-steamworks`の再エクスポート）経由で`Res<sdk::Client>`を直接使う。

```rust
app.add_plugins(GutzSteamPlugin::new(GutzSteamPlugin::DEV_APP_ID)); // 480 = SpaceWar（開発用）
```

`dirty_way`では既にこのfeatureを有効化し、`GutzSteamPlugin::DEV_APP_ID`
（実App ID未登録の開発段階のため）で組み込み済み。

**重要**：`steam` featureを有効にすると、`steamworks-sys`がOS動的リンカ
レベルで`libsteam_api.so`（Win: `steam_api64.dll` / Mac:
`libsteam_api.dylib`）を実行ファイル起動時の必須共有ライブラリとして
要求するようになる。これはRustの`Result`より手前、OSローダーの段階で
起きる失敗のため、`GutzSteamPlugin`内のグレースフルデグレードは一切
関与できず、ファイル不在ならプロセスごと即終了する
（`error while loading shared libraries: libsteam_api.so: ...`）。

このファイルはValveのSteamworks SDK配布物（`lib/steam/
redistributable_bin`配下）に含まれる。`dirty_way`では公式SDK
（partner.steamgames.comから取得したSteamworks SDK v1.65）の
`redistributable_bin/linux64/libsteam_api.so`を配置している。

なお`steamworks-sys`crate自体もビルド時リンク用に同バイナリを同梱して
いるため、公式SDKを取得する前の一時的な動作確認だけなら
Steamworksパートナーアカウント無しでも試せる
（`cargoレジストリキャッシュ内のsteamworks-sys-*/lib/steam/
redistributable_bin/linux64/libsteam_api.so`を同じ場所へコピーすれば
よい）。ただしバージョンが古い可能性があるため、実際の開発・リリースでは
公式SDKの配布物に揃えること。

`dirty_way/build.rs`が`steam_redist/linux64/libsteam_api.so`を検出すると、
実際の出力ディレクトリ（`target/debug/`等）へ自動コピーし、`$ORIGIN`を
rpathに追加する。これにより素の`cargo run`だけで動く
（`LD_LIBRARY_PATH`の手動設定は不要）。`steam_redist/`はgit管理外
（`.gitignore`参照）。ファイル未配置でも`cargo build`自体は通り、実行時に
起動不能になるだけなので、CI等Steamと無縁な環境でのビルドは壊れない。

Steamクライアント自体が起動していない・ログインしていない場合は
（このファイルさえあれば）`SteamAPI_Init`が正常に失敗を返し、
`GutzSteamStatus::Unavailable`へグレースフルデグレードする——これは
実機で確認済み。実際のリリース（App ID登録・ストアページ設定・実績等の
Steamworksダッシュボード操作）にはSteamworksパートナーアカウントが
別途必要だが、それはこのfeatureを有効化する条件ではない。

### `pacing` — `GutzRampTimer`

一定間隔で発火する周期タイマーで、経過時間に応じて間隔自体が線形に
ランプ（変化）していく。ホラー/タワーディフェンス/パズルのような、
波状に敵・イベントを発生させるゲームで繰り返し必要になる想定。
`Resource`/`Component`/`Plugin`は持たない——`bevy::time::Timer`と同じ
「素材」の位置づけで、ゲーム側が自分のResource/Componentに埋め込んで使う。

```rust
#[derive(Resource)]
struct EnemySpawnTimer(GutzRampTimer);

impl Default for EnemySpawnTimer {
    fn default() -> Self {
        // 開始間隔1.6秒 → 下限0.45秒まで、1秒あたり0.01ずつ短縮。
        Self(GutzRampTimer::new(1.6, 0.45, 0.01))
    }
}

fn spawn_enemies(time: Res<Time>, mut timer: ResMut<EnemySpawnTimer>, /* ... */) {
    if !timer.0.tick(time.delta()) { return; }
    // 発火した——ここで実際のスポーン処理。
}
```

`ramp_per_sec`に`0.0`（`start == min`）を渡せば、ランプなしの固定間隔
クールダウンとしても使える。dirty_wayの`enemy.rs`（ランプあり）と
`player.rs`のノズル連射クールダウン（ランプなし）で、同じ「周期的に
再アームされるタイマー」パターンが独立に2箇所書かれていたのを機に抽出した。

### `camera` — `GutzCameraPlugin` / `spawn_fit_camera_2d`

ワールド空間の矩形領域を少なくとも画面に収める2Dカメラのセットアップ。
`Camera2d`/`OrthographicProjection`自体はラップしない——「最低でもこの
ワールド幅×高さを画面に収める（`ScalingMode::AutoMin`）」という、2Dの
アリーナ・トラック・盤面を持つゲームで繰り返し必要になるボイラープレート
だけを引き受ける。`GutzCameraPlugin`自体は骨組みのまま（何もしない）で、
実体は自由関数`spawn_fit_camera_2d`。

```rust
let camera = spawn_fit_camera_2d(&mut commands, GutzCameraFit2d {
    min_width: 36.8,
    min_height: 24.0,
    offset: Vec2::new(0.0, 5.6),
});
// ゲーム固有の追加設定（描画パイプラインの都合によるMsaa::Off等）は
// 呼び出し側が別途 .insert する。
commands.entity(camera).insert(Msaa::Off);
```

**2026-07-30追記**：当初「dirty_wayと異なるカメラ制御を要求する2作目が
出てから中身を詰める」としていたが、`atlas-sprite2d`と同じ理由
（複数ジャンルのゲームを量産する前提が明確になったこと）で、dirty_wayの
`scene.rs`にあったカメラセットアップを機に前倒しで実装した。

### `audio` — 未着手

現時点では`dirty_way`にまだ該当する共通化対象コードが存在しないため、
「何もしないが`app.add_plugins(...)`で差し込める」骨組みだけを用意して
ある。実際にサウンドが必要なゲームが出てから中身を詰める。

## 非機能要件

- **テスト**：物理・UIが絡む都合上、Bevyの`App`を使った統合テスト
  （headlessモードでの`update()`実行）を基本とする。
- **ドキュメント**：各`GutzXxxPlugin`は`lib.rs`/モジュール冒頭のdocコメントに
  「何をするか」「何をしないか（＝ゲーム側の責務）」を明記する。
