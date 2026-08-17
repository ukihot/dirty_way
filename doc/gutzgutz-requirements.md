# gutzgutz（グツグツ）要件定義

## 1. 背景・目的

`dirty_way` は自社ゲーム制作の1作目である。今後何十作も作ることを前提とすると、
毎回ゼロから「FPS表示」「デバッグメニュー」「レイキャストやドラッグ操作」「セーブ」
「オーディオ」「カメラ制御」といった各タイトル共通のコードを書き直すのは非効率であり、
プロジェクト間で品質・実装パターンが分散するリスクもある。

そこで、**`gutzgutz`** を新設し、以下の3層構造を確立する。

```text
Bevy
 ↑
gutzgutz
 ↑
Game（dirty_way, 2作目, 3作目, ...）
```

**呼称について**：本ドキュメントでは`gutzgutz`を「自社フレームワーク」とは呼ばず、
**「自社ゲーム開発共通基盤」**と位置づける。gutzgutzはBevyもAvian3Dも隠蔽せず、
ゲームのアーキテクチャも規定しない（2節）。依存関係としては上図の通り
Bevy→gutzgutz→Gameだが、gutzgutzがGameを支配する構造ではない。実態は
「Bevyの上に、自社ゲーム制作で繰り返し使う知識と実装を蓄積していく層」であり、
"フレームワーク"という言葉が連想させる「規約に従わせる」ニュアンスとは異なる。

ゲーム側から見た利用イメージ：

```rust
app.add_plugins((
    GutzInputPlugin,
    GutzUiPlugin,
    GutzSavePlugin,
    GutzAudioPlugin,
    GutzCameraPlugin,
    GutzSteamPlugin,
    GutzDevtoolsPlugin,
    GutzInteractionPlugin,
));
```

`dirty_way` はこの3層構造における「Gameレイヤーの最初の実装」であり、同時に
gutzgutzの最初の実用テストケースでもある。

## 2. スコープ方針（何を抱え込み、何を抱え込まないか）

**方針：`gutzgutz` は Avian3D／Bevy 本体を丸ごとラップしない。**

物理エンジン（Avian3D）や Bevy 本体のAPIをgutzgutzで大量にラップすると、
「gutzgutzのAPIを覚える」→「結局中身のAvian3D/BevyのAPIも覚える」の二重学習コストが
発生し、かつAvian3D側のバージョンアップに追従するメンテナンスコストだけが増える
不要な抽象化になる。ゲーム側は Avian3D・Bevy を直接使ってよい。

gutzgutzが担うのは、**「複数のゲームで何度も書くことになる、Bevy/Avian3Dの上に載る
"便利機能"・"開発基盤"」** に限定する。

| 分類 | gutzgutzに入れる | gutzgutzに入れない |
|---|---|---|
| 物理 | レイキャストのラッパー、grab/drag、爆発／衝撃系ユーティリティ | `RigidBody`/`Collider`/`Restitution`等Avian3D APIそのもののラップ、共通コリジョンレイヤーの既定値提供（6節） |
| UI | 開発用UI基盤（devtools）、複数作品で使うUI部品（ゲージ、ダイアログ等の**部品**） | 各ゲーム固有のHUD構成そのもの（スコア表示レイアウト等） |
| システム | 入力抽象化、セーブ、オーディオ、カメラ制御、Steam連携 | ゲーム固有のゲームプレイロジック（今回で言えばsoap/bubble/enemy） |

判断基準はシンプルに：**「2作目を作るときにコピペしそうなコードか？」** で切り分ける。
Yesならgutzgutz行き、ゲーム固有のドメインロジック（今回なら泡・敵・チャージ）ならNo。

ただし、**1作目で一度しか使っていないコードを、将来の可能性だけで抽象化しない。**
判断できるのは実際にもう一度書いたときだけなので、抽象化のタイミングは常に事後にする。

```text
1作目で実装 → 2作目でも同じものが必要になる → 「あ、これまた書いた」
→ そこで初めて抽象化してgutzgutzへ
```

7節の「Phase 2以降は2作目以降で初めて共通化の判断材料が揃う」という段階分けは、
この原則をそのままロードマップに反映したもの。早すぎる抽象化を避けることが
gutzgutzの品質を保つ最大の防波堤になる。

## 3. リポジトリ構成（確定）

`gutzgutz` は**別リポジトリ**として切り出し、開発中は**path依存**で参照する。

**理由**：本プロジェクトのゴールは「何十作もの別プロジェクトで使い回す」ことであり、
同一リポジトリ内の workspace（`dirty_way`のサブディレクトリ）にしてしまうと、
2作目以降のリポジトリから参照できない。最初から独立リポジトリにしておく方が、
後から分離するより手戻りが少ない。

開発フローの案：

```text
d:\dev\sandbox\gutzgutz\   ← 独立リポジトリ
d:\dev\sandbox\dirty_way\  ← 独立リポジトリ（今のまま）
```

`dirty_way/Cargo.toml` からは開発中は相対パス依存で参照し、gutzgutz側の変更を
即座にdirty_wayで試せるようにする。

```toml
[dependencies]
gutzgutz = { path = "../gutzgutz", features = ["devtools", "interaction"] }
```

安定してきたバージョン・複数プロジェクトへの展開が始まったタイミングで、
`git` 依存（tagまたはrev固定）に切り替える運用を想定する（プライベートリポジトリ
のため crates.io 公開は不要）。

```toml
gutzgutz = { git = "https://github.com/<org>/gutzgutz", tag = "v0.3.0", features = [...] }
```

*この章はアーキテクチャ上の決定であり、実際にリポジトリを分離する作業は
別タスクとする（本ドキュメントは要件定義のみ）。*

## 4. クレート構成・feature設計

各機能領域を **Cargo feature でopt-in** できるようにする。1ゲームが
Steam連携やセーブ機能を必要としないケースもあるため、`default-features = false`
運用とし、使うプラグインのfeatureだけ有効化する。これによりビルド時間・
依存クレート数を必要最小限に抑える。

ただし機能を単なる横並びの一覧にはしない。タイトル・プレイ・ポーズ・メニュー・
保存をまたぐものは **Session Core** とし、共有する契約を最小限に絞って一方向に
接続する。具体的なゲームState、Action、画面、保存データの意味はゲーム側に
残す。`GutzGameSessionPlugin<S, A, U, D>`は、この標準構成を一行で配線する
合成入口であり、下位プラグインを隠すフレームワークではない。

```text
Session Core: lifecycle ──► input / UI / save
Accelerators: atlas / camera / pacing / interaction
Integrations: devtools / steam
```

```toml
[features]
default = []
devtools    = ["dep:bevy_egui"]   # 開発用オーバーレイ（egui採用は5.2節参照）
interaction = []                   # raycast/grab/explosion等
session     = ["lifecycle", "input", "ui", "save"]
input       = []
ui          = []
save        = ["dep:serde", "dep:ron"]
audio       = []
camera      = []
steam       = ["dep:steamworks"]
```

クレート内部は機能ごとにモジュール分割し、各モジュールが独立した `Plugin` を
公開する（`GutzXxxPlugin` 命名で統一）。

```text
bevy_gutzgutz/
├── src/
│   ├── lib.rs
│   ├── devtools/       (feature = "devtools")
│   │   ├── mod.rs      … GutzDevtoolsPlugin
│   │   ├── fps.rs
│   │   ├── physics_debug.rs
│   │   ├── spawn_entity.rs
│   │   ├── time_scale.rs
│   │   ├── god_mode.rs
│   │   ├── screenshot.rs
│   │   └── stats.rs     … 汎用デバッグ統計チャンネル（4.2節）
│   ├── interaction/     (feature = "interaction")
│   │   ├── mod.rs      … GutzInteractionPlugin
│   │   ├── raycast.rs
│   │   ├── grab_drag.rs
│   │   ├── explosion.rs
│   │   └── impulse.rs
│   ├── input/           (feature = "input")   … GutzInputPlugin
│   ├── session.rs        (feature = "session") … GutzGameSessionPlugin
│   ├── ui/               (feature = "ui")      … GutzUiPlugin
│   ├── save/             (feature = "save")    … GutzSavePlugin
│   ├── audio/            (feature = "audio")   … GutzAudioPlugin
│   ├── camera/           (feature = "camera")  … GutzCameraPlugin
│   └── steam/            (feature = "steam")   … GutzSteamPlugin
```

## 5. Devtools（`GutzDevtoolsPlugin`）

現在 `dirty_way` の [ui.rs](../src/ui.rs) にハードコードされているFPS/フレームタイム
表示は、どのプロジェクトでも欲しくなる機能であり、gutzgutzに切り出す最有力候補。
要求仕様は以下の8機能（ユーザー提示のリストに準拠）。

| 機能 | 内容 | gutzgutz側の責務 | ゲーム側の責務 |
|---|---|---|---|
| FPS | フレームレート表示 | `FrameTimeDiagnosticsPlugin`を内包し、オーバーレイに描画 | なし |
| Physics Debug | 物理形状の可視化 | Avian3Dの `PhysicsDebugPlugin`（既存）をトグルキーで有効/無効化する薄い配線のみ。デバッグ描画自体は再実装しない（2節の方針） | なし |
| Spawn Entity | 任意エンティティをその場で生成 | 「登録された生成関数を一覧して呼び出す」ためのレジストリ機構（`register_spawnable("敵A", closure)`など） | 生成したいプレファブを登録する |
| Time Scale | 時間の流れを可変速に | `Time<Virtual>::set_relative_speed`をスライダー/キーで操作 | なし |
| Skip Level | 現在のレベルを飛ばす | イベント `GutzDevtoolsEvent::SkipLevel` を発行するだけ | イベントを購読してレベル遷移を実行 |
| God Mode | 無敵化 | `Resource<GutzGodMode(bool)>` の保持とトグルUI | ダメージ処理側で `GutzGodMode` を参照して無視する |
| Reload Scene | シーン再読込 | イベント `GutzDevtoolsEvent::ReloadScene` を発行するだけ | イベントを購読して状態リセット（`dirty_way`なら`state.rs`の`reset_game`相当） |
| Screenshot | スクリーンショット保存 | Bevy組み込みの `Screenshot` コンポーネントをキー1つで実行し、タイムスタンプ付きファイル名で保存 | なし |

"Level"や"Scene再構築"はゲームごとに構造が異なるため、gutzgutzはイベントを
発行するだけに留め、実際の処理はゲーム側が購読して実装する **フック方式**を
徹底する（gutzgutzがゲームのシーン構造を知る必要をなくす）。

### 5.1 汎用デバッグ統計チャンネル

`dirty_way`のFPS表示には、FPS/Frame Time以外に "Foam数" "Quality" という
ゲーム固有の値も同じ枠に表示されている（[ui.rs:143-166](../src/ui.rs#L143-L166)）。
これをそのままgutzgutzに持っていくとゲーム固有知識の混入になるため、
**任意のゲームが行に追加できる汎用チャンネル**を用意する。

「毎フレームpushして溜め続ける」設計は、①同じラベルのStringを毎フレーム生成し続ける
無駄と、②古い行が消えず溜まり続ける（クリア漏れ）リスクがある。開発者体験を優先し、
**キーで上書きする`set`API**にする。

```rust
// gutzgutz側
#[derive(Resource, Default)]
pub struct GutzDebugStats {
    values: std::collections::HashMap<&'static str, String>,
}

impl GutzDebugStats {
    pub fn set(&mut self, key: &'static str, value: impl std::fmt::Display) {
        self.values.insert(key, value.to_string());
    }
}

// ゲーム側の毎フレーム更新例
debug_stats.set("Foam", foam_count);
debug_stats.set("Quality", quality.0.label());
```

ゲーム側は「デバッグ情報を登録する」だけで、表示方法（現在はegui、将来Bevy UI／
ImGui／ファイル出力／リモートデバッグに差し替える可能性）はgutzgutz側が決める。
この境界がある限り、表示方式を変えてもゲーム側のコードは変更不要になる。
（内部コンテナはHashMapに限らず、表示順の安定性が必要ならinsertion-order付きの
マップに変更してよい。表示順の扱いは実装時に決める詳細でありAPI契約には含めない。）

devtoolsオーバーレイはFPS/Frame Timeを標準エントリとして自動`set`した上で、
`GutzDebugStats`の内容をあわせて表示する。

### 5.2 UI実装方式：`bevy_egui`採用

devtoolsのUI（トグル可能なメニュー、スライダー、Entity一覧ドロップダウン等）は、
`bevy_ui`（リテインドモード）で組むと開発コストが高い。**本番プレイヤー向けUIと
開発者向けUIを実装方式ごと明確に分離し、devtoolsは`bevy_egui`を採用する。**

```text
ゲーム本番UI   → Bevy UI（既存の ui.rs のスコア/体力/チャージゲージ等はこのまま）
開発者向けUI   → egui（GutzDevtoolsPlugin）
```

FPS/Physics Debug/Time Scale/Spawn Entity/God Mode/Skip Level/Screenshotは
いずれもプレイヤーに見せるUIではなく開発者ツールであり、トグルできるパネル・
スライダー・ドロップダウンのような「頻繁に変わるUI」はimmediate-mode UIの方が
実装・保守コストが圧倒的に低い（`bevy-inspector-egui`等での採用実績もある）。

### 5.3 リリースビルドから完全に切り離せること

`GutzDevtoolsPlugin`（および`bevy_egui`依存）は、`devtools` featureを介して
**Steam向け等のリリースビルドから依存グラフごと消せる**ことを必須要件とする。
God Mode・Skip Level・Spawn Entityのような機能をリリース版に混入させないための
安全策でもある。

```text
開発ビルド                          リリースビルド
Bevy                                Bevy
 ↓                                   ↓
gutzgutz                            gutzgutz
 ├─ Devtools                         ├─ Interaction
 ├─ Interaction                      ├─ Save
 ├─ Save                             ├─ Audio
 ├─ Audio                            └─ Steam
 └─ Steam                            ↓
 ↓                                  Game
Game
```

```toml
# 開発時
gutzgutz = { path = "../gutzgutz", features = ["devtools", "interaction", "save", "audio", "steam"] }

# リリースビルド
gutzgutz = { git = "...", tag = "v0.3.0", default-features = false, features = ["interaction", "save", "audio", "steam"] }
```

ゲーム側の`main.rs`も`#[cfg(feature = "devtools")]`で`GutzDevtoolsPlugin`の
追加を条件付きにする（gutzgutzのfeature未有効時はプラグイン自体が存在しないため、
呼び出し側もコンパイル対象から外れる）。

## 6. Interaction（`GutzInteractionPlugin`）

ゲーム側で何度も書くことになる、Avian3Dの「上に乗る」便利機能群。Avian3DのAPIを
隠蔽せず、その場でAvian3Dの型（`RigidBody`,`Collider`等）をそのまま受け渡しする
薄いユーティリティとして設計する。

| 機能 | 内容 |
|---|---|
| raycast helpers | カーソル位置→ワールドレイの変換（`viewport_to_world`相当の定型処理）＋`SpatialQuery::cast_ray`の結果を扱いやすい型で返す |
| grab / drag | マウスでRigidBodyを掴んで動かす（ばね力/位置合わせでの追従）。ゲームによってはパズル・デバッグ操作双方で使われる定番機能 |
| explosion | 指定座標・半径内の全RigidBodyへ放射状の力積を与えるユーティリティ関数 |
| impulse utilities | Avian3Dの`Forces`（一時的な力積を与えるQueryData）を薄くラップした、方向・大きさ指定だけで撃てるヘルパー関数群 |

**common collision layersはスコープから除外する。** 当初「共通コリジョンレイヤーの
土台」を候補に挙げていたが、レイヤー体系（Player/Enemy/Projectile/...の粒度や
数）はゲームごとに違いすぎ、gutzgutzが既定値を用意してもほぼ全ゲームで上書き・
再定義することになり、デフォルト自体に価値がない。加えて`PhysicsLayer`は
Avian3D自身のAPI（bitset的な型）であり、そこに既定値を与える行為自体が2節で
禁じた「Avian3D APIそのもののラップ」に踏み込みかねない。ゲーム側が
Avian3Dの`PhysicsLayer`を都度直接定義する（現状のdirty_wayと同じ）方針とする。

## 7. その他プラグイン（Phase分け・2026-07-30時点の実装状況）

`GutzInputPlugin` / `GutzUiPlugin` / `GutzSavePlugin` / `GutzAudioPlugin` /
`GutzCameraPlugin` / `GutzSteamPlugin`は、当初はユーザー提示コードにある
将来構想としてPhase分けし、実装を後回しにする計画だった。

**2026-07-30追記**：以下の表は当初のPhase計画であり、`GutzAudioPlugin`を
除いて全て実装済み。特にPhase 3（`GutzCameraPlugin`）は当初「2作目の企画で
dirty_wayと異なる操作方式が出た時点」まで待つ計画だったが、gutzgutzが
今後多数の2Dアーケードゲームを量産する土台になる方針が明確になったため、
1作目（dirty_way）の実装のみを根拠に前倒しで実装した（詳細は
[bevy_gutzgutz/README.md](../bevy_gutzgutz/README.md)の該当節参照）。
`GutzInteractionPlugin`はgutzgutz側は実装済みだが、dirty_wayがまだ
raycast/grab/explosion系の操作を必要としていないため有効化していない
（`dirty_way/Cargo.toml`のコメント参照）——「実装済みだが未使用」は
「未実装」とは別の状態として区別しておく。

| Phase | 内容 | 状況 |
|---|---|---|
| 0 | gutzgutzリポジトリ新設・ワークスペース設計・feature骨組み | ✅ 完了 |
| 1 | `GutzDevtoolsPlugin`（FPS/Physics Debug/Time Scale/Screenshot/God Mode/統計チャンネル） | ✅ 完了・dirty_wayで使用中 |
| 2 | `GutzInteractionPlugin`（raycast/grab/explosion/impulse） | ✅ gutzgutz側は実装済み。dirty_wayはまだ未使用（該当する操作が無いため） |
| 3 | `GutzInputPlugin`（キーバインド抽象化）, `GutzCameraPlugin` | ✅ 完了・dirty_wayで使用中（`GutzCameraPlugin`は前倒し実装、上記追記参照） |
| 4 | `GutzUiPlugin`（共通UI部品）, `GutzSavePlugin`, `GutzAudioPlugin` | `GutzUiPlugin`・`GutzSavePlugin`は✅完了・使用中。`GutzAudioPlugin`は未着手（`audio` featureは空の骨組みのみ） |
| 5 | `GutzSteamPlugin` | ✅ 完了・dirty_wayで使用中（開発中はDEV_APP_IDでグレースフルデグレード確認済み） |

以下は当初のPhase計画時点の記述（歴史的経緯として残す）：Phase 0-1は
"今すぐやる価値がある"部分、Phase 2以降は"2作目以降で初めて共通化の
判断材料が揃う"部分として区別する計画だった。実際にはPhase 1完了後、
2作目を待たずにPhase 2〜5の多くを1作目（dirty_way）の実装だけを根拠に
前倒しで進めることになった。

## 8. `dirty_way`側の対応

**2026-07-29追記：以下はPhase 1着手時点の想定であり、記載内容は既に実装済み。**
`main.rs`は`GutzDevtoolsPlugin`を`.add_plugins(...)`に含んでおり、[ui.rs](../src/ui.rs)は
`update_stats_text`／`StatsText`のようなハードコード実装を持たず、`update_gutz_debug_stats`が
`GutzDebugStats::set("Foam", ...)`/`set("Quality", ...)`でFoam数・現在のQualityをdevtoolsオーバーレイへ
渡すだけになっている（スコア/体力は`dirty_way`固有のHUDとして`ui.rs`に残っている。チャージゲージは
doc/soap-issues.md S-19の仕様変更でノズル自体の沈み込み表現に置き換わり撤去された）。

Phase 1着手時に想定されていた変更（実施済み）：

- [ui.rs](../src/ui.rs) の `update_stats_text`／`StatsText`関連を削除し、
  `GutzDevtoolsPlugin` 導入 + `GutzDebugStats::set` 呼び出しに置き換える。
  スコア/体力/チャージゲージ（本番プレイヤー向けHUD）は`dirty_way`固有のまま
  `ui.rs`に残す。
- `main.rs` に `GutzDevtoolsPlugin` を`.add_plugins(...)`へ追加。
- [quality.rs](../src/quality.rs) のキー切替UI（1〜4キー）は現状ゲーム固有だが、
  「Quality設定を切り替えられるdevtools項目」として一般化できるかはPhase 1完了後に
  再検討する（本要件定義では対象外＝スコープ外と明記しておく。2026-07-29時点でも
  未着手のまま）。

## 9. 非機能要件

- **命名規則**：公開プラグインは`GutzXxxPlugin`、公開Resourceは`GutzXxx`、
  公開イベントは`GutzXxxEvent`で統一する。
- **バージョニング**：非公開の自社ライブラリのため crates.io registry 準拠の
  厳密なSemVerは必須としない。ただし各ゲームリポジトリはgit依存を
  tag/rev固定するため、gutzgutz側の破壊的変更は「固定しているゲームには
  影響しない」ことを前提に自由に行ってよい。
- **ドキュメント**：各`GutzXxxPlugin`は`lib.rs`のdocコメントに「何をするか」
  「何をしないか（＝ゲーム側の責務）」を明記する（2節のスコープ方針を
  実装側でも反映する）。
- **テスト**：物理・UIが絡む都合上、Bevyの`App`を使った統合テスト
  （headlessモードでの`update()`実行）を基本とする。

## 10. 未決定事項

なし。本要件定義の論点はすべて確定した（3節：別リポジトリ＋path依存、5.2節：
`bevy_egui`採用、6節：common collision layers除外）。実際のリポジトリ名・
公開範囲・ディレクトリ配置はPhase 0着手時に決める運用詳細とする。
