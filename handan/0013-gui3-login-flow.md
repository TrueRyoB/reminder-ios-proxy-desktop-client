# 0013: GUI-3(ログインフロー実装)完了・検証済み

**日時**: 2026-07-26

## 実装内容
- `gui/src-tauri/src/state.rs`: `AuthState`(LoggedOut / AwaitingTwoFactor / Ready)
  + `AppState`(`tokio::sync::Mutex<AuthState>`)。`AppleAuthClient`は
  ログイン/2FA完了までの一時的な期間だけロック内に置き、完了後は
  `Arc<RemindersService>`としてロックなしで共有する設計(計画書通り)。
- `gui/src-tauri/src/commands.rs`: 4つのTauriコマンド
  `get_persisted_apple_id` / `try_resume` / `login` / `submit_two_factor_code`。
  ロジックはCLIの`ensure_login`が使っていた`bootstrap`/`auth`の既存関数をそのまま
  再利用し、標準入力プロンプトの代わりにフロントエンドのSheet Modalとやり取りする。
  ログイン/2FA成功後はCLIと同様にWindows Credential Managerへパスワードを保存
  (`persist_and_store_password`)。
- フロントエンド: `index.html`にログイン用・2FA用の2つの`sheet-modal`を追加、
  `main.ts`で`app.sheet.create()` + フォームの`submit`イベントから
  `@tauri-apps/api/core`の`invoke()`を呼ぶ。起動時に
  `get_persisted_apple_id` → `try_resume` を試し、失敗時のみログインSheetを開く。

## 検証内容
1. `cargo check -p gui` / `cargo build -p gui` / `cargo clippy -p gui --all-targets -- -D warnings`
   — 全て成功(clippy指摘2件を修正: enum variantサイズ差是正のため`AppleAuthClient`を
   `Box`化、`impl Default`を`#[derive(Default)]`に変更)。
2. `npx tsc --noEmit`(gui/) — フロントエンドの型チェック成功。
3. `cargo tauri dev`を実行し、実行中の`gui.exe`プロセスのウィンドウを
   `PrintWindow`/`CopyFromScreen`で直接スクリーンショット撮影して視覚確認。
   結果: 起動時に`try_resume`が実際のApple IDセッション(以前CLIでの検証時に
   永続化されたセッション)を使って**無操作のまま成功**し、画面が
   「ログイン済みです。次のマイルストーンでリスト表示を実装します。」に
   遷移することを確認した。ログインSheetは開かれなかった(想定通り、
   resumeが成功したケース)。これは計画書の受け入れ基準
   「アプリ再起動で無操作のまま try_resume が成功すること」を満たす。
4. `cargo test --workspace --exclude gui` — 既存5テスト(SRP/CRDT)全て通過。

## 意図的にテストしなかったこと(スコープの明示)
新規ログイン(パスワード入力→2FAコード入力)のライブパスは、今回は
**意図的に自動実行しなかった**。理由: ユーザーが「暫くパソコンから離れる」と
明言している間に新規ログインを発火すると、実際のApple IDへ2FAプッシュ通知が
送られ、ユーザー本人の承認が必要になる(信頼済みデバイスでの承認は
自動化できない)。ユーザー不在中に本人の携帯へ通知を送ってしまうのは
不適切と判断した。ただし、このパス自体が呼び出す`AppleAuthClient::login`/
`validate_trusted_device_code`/`trust_session`は、CLI開発時に実サーバーへ
2FA込みで動作確認済みの既存ロジックをそのまま再利用しているため
(Kanban Doneログ参照)、GUI-3で新規に書かれたのはコマンド層の配線と
フロントエンドのSheet UIのみであり、これらは型チェック・コンパイル成功
および実際のtry_resumeパスの成功によって間接的に検証されている。

## 次のタスク
GUI-4: 読み取り専用リスト表示(`list_lists`/`list_reminders`コマンドを
サイドバー・メインリストに接続)。
