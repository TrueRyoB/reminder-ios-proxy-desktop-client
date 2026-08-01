# 0033: Dashboard v2(問い起点の再設計)を実装

**日時**: 2026-08-01

## 経緯
設計3層(design/idea/user-interaction.md → expression.md → dashboard.md)を
artist 規律(Gate 0/1/2、design/artist/dashboard.md)で確定させたのち、
Gate 2 プロトタイプ(design/draft/dashboard-prototype.html)の承認と
差し戻し3件の反映を経て、ユーザーから実装フェーズ入りの指示を受けた。

## 実装内容

### Rust(core)
- `core/src/proxy_store.rs` 新設: proxy ローカル語彙のストア。
  - `ProxyMeta`(cls=signal/habit, group=儀式, purpose, parent, env)
  - `notified`(通知済み集合の永続化 — 再起動での全件再通知を解消)
  - `last_meta_reminder`(週次メタリマインドの時刻)
  - 2書き手(コマンド/ポーラー)の load-modify-save を `with_store` +
    プロセス内 Mutex で原子化。
  - `backup_to_documents`: 起動時に `Documents\reminder-proxy-client\` へ
    世代バックアップ(最新5世代、expression §1 の耐久性確定に対応)。
- `Reminder` に `created`(CloudKit CreationDate、読み取りのみ)を追加 —
  編成スコアのエイジング(放置日数での浮上)の材料。
- `RemindersService::create` に `all_day` パラメータを追加(従来は 0 固定)。
  写像「終日=締切(鳴らない)/時刻付き=発火(鳴る)」の投函側。CLI の
  呼び出しも追随。

### Rust(gui/src-tauri)
- `watch.rs` を「軸を読む通知」に書き換え:
  - **終日カードをスキップ**(締切は鳴らさない — 実測済みの iOS 挙動と対に)
  - 通知済み集合を proxy_store に永続化
  - 発火済み時点カード(cls=signal)を**自動完了**(衝突C: 削除でなく完了)
  - **週次メタリマインド**「締切不明の課題がN件」(漏れない保証・時間側)
  - `lists()` の全履歴リプレイをやめ `lists_cached`(QA-A のキャッシュ)に変更
- コマンド追加: `get_proxy_store` / `set_proxy_meta`。`create_reminder` に
  `all_day` 追加。lib.rs で起動時バックアップを実行。

### フロントエンド(gui/index.html + src/main.ts + styles.css)
- **サイドバー**: スマートリスト4種を全廃(Gate 1 D1)。ダッシュボード+
  マイリスト+グローバル検索(課題ありフィルタ、D2)。
- **ダッシュボード=モード付き単一面**(壁の禁止):
  - 待機: 残高ヘッダ(締切不明N・今週締切N、数字のみ)+締切支配ゾーン
    (期限切れ→着手中→今日)+**次の一手**(一等地)+セッション宣言
  - 編成: 皿の自動仮組み(義務全部+**締切不明を必ず1枚**+スコア順に容量まで)、
    理由バッジ(⏳逼迫/👔同環境/🔗同系統/🌱いつでも/⬆浮上)、DnD+クリックで
    出し入れ、容量ゲージは定性ラベルのみ(分数は表示しない — Gate 2 #2)、
    ☕息抜き挿入提案
  - 実行: 現在カード(🚩自動点灯)、完了/スキップ/中断
  - 完走: 🎉 おしまい/もう一皿
- **クイック投函3入口**(D3、アニメなしインライン行):
  ＋やること(鳴らない、任意で終日締切)/＋リマインド(その時刻に鳴る)/
  ＋イベント(行事カード+時点カードN枚を一括生成、儀式グループでローカル紐付け)。
  作成シートは廃止。完了時の後続キャプチャはクイック投函行の自動オープンに。
- **編集シート拡張**(全射の受け皿): 所要時間(大/中/小 = priority 転用)、
  区分(通常/時報/習慣)、目的、環境、分解(子カード生成+目的継承+親子ローカル1段)。
- **具体リスト=唯一の生ビュー**(時点・習慣カードが見える)。並び替え編集
  モードの入口を復活(D5 — 従来コメントアウトで到達不能だった)。
- ビュー切替時のスケルトン表示(遷移直後の無反応の解消)。
- ポーラーの `reminders-changed` イベントを**フロントで購読**(従来 emit
  されるだけで誰も聞いていなかった)。

## 検証内容
1. `npx tsc --noEmit`(gui/) — 通過。
2. `cargo clippy --workspace --all-targets -- -D warnings` — 通過。
3. `cargo test --workspace --exclude gui` — 既存テスト通過。
4. `vite build` — 通過(チャンクサイズ警告は Framework7 由来で従来どおり)。
5. `cargo build -p gui` → 実行 → `Get-Process` で `Responding: True` を確認。
   注意: dist 更新後に cargo が再コンパイルをスキップし古いフロントが
   埋め込まれたままになる事象を確認(lib.rs を touch して強制再ビルドで解消)。

## 意図的に見送ったもの(設計文書に根拠)
- 5分未満の精密通知タイマ(watch は5分ポーリングのまま。「10分前」が最大
  5分遅れる) — expression §3 の要求として残存。
- 習慣(なし×N)のローカル周期通知 — 区分の付与までは可能、発火は未実装。
- skip の長期学習(セッション内の除外のみ)/タグによるバッチング提示の高度化。
- Google カレンダー同期(可処分時間の入力ポートとして分離済み)。
- リスト自体の CRUD(handan/0031 の見送りを維持)。

## 次のアクション
実機での視覚 QA(ユーザー立ち会い)。特に: 宣言→仮組み→実行→完走の一周、
イベント投函→iPhone 側での時点カードの見え方と通知、終日締切が鳴らないこと
(「今日の通知」設定依存 — user-interaction §5)、時点カード発火→自動完了の掃除。
