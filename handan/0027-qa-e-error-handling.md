# 0027: QA-E(エラーハンドリング改善)実装完了

**日時**: 2026-07-26

## 実装内容
- `friendlyError(err)`: Rust側から返る生のanyhowエラー文字列
  (例: "CloudKit /records/modify failed (421): ..."、
  "modify failed for 1 record(s): Reminder/XXX: CONFLICT (...)")を、
  既知のパターンに応じて日本語の分かりやすいメッセージへ変換:
  - 未ログイン検知 → 「ログインが必要です。アプリを再起動してください。」
  - 401/421/AUTHENTICATION_FAILED → 「セッションの有効期限が切れました。
    アプリを再起動して再ログインしてください。」
  - CONFLICT → 「この項目は他の場所(iPhoneなど)で更新されていました。
    表示を最新の状態に更新しました。」
  - タイムアウト/接続エラー → 「ネットワークに接続できませんでした。
    しばらくしてからもう一度お試しください。」
  - 未知のパターンは生のメッセージをそのまま表示(隠蔽しない)。
- `isConflictError`/`reportMutationError`: CONFLICT検出時は
  メッセージ表示に加えて自動的に`refreshCurrentView()`を呼び、
  古いデータを表示し続けないようにした。
- 適用範囲: 読み取り系(selectList/selectSmartList/selectDashboard/onReady)
  は`friendlyError`のみ、書き込み系(toggleCompleted/toggleFlag/applyEdit/
  削除/並べ替え/作成)は`reportMutationError`(競合時は自動リフレッシュ付き)。
  ログイン/2FA画面のエラー表示は意図的に対象外とした(生の
  メッセージの方がその文脈では適切であり、「アプリを再起動して
  再ログイン」という誤った案内になりかねないため)。

## 意図的にスコープ外とした点
「常駐中のセッション失効検知」について、完全な自動再認証
(アプリ再起動なしでログインSheetを再表示する等)は実装しなかった。
理由: 現在のbootフロー(`boot()`→`onReady()`)を安全に再入させる設計が
別途必要で、リスクと実装コストに対して「アプリを再起動してください」という
明確な案内メッセージで十分実用的と判断したため。将来的な改善余地として
記録する。

## 検証内容
1. `npx tsc --noEmit`(gui/) — 通過。
2. `cargo build -p gui` — 成功(Rust側の変更なし)。
3. `cargo tauri dev`実行、`Get-Process`で`MainWindowTitle='gui'`かつ
   `Responding: True`を確認。list_listsが引き続き高速(345ms)であることも
   確認(QA-Aの修正が継続して有効)。

実際にCONFLICTエラーやセッション失効を実サーバーで意図的に発生させての
検証は行っていない(意図的な障害注入が必要でリスクを伴うため)。
コードレビューと型チェックによる検証に留める。

## 次のタスク
QA-D(iOSパリティ残課題)またはQA-F(トレイ/通知/ウィンドウ状態)。
