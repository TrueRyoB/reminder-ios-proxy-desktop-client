# 0015: GUI-4(読み取り専用リスト表示)完了・検証済み

**日時**: 2026-07-26

## 実装内容
- `core/src/reminders.rs`: `RemindersList`/`Reminder`に`Serialize`/`Deserialize`
  (`camelCase`)を追加し、Tauri IPC経由でフロントエンドへ渡せるようにした。
- `gui/src-tauri/src/state.rs`: `AuthState::reminders()`ヘルパーを追加
  (`Ready`以外の状態からの呼び出しはエラーになるようにする)。
- `gui/src-tauri/src/commands.rs`: `list_lists`/`list_reminders`コマンドを追加。
  `AppState`のロックはネットワーク呼び出し前にdropし、`Arc<RemindersService>`の
  cloneだけを保持して待機時間中ロックを持たない設計。
- フロントエンド: サイドバー用の`panel-left`(リスト一覧、件数バッジ付き)と、
  メインの`media-list`(タイトル・チェックボックス(無効化、読み取り専用)・
  期限・メモ・フラグ🚩)を追加。起動時に最初のリストを自動選択して表示。

## 検証内容
1. `cargo clippy -p gui -p reminder-core -p reminder-proxy-client --all-targets -- -D warnings`
   — 全て通過。
2. `npx tsc --noEmit`(gui/) — 型チェック通過。
3. `cargo test --workspace --exclude gui` — 既存5テスト全て通過。
4. `cargo tauri dev`を実行し、実アカウントの実データに対してリスト一覧・
   リマインダー一覧が正しく表示されることを画面キャプチャで確認
   (⚠️この過程で実データ内の1件にパスワードが記載されており、
   スクリーンショットに写り込んでしまう事故が発生。ユーザーへ報告し、
   「問題ない、パスワード変更の必要性は認知した」との回答を得た。
   詳細: handan/0014。ローカルのスクリーンショットファイルは削除済み)。

## 教訓の追記
実データを画面キャプチャで確認する行為は、書き込みテストと同様に
「実データに触れる操作」に近いリスクがあると判明(個人情報がUIに
表示され得るため)。以後は視覚確認が必要な場合、ダミーデータの
専用テストリストを使う方針とする(handan/0014参照)。

## 次のタスク
GUI-5: スマートリスト(Today/Scheduled/All/Flagged、クライアント側で集計)。
Todayビューは批評を踏まえ降順・スクロール最小化で実装する(計画書参照)。
