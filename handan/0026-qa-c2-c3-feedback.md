# 0026: QA-C2/C3(操作フィードバック)実装完了

**日時**: 2026-07-26

## 実装内容
- `withLoading<T>(fn)`ヘルパーを追加。Framework7標準の
  `app.preloader.show()/hide()`を使い、非同期処理中はオーバーレイの
  スピナーを表示する。以下の操作にすべて適用:
  - `toggleCompleted`(完了チェック)
  - `toggleFlag`(フラグ切替)
  - `applyEdit`(編集Sheetの各フィールド変更)
  - 並べ替え確定時(`handleReorder`)
  - 新規作成フォーム送信
- 削除操作に確認ダイアログを追加。`app.dialog.confirm()`を使い、
  「「〇〇」を削除しますか?この操作は取り消せません。」と表示してから
  実際に`delete_reminder`を呼ぶよう変更(誤タップでの削除を防止)。
  取り消し(Undo)機能は今回実装しなかった(確認ダイアログで防止する
  設計とし、Undoは過剰と判断)。

## 検証内容
1. `npx tsc --noEmit`(gui/) — 通過。
2. `cargo build -p gui` — 変更なし(Rust側の修正はないため無変更ビルド)。
3. `cargo tauri dev`実行、`Get-Process`で`MainWindowTitle='gui'`かつ
   `Responding: True`を確認(クラッシュしないことのみ)。list_listsが
   引き続き高速(337ms)であることも合わせて確認(QA-Aの修正が
   継続して有効)。

視覚的な内容確認(スピナーの見た目、確認ダイアログの文言表示)は
タスク#21(視覚QA一括実施)へ委ねる。

## 次のタスク
QA-D(iOSパリティ残課題)またはQA-E(エラーハンドリング)。
