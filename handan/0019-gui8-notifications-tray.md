# 0019: GUI-8(通知ポーリング + システムトレイ)完了・検証済み

**日時**: 2026-07-26

## 実装内容
- `gui/src-tauri/src/watch.rs`(NEW): CLIの`watch`コマンドを移植。
  `tauri::async_runtime::spawn`でバックグラウンドタスクとして起動し、
  5分間隔で全リストをポーリング、期限到来かつ未通知のリマインダーに
  `notify::send`(既存のWindows Toast通知)を発火。通知が1件以上出た
  ポーリングでは`app.emit("reminders-changed", ())`をフロントエンドへ
  送出(フロントエンド側でのライブ再取得はスコープ外、イベント自体は
  今後利用可能な形で用意)。
- `commands.rs`の`make_ready`: `AppState`に追加した`watcher_started`
  (`AtomicBool`)で二重起動を防止しつつ、`Ready`になった直後に
  `watch::spawn`を呼ぶよう変更。`try_resume`/`login`/`submit_two_factor_code`
  各コマンドに`AppHandle`パラメータを追加(Tauriが自動注入)。
- `lib.rs`: `setup`フックで`TrayIconBuilder`によるトレイアイコンを構築
  (メニュー「終了」+ 左クリックでウィンドウ表示&フォーカス)。
  `on_window_event`で`WindowEvent::CloseRequested`を捕捉し
  `api.prevent_close()` + `window.hide()`に差し替え、ウィンドウを閉じても
  プロセス(=通知ポーリング)が終了しないようにした。
- `capabilities/default.json`: `core:tray:default`/`core:menu:default`を追加
  (実装時に判明: このTauriバージョンではトレイ/メニュー権限は`tray:default`
  ではなく`core:`名前空間配下)。
- `Cargo.toml`: `tauri`に`tray-icon`feature、`tracing`依存を追加。

## 検証内容
1. `cargo clippy -p gui --all-targets -- -D warnings` — 通過
   (途中2件のビルドエラーを修正: ①`tracing`未依存 → 追加、
   ②`CloseRequestApi::prevent_default()`という存在しないメソッド名を
   ベンダーソース確認の上`prevent_close()`に修正)。
2. `cargo build -p gui` — 成功。
3. `cargo tauri dev`を実行し、`Get-Process -Name gui`で有効な
   `MainWindowHandle`/`Responding: True`を確認(トレイアイコン構築処理を
   含む`setup`フックがクラッシュしないことを確認)。
4. **クローズ→トレイ格納の実地検証**: `PostMessage(hwnd, WM_CLOSE, ...)`で
   ウィンドウへ実際にクローズ要求を送信 → 3秒後も同一PIDのプロセスが
   生存し応答していることを確認(`MainWindowTitle`が空になり、
   `MainWindowHandle`が別のハンドルに変化 = 実際に"gui"ウィンドウが
   非表示になったことと整合)。ウィンドウを閉じてもプロセスが終了しない
   という受け入れ基準を満たした。
5. `cargo test --workspace --exclude gui` — 既存5テスト通過。

## 意図的に省略した検証
トレイアイコンの左クリックでウィンドウが再表示される挙動、および右クリック
メニューの「終了」は、実際のマウス操作をこの環境から自動化する手段がなく
(WM_CLOSEのようなメッセージ送信では代替できない、実際のトレイ領域への
クリック座標特定が環境依存で不安定)、ライブでは検証していない。
コードは公式Tauri v2 tray-icon APIのドキュメント通りの実装であり、
`cargo build`が通っている(型・API呼び出しの正しさは保証される)ことで
一定の確からしさとした。

## 次のタスク
GUI-9: ダークモード仕上げ(theme-dark + セマンティックカラー + backdrop-filter)。
