# 0018: GUI-7(並べ替え)完了・検証方針をユーザーと合意

**日時**: 2026-07-26

## 実装内容
- `gui/src-tauri/src/commands.rs`: `reorder_list(list: RemindersList, new_order: Vec<String>)`
  コマンド追加。`RemindersService::reorder`への薄いラッパー(CloudKitの
  楽観的並行性制御に必要な`record_change_tag`のため、リストIDだけでなく
  `RemindersList`構造体全体を受け取る設計)。
- フロントエンド:
  - ナビバーに「編集」/「完了」トグルボタンを追加(スマートリスト選択中は
    非表示 -- 並べ替えは単一リスト前提であり、複数リストを集約するスマート
    リストには「並べ替え先」が存在しないため)。
  - 「編集」タップで`app.sortable.enable(#reminders-list)`、「完了」で`disable`。
  - `sortable:sort`イベント発火時、DOM上の`li[data-reminder-id]`の並び順を
    直接読み取って`reorder_list`を呼ぶ(Framework7のイベントペイロードの
    from/toインデックスを信頼するのではなく、確定済みのDOM順を正とする設計)。
  - 成功後は`cachedLists`を再取得し、同一セッション内で2回目の並べ替えを
    行っても`record_change_tag`が古くて拒否される事態を防止。
  - 編集モード中はタップでの編集Sheetオープンを無効化(ドラッグ操作と競合
    させないため)。

## 検証方針(ユーザーと合意)
並べ替えは**実データの既存の並び順を書き換える操作**であり、このプロジェクトで
過去に事故(handan記録済み、実運用中のリストの並び順を誤って書き換えた)が
起きた操作そのものであるため、GUI-6の許可とは別に改めてユーザーに確認した。
選択肢A(使い捨てテスト項目同士の順番だけ入れ替えて検証)とB(ライブ検証を
スキップし型チェック/clippy/コードレビューのみ)を提示し、**Bを選択**。

理由: `RemindersService::reorder`のバックエンドロジック自体は、このプロジェクトの
CLI開発フェーズで既に実サーバーに対して検証済み(Kanban Doneログ参照)。
GUI-7で新規に書かれたのはTauriコマンドの薄いラッパーとFramework7 Sortableの
フロントエンド配線のみであり、これらは型チェック・clippyで検証済み。

## 検証内容
1. `cargo clippy -p gui --all-targets -- -D warnings` — 通過。
2. `npx tsc --noEmit`(gui/) — 通過(型の絞り込みが`let`変数越しのクロージャで
   効かない問題を、ローカル`const listId`に一度束縛することで解消)。
3. `cargo build -p gui` — 成功。

## 次のタスク
GUI-8: 通知ポーリング + システムトレイ(ウィンドウ非表示でも通知継続)。
