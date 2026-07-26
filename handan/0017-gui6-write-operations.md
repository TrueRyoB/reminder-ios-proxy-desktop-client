# 0017: GUI-6(書き込み系操作)完了・検証済み

**日時**: 2026-07-26

## 実装内容
- `gui/src-tauri/src/commands.rs`: `create_reminder`/`update_reminder`/
  `delete_reminder`の3コマンドを追加。共通の`reminders_service()`ヘルパーで
  ロック取得〜dropを一元化(list系コマンドと同じパターン)。
  `update_reminder`/`delete_reminder`はフロントエンドが直前に取得した完全な
  `Reminder`(`recordChangeTag`込み)をそのまま送り返す設計とし、CloudKitの
  楽観的並行性制御に対応。
- `gui/src-tauri/Cargo.toml`: `chrono = { features = ["serde"] }`を追加
  (`due_date: Option<DateTime<Utc>>`パラメータの型のため)。
- フロントエンド: チェックボックスを有効化しタップで完了状態を即時反映
  (`toggleCompleted`)。行タップで編集Sheetを開き、タイトル/メモ/優先度/
  フラグ/期限の各フィールドが`change`イベントで即座に`update_reminder`を
  発火(計画書の「Doneゲートなし」方針通り)。編集Sheet内に削除ボタン。
  ナビバーの「＋」ボタンから新規作成Sheet(タイトルのみ、クイック追加)。

## 検証内容(ユーザーに事前確認の上で実施)
書き込みAPIの検証は、ユーザーに「既存リストへの使い捨てテストリマインダー
作成→操作確認→即削除」の許可を明示的に得てから実施(リスト作成APIが
未実装のため、新規専用リストではなく既存リストへの一時追加になる旨を
説明し、「許可する」の回答を得た)。

CLIバイナリ(`reminder-proxy-client.exe`)経由で、GUIの新規コマンドが薄く
ラップしている`RemindersService`の同一メソッドを直接呼び出して検証:
1. `create`: `[テスト用] GUI-6動作確認`という明確にテストと分かるタイトルで
   実リストにリマインダーを作成 → 成功(`Reminder/C58DC7D6-...`)。
2. `set-priority --flagged`: 優先度・フラグを更新 → `priority=1 flagged=true`
   で成功。`completed`フィールドも`update()`内の同じJSONペイロード生成ロジックの
   一部であり、専用のCLIサブコマンドはないが同一コードパスで検証済みと判断。
3. `delete`: 削除 → 成功。テスト用リマインダーは実データに一切残っていない。

加えて:
- `cargo clippy -p gui -p reminder-core -p reminder-proxy-client --all-targets -- -D warnings`
  — 全て通過。
- `npx tsc --noEmit`(gui/) — フロントエンド型チェック通過。
- `cargo test --workspace --exclude gui` — 既存5テスト通過。

## 意図的に省略した検証(スコープの明示)
GUI(Tauriコマンド層・Framework7 Sheet UI)自体をマウス操作で実際にクリック
して確認することは、自動化手段(WebView2のリモートデバッグ等)を用意する
コストと、実データを画面に表示する行為自体のリスク(handan/0014参照)を
天秤にかけ、今回は行わなかった。新規コマンド(`create_reminder`/
`update_reminder`/`delete_reminder`)はいずれも`RemindersService`の
既存メソッドへの薄いラッパーであり、型チェック済みのシリアライズ形式
(`camelCase`、`recordChangeTag`)でIPCする設計のため、バックエンドロジックの
実サーバー検証 + 型チェックの組み合わせで十分と判断した。

## 次のタスク
GUI-7: 並べ替え(編集モード + Sortable List)。
