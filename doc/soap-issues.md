# ソープ表現 課題管理表

`soap.rs`（GPU常駐パーティクル + メタボールレイマーチ、[soap-model.md](soap-model.md) の実装）を
レビューして洗い出した課題一覧。起点は実機確認で報告された「泡っぽい形にはなるが、毎回同じ形になる」
という症状で、そこから静的コードレビューで見つかった関連課題を合わせて記載する。

**2026-07-28追記**：S-01〜S-08への対症療法（パラメータ調整・ノイズ追加）を検証する過程で、
根本原因は「泡ではなく粘性液体の力学モデルを使っていたこと」と「`soap_compute.wgsl`が
`bubble.rs`のAvian3Dボディと知らずに二重に物理（特に重力）を解いていたこと」の2点にあると
判明した。[soap-model.md](soap-model.md) 第27〜30節でPhase計画ごと設計を改訂し
（Particle=液体の小片 → Particle=Foam Aggregate、Avian3Dをマクロ力学の一次情報源にする）、
本表もそれに合わせて更新している。続く第31節では、命名（GPU Particle Pool →
GPU Foam Instance Pool）・所有権（Entity⇔スロット対応をResourceではなくEntity自身の
Component `FoamGpuBinding` に持たせる）をさらに精緻化し、その過程でS-10（スロット再利用時の
変形状態残留）を発見・修正している。

## 課題一覧

| ID | 優先度 | 分類 | 症状 | 原因（コード上の根拠） | 対策案 | 状態 |
|----|--------|------|------|------------------------|--------|------|
| S-01 | 高 | 見た目/シミュレーション | 泡っぽい形にはなるが、発射するたびにほぼ同じ最終形状に収束する | `soap_compute.wgsl` の `SPREADING` で全粒子が同じ `max_spread = 2.2` に向かって補間される。さらに着弾速度（重力 `14.0`・射出高さ `NOZZLE_HEIGHT = 1.0` からの落下分を含めると、実際の着弾速度はチャージ量に応じて概ね 6.1〜10.4 程度）× `impact_factor = 0.12` で決まる着弾直後の `spread` が、既に 1.73〜2.25 とほぼ上限に張り付いてしまう。結果としてチャージ量（飛距離）による違いが `SPREADING` 開始時点でほぼ失われている | ① `max_spread` を着弾速度やチャージ量に応じて可変にする ② `impact_factor` を下げて扁平化の伸び代を残す ③ 粒子ごとに `max_spread` へも乱数を混ぜる | ✅ 対応済み：`max_spread` を自然な到達範囲（1.73〜2.25）より十分高い `3.4` に変更（`soap.rs` `prepare_soap_frame_data`）。着弾速度差がそのまま最終形状差として残るようにした |
| S-02 | 高 | 見た目/乱数 | 同じ方向へ連射すると、着弾位置や粒子クラスタの配置がほぼ同じに見える | `prepare_soap_frame_data` の `scatter`（±0.6/±0.3/±0.6）は基準速度（水平 6〜16 + 鉛直 3〜9、合成でおよそ 7〜18）に対して 3〜8% 程度しかなく、弾道のばらつきがほとんど出ない。着弾位置側の `offset`（±0.15/±0.05/±0.15）もクラスタ位置のジッタとしては小さい | `scatter` の振幅を速度に対する相対値（例：速度の 15〜25%）にする。または既存の `CHARGE_MAX_JITTER`（チャージが浅いほど狙いがブレる）と同じ思想を GPU 側の散らばりにも反映する | ✅ 対応済み→さらにアーキテクチャ変更で問題自体が消滅：一旦は `scatter` を速度に対する相対値（8%〜43%）に変更したが、Avian駆動化（S-09）で「1発 = 1エンティティ = 1 Foam Aggregate」になったため、そもそも「1発から生成される複数粒子をバラけさせる」という概念自体が不要になった。着弾位置のばらつきは Avian が解く実際の弾道（重力・反発・摩擦）にそのまま委ねられる |
| S-03 | 中 | 見た目/シェーディング | 形が多少違っても影の付き方が単調で、「毎回同じ」という印象を強めている | `soap_render.wgsl` の光源方向・ベースカラー・リムライト係数が全て定数で、シーンの `DirectionalLight`（`scene.rs`）と連動していない。密度カーネルも `particle_density` の `max(0, 1 - d)` という単純な球対称関数のみで、表面ノイズが一切ない | ① ライト方向を uniform として WGSL 側に渡す ② `particle_density`（または最終密度）にノイズ（擬似乱数・simplex 等）を混ぜて表面に凹凸を出す（ユーザーが挙げていた「ノイズを足す」対策そのもの） | 🟡 一部対応：`particle_density` にハッシュベースのバリューノイズを追加し、表面に凹凸を出した（`soap_render.wgsl`）。光源方向の uniform 化は見送り（既存の固定値 `(0.3,0.8,0.4)` がシーンの `DirectionalLight` 方向とほぼ一致しており、優先度が低いと判断） |
| S-04 | 中 | シミュレーション/挙動 | 高速連打すると、画面に残っているはずの古い泡が前触れなく消えたり、別の泡に置き換わったりする可能性がある | Particle Pool（256スロット）はリングカーソル方式。`MAX_LIFETIME = 6.0` 秒が経過する前でも、カーソルが一周すると生存中の粒子スロットを新しい Spawn Request が無条件で上書きする（`soap_compute.wgsl` のスポーン処理に生存チェックがない） | 空きスロット（`state == INACTIVE`）優先の簡易フリーリスト化、もしくはプールサイズを増やす／1発あたりの `amount` を絞る | ✅ 対応済み（S-09と合わせて根本解決）：Avian駆動化により、スロットはリングカーソルではなく`bubble.rs`の`FoamGpuBinding`コンポーネント（Bubbleエンティティ自身が自分のスロット番号を持つ）で管理する。1発ごとに複数粒子を消費しなくなった（S-02参照）ため、512スロットに対して同時消費数が桁違いに少なくなり、上書き自体が構造的に起きなくなった |
| S-05 | 中 | パフォーマンス | 泡が画面に1つもない（未発射）状態でも、毎フレーム画面全域でレイマーチが走る | `queue_soap_metaballs` が発射有無に関わらず毎フレーム無条件で `Transparent3d` アイテムを push しており、フラグメントシェーダーは可視領域内で最大 96 ステップ × 256 粒子のループを常に実行する（AABB によるカリングはあるが、範囲がアリーナとほぼ同じ大きさなのでほぼ素通りする） | アクティブ粒子が 0 件のフレームは `Transparent3d` アイテムを push しない（Main World 側から直近のアクティブ粒子数/発射有無を Render World に伝える） | ✅ 対応済み（さらに精度向上）：当初は`SoapActivity`（経過秒数のヒューリスティック）で対応していたが、Avian駆動化で`ExtractedFoamAggregates`が「今まさに何個のBubbleが生きているか」を正確に把握できるようになったため、タイマーではなくExtract結果が空かどうかで直接判定するように変更（`soap.rs`） |
| S-06 | 低 | 描画/オクルージョン | 液体が本来奥にあるはず（敵の後ろなど）のシーンでも、常に手前に描画される | `TransparentSortingInfo3d::AlwaysOnTop` で参加しており、パイプラインの `depth_stencil` も `depth_write_enabled: false` / `depth_compare: Always` で実質的に深度テストを行っていない | Phase 1 の割り切りとして許容範囲。気になる場合はシーン深度テクスチャを bind してレイの進行を制限する（`soap-model.md` 第24節で当初想定されていた `scene_depth` バインドを追加する） | ⏸️ 見送り：シーン深度テクスチャの bind 追加はパイプライン全体に影響する変更で、リスクの割に本プロトタイプでの見た目への影響が小さいと判断。Phase 4以降、必要になった時点で対応 |
| S-07 | 低 | 保守性 | `SoapSpawnRequest.direction` に正規化前の速度ベクトルそのものを渡し、`pressure` は常に `1.0` 固定で実質未使用になっている | `player.rs` の `fire_bubble` が `direction: velocity, pressure: 1.0` として呼んでいる。フィールド名（方向）と実際に渡している値（速度）の意味がズレている | フィールドを `velocity: Vec3` に統合するか、`direction.normalize()` と `speed` を実際に分離して渡す | ✅ 対応済み：S-02対応と合わせて `direction`/`pressure` を `velocity`/`spread` にリネームし、実際の意味と一致させた |
| S-08 | 低 | 見た目 | 寿命（6秒）を超えた泡が、縮小やフェードなしに突然消える | `soap_compute.wgsl` で `lifetime > MAX_LIFETIME` になった瞬間に `state = STATE_INACTIVE` へ即遷移し、`scene_density` は INACTIVE 粒子を即座にスキップする | `RESTING` 後に `scale` を時間経過で徐々に縮小させてから `INACTIVE` にする、または密度計算にフェード係数を掛ける | ✅ 対応済み：`soap_render.wgsl` の `particle_density` で `lifetime` から毎フレーム計算するフェード係数を密度に掛け、寿命終了1秒前から自然に薄れて消えるようにした（最初 compute 側で `scale` を毎フレーム乗算で縮める実装にしたところ指数的に潰れるバグを自己レビューで発見し、render側の非破壊な計算に変更） |
| S-09 | 高 | アーキテクチャ/潜在バグ | 重力定数が2箇所に独立して存在し（`main.rs` の `Gravity(Vec3::NEG_Y * 14.0)` と `soap.rs` の `SimParams.gravity: 14.0`）、どちらか一方だけ変更すると見た目の弾道とゲームプレイ上の当たり判定がズレる | `soap_compute.wgsl` が独自に重力積分・地面接触判定を行っており、`bubble.rs` が既に持っている本物の Avian3D `RigidBody`（重力・反発・摩擦込み）と知らずに二重に物理を解いていた | Avian3D が解いた `Transform`/`LinearVelocity` を GPU 側がそのまま採用し、自前の重力積分を廃止する（[soap-model.md](soap-model.md) 第27〜28節でアーキテクチャごと改訂） | ✅ 対応済み：`soap_compute.wgsl` から重力積分・速度減衰の自前実装を削除し、`bubble.rs` の Avian ボディが解いた位置・速度を毎フレーム受け取って採用するように変更。GPU側は着弾判定と扁平化（レオロジー）だけを担当する |
| S-10 | 高 | 潜在バグ | リスタート直後に泡を発射すると、着弾もしていないのに最初から扁平に潰れた見た目で飛んでいく可能性がある | S-09対応（Entity⇔スロットの永続対応）実装時に発見。スロットは対応するBubbleエンティティのdespawnと同時に即座に再利用可能になるが、リスタート（`state.rs`の`reset_game`）は生存中のBubbleを寿命を待たずに一斉despawnさせる。再利用されたスロットのGPU側`state`が`STATE_INACTIVE`でない（まだ`SPREADING`/`RESTING`のまま）と、新規スポーン時の初期化（scale/lifetimeのリセット）がスキップされ、前のBubbleの変形済み形状を引き継いでしまう（[soap-model.md](soap-model.md) 第31.4節） | `FoamGpuBinding`/`FoamInstance`/`DriveEntry`に`generation: u32`（単調増加の世代番号）を追加し、GPU側の新規スポーン判定を「`state == INACTIVE` または `generation`が食い違う」に変更する | ✅ 対応済み（実装中に自己レビューで発見・同一パスで修正）：`bubble.rs`の`FoamSlotAllocator`が`allocate()`のたびに新しい世代番号を払い出し、`soap_compute.wgsl`がスロット再利用を確実に検知して初期化し直すようにした |
| S-11a | 高 | パフォーマンス/アーキテクチャ | GPU性能が低い環境で、Foam Aggregate数・Microstructure・Density Fieldの計算量が固定だとフレームレートが成立しない | 現行設計はGPU Computeを前提としており、開発機（RTX 5080）での同時Aggregate数・レイマーチステップ数・Microstructure解像度が、iGPU搭載ノートで過剰になる可能性がある | 静的なQuality Profile（Low/Medium/High）を導入し、同時Aggregate数上限・レイマーチステップ数・Microstructure詳細度をQuality別に分離する。Avianによるマクロ力学（`bubble.rs`）は品質に関わらず共通のままにする | ✅ 対応済み：`quality.rs`に`FoamQuality`（Low/Medium/High）と`FoamQualityProfile`（max_aggregates/raymarch_steps/microstructure_quality）を追加。GPU Instance Poolの物理容量（`FOAM_INSTANCE_POOL_SIZE = 512`）は常に固定のまま、`FoamSlotAllocator::allocate()`が品質別の`max_aggregates`だけで同時数を絞る（Pool再構築なしで品質を切り替えられる）。`soap_render.wgsl`はレイマーチのステップ数とMicrostructureノイズの段階（Simple/Normal/Detailed）を`SoapView`経由の値でランタイム分岐する。暫定UIとして1/2/3キーで即切替可能にし、HUD右下にFPS/Frame Time/Foam数/現在のQualityを表示してベンチマークできるようにした |
| S-11b | 中 | パフォーマンス/アーキテクチャ | S-11aの静的Quality Profileだけでは、実行環境ごとに手動でQualityを選ぶ必要がある | — | Main World側でフレーム時間（EMA）を監視し、ヒステリシス付きで`FoamQualitySetting`を自動的に上げ下げする（Auto Dynamic Quality）。Graceful Degradationの順序（Microstructure→Ray March→Macro Surface→Aggregate数の順に品質を落とす）も設計する | ⏸️ 未着手（意図的に後回し）：S-11aで固定品質のベンチマークが可能になってから、実測（RTX 5080でHighの余裕、一般的なGPUでのMedium、iGPUでのLow成立可否、Aggregate数とRay Marchのどちらが支配的か）を取り、その結果を踏まえて閾値を設計する方針 |

## 備考

- S-01・S-02・S-03 が「毎回同じ形に見える」問題の直接の症状で、S-09（重力の二重管理）がその奥にあった構造的な根本原因だった。パラメータ調整（S-01〜S-03）は症状を緩和したが、S-09の解消（Avian駆動化、[soap-model.md](soap-model.md) 第27〜28節）でモデルの前提そのものが「液体」から「泡の集合体」に変わり、S-02・S-04は問題の前提条件ごと消滅している。
- S-10はS-09の実装（永続的なEntity⇔スロット対応）そのものが持ち込んだ新しい課題で、同じ実装パス内で世代番号（generation）により解決した。「以前は無かった種類のバグ」を設計変更が持ち込みうる、という一例として記録している。
- S-11はS-11a（静的Quality Profile）とS-11b（Auto Dynamic Quality）に分割した。S-11aを先に完成させ実機ベンチマークを取ってからでないと、S-11bのヒステリシス閾値は「当てずっぽう」になってしまうため、意図的にS-11bを後回しにしている。
- S-06とS-11bのみ見送り／後回し。他は全て対応済み。
- 全ての変更は `cargo check` でRust側のコンパイルを確認済み。ただしWGSLの構文・実行時の見た目は未実行で未検証（コンパイル不要という指示だったため）。実際に `cargo run` して見た目を確認することを推奨。
