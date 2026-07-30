# 🧼 dirty_way

> **泡で埋め尽くせ！ポンプ連射で押し寄せる汚れを迎え撃つ2Dサイドビュー防衛バカゲー**

`dirty_way` は、床の中央に置かれた1台のハンドソープを操作し、左右の端から交互に押し寄せる「マヌケな汚れ大群」をモコモコ物理泡で撃退する 2D サイドビュー防衛ゲームです。

---

## 🌟 ゲームの特徴

- **2Dサイドビューの左右防衛**
  ハンドソープ筐体を真横から見た構図。床（Y=0）の左右の端から迫る敵（油汚れ、ホコリ、泥人間）を、中心のノズルを回転させて狙い撃ちます。
- **本物のポンプ式ノズル操作**
  ボタンを押している間、ノズルが沈み込みながら連続的に泡を噴射し続け、押し込みきる（ポンプストロークを使い切る）と一時的に出なくなります。離すとバネで元の高さに戻り、再び噴射可能に。残量ゲージは無く、ノズル自体の沈み込みがそのまま残量表示を兼ねます。
- **Avian2D駆動の物理泡×GPUリアルタイム液体表現**
  発射した泡はAvian2Dの剛体として物理的に飛び、着地・積み重なりを解きます。その結果をGPU常駐のFoam Instance Pool（コンピュートシェーダー）が受け取り、着弾時の扁平化や泡だまり同士のメタボール融合を毎フレーム計算してぬちゃっと描画します（詳細は[doc/soap-model.md](doc/soap-model.md)）。
- **積み上がる泡だまり**
  飛行中の泡同士はすり抜けますが、着地済みの泡だまりの上には正しく乗って積み上がっていき、山の形がプレイのたびに変化します。
- **安心のグロフリー（配信フレンドリー）**
  流血・欠損表現は一切なし！ポップでシュール、配信で大叫びできる実況映え特化型デザイン。

`README`公開当初に構想していた「360°全方位サークル防衛」「押し込み量で飛距離が変わるチャージショット」「ノズル/スプリング/ソープ液のハクスラビルド」は、プロトタイピングの過程で2Dサイドビュー＋常時ポンプ連射の現行仕様に置き換わりました（経緯は[doc/soap-issues.md](doc/soap-issues.md)参照）。ハクスラ的なビルド要素は現時点のコードには未実装で、将来の拡張候補です。

---

## 🛠️ 技術スタック

- **Engine:** [Bevy (0.19)](https://bevyengine.org/) - Rust製データ指向ゲームエンジン
- **Physics:** [Avian2D](https://github.com/Jondolf/avian) 0.7 - 2D物理演算ライブラリ（ゲームプレイ上の当たり判定・積み重なりを担当）
- **Rendering:** Avian2Dが解いた位置・速度をもとに、GPUコンピュートシェーダー＋カスタムメタボールフラグメントシェーダーでリアルタイムに液体状の泡を描画（[src/soap.rs](src/soap.rs)）
- **共通基盤:** `bevy_gutzgutz`（自社ゲーム開発共通基盤、[bevy_gutzgutz/README.md](bevy_gutzgutz/README.md)参照）
- **Language:** Rust (2024 Edition)
- **Target Platform:** Linux / Windows / macOS

---

## 🚀 開発環境のセットアップ (Ubuntu / Linux)

### 1. 前提条件のインストール

高速リンカ `mold` と `clang` を使用してビルド速度を最適化しています。

```bash
sudo apt update
sudo apt install -y build-essential clang mold pkg-config libx11-dev libasound2-dev libudev-dev
```

### 2. ビルド・実行

```bash
cargo run
```

初回はBevy本体のビルドに数分かかるが、以降は差分ビルドで済む。開発中の高速反復には`dev` feature（`bevy_dylib`経由の動的リンク）が使える。

```bash
cargo run --features dev
```

---

## 📦 リリース

### リリースビルド

```bash
cargo build --release
```

生成物は `target/release/dirty_way`。泡（メッシュ・マテリアル代わりのGPUメタボール）はすべてコード内で生成し、フォントもBevyの`default_font` featureで実行ファイルに埋め込まれているため、追加で必要なアセットは敵キャラクターのスプライトアトラスのみ。`assets/character/`配下のイラストから`build.rs`が生成する`assets/generated/atlas/`（アトラス画像+`manifest.toml`、`GutzAtlasPlugin`が実行時にカレントディレクトリからの相対パスで読む）を実行ファイルと同じ配置で配布物に含める必要がある（Steam連携を有効にしている場合は下記の`libsteam_api.so`も追加で必要）。

`save.toml`/`input.toml`は実行時に自動生成・フォールバックされる（`GutzSavePlugin`/`GutzInputPlugin`）ため、配布物に含める必要はない。

より小さい実行ファイルが欲しい場合は`Cargo.toml`に以下を追加するとよい（未設定・任意）：

```toml
[profile.release]
strip = true
lto = "thin"
```

### Steamworks連携（`steam` feature）

`bevy_gutzgutz`の`GutzSteamPlugin`経由でSteamworks SDKと連携している（詳細は[bevy_gutzgutz/README.md](bevy_gutzgutz/README.md)の`steam`節）。Steam版として実際にリリースする際の手順：

1. **App IDを本番用に切り替える**
   [src/main.rs](src/main.rs)の`GutzSteamPlugin::new(GutzSteamPlugin::DEV_APP_ID)`（480 = SpaceWar、Valve配布の開発・検証用テストID）を、Steamworksパートナーサイトで取得した実際のApp IDに差し替える。

2. **`libsteam_api.so`（再配布ライブラリ）を配置する**
   公式Steamworks SDK（[partner.steamgames.com](https://partner.steamgames.com/)から取得。現在v1.65）の`redistributable_bin/linux64/libsteam_api.so`を、リポジトリ直下の`steam_redist/linux64/`へ配置する。このディレクトリは`.gitignore`済みのため、クリーンチェックアウトのたびに一度だけ手動で行う。[build.rs](build.rs)がビルド時に自動で`target/release/`（等）へコピーし、`$ORIGIN`をrpathへ追加するため、配置さえ済んでいれば`cargo build --release`だけでリンク・実行の両方が解決する（`LD_LIBRARY_PATH`等の手動設定は不要）。

3. **Steamビルドのアップロード（Steam Pipe / ContentBuilder）**
   Steamworksダッシュボードでdepotを作成し、`steamcmd`のContentBuilderで`target/release/dirty_way`と`target/release/libsteam_api.so`を含むビルドをアップロードする。手順の詳細はSteamworksドキュメントの[Uploading Your Content](https://partner.steamgames.com/doc/sdk/uploading)を参照。

`steam_appid.txt`の配置は不要——`GutzSteamPlugin`はApp IDをコード側から`SteamworksPlugin::init_app`へ直接渡しており、内部で`SteamAppId`環境変数を設定する。Steam経由で起動される本番環境・Steamクライアント無しのローカル実行のどちらでも、別ファイルを用意する必要はない。

Steamクライアントが起動していない環境（CI・非Steam版配布）でも、`GutzSteamPlugin`はクラッシュせず`GutzSteamStatus::Unavailable`へグレースフルデグレードする（実機確認済み）。ただし`libsteam_api.so`自体が見つからない場合はOS動的リンカの段階でプロセスが即終了するため、`steam` featureを有効にしたビルドを配布する際は`libsteam_api.so`の同梱を絶対に忘れないこと。

### クロスプラットフォームビルド

`.cargo/config.toml`に以下のエイリアスを用意してある：

```bash
cargo build-linux    # x86_64-unknown-linux-gnu
cargo build-windows  # x86_64-pc-windows-msvc
cargo build-macos    # x86_64-apple-darwin
```

Linux上でWindows/macOS向けにクロスコンパイルするには、対象の`rustup target add <target>`に加え、Windows向けには`rust-lld`（設定済み）、macOS向けにはosxcross等の追加SDKが別途必要（本リポジトリではLinux上での動作のみ検証済みで、実際のクロスコンパイル自体は未検証）。ゲームパッド入力（`bevy_gilrs`）は、依存する`windows`クレートのバージョンがwgpu-hal側と衝突しWindowsビルドを壊すため意図的に無効化している（`Cargo.toml`のコメント参照）。マウス/キーボード操作のみが全プラットフォーム共通の対応範囲。

Windows/macOS向けにSteam連携をビルドする場合は、`steam_redist/`配下に該当プラットフォームの再配布ライブラリ（`win64/steam_api64.dll`・`osx/libsteam_api.dylib`）を追加し、`build.rs`のコピー先ロジックを対象プラットフォーム向けに拡張する必要がある（現状は`linux64`決め打ち）。

### バージョニング

`Cargo.toml`の`version`フィールドを更新し、対応するgit tagを打つ運用を想定（例: `v0.2.0`）。非公開プロトタイプのため、SemVer厳守は必須ではない。