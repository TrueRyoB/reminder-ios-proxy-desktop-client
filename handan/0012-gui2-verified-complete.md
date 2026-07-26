# 0012: GUI-2(空のTauriスキャフォールド)完了・検証済み

**日時**: 2026-07-26

## 検証内容
`rust-toolchain.toml`によるMSVCツールチェーン固定(handan/0011)を経て、以下を確認した:

1. `cargo build --workspace`(`+toolchain`指定なし、rust-toolchain.tomlが自動適用) —
   core/cli/gui 全クレートがリンクエラーなくビルド成功。
2. `cargo install tauri-cli --version "^2.0.0"` — 導入済み(`cargo-tauri.exe`)。
3. `cargo tauri dev` を実行 → vite dev server(`localhost:1420`)起動 → `gui.exe`が
   ビルドされ実行される → `Get-Process -Name gui` で有効な `MainWindowHandle`
   (ゼロでない)かつ `Responding: True` を確認。ネイティブウィンドウが実際に開いて
   応答していることをプロセスレベルで直接確認した(報告されたexit codeやログの
   文字列だけに頼らない、これまでの教訓を踏襲)。
4. テスト後、`Stop-Process -Name gui -Force` で明示的に終了。他に残存する
   node/cargo/gui プロセスがないことも確認済み。

## 状態
GUI-2(空のTauriスキャフォールド作成)の受け入れ基準
「`cargo tauri dev` でウィンドウが開くこと」を満たした。タスク完了。

## 次のタスク
GUI-3: ログインフロー実装(Sheet Modal)。`try_resume`/`login`/
`submit_two_factor_code` の3コマンドと、ログイン/2FA用のSheet Modal UIを実装する。
