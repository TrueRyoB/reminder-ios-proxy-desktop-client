# 0002: GUI-1 ワークスペース分割 完了

**日時**: 2026-07-26
**タスク**: GUI-1 (Cargo workspace化)

## 決定・実施内容
- 単一バイナリクレートを `core`(reminder-core, lib) / `cli`(reminder-proxy-client, bin) の
  2メンバーからなる Cargo workspace に分割。`gui/src-tauri` はGUI-2で追加予定のため
  現時点ではworkspace membersに含めていない。
- `core/src/lib.rs` を新設、全モジュールを `pub mod` で公開。
- `PersistedAuthState` に `apple_id: Option<String>` を追加(`auth::AppleAuthClient::persisted_state()`
  でセット)。GUIが起動時に無入力でセッション再開するために必要。
- `core/src/bootstrap.rs` を新設し、CLI/GUI共通のプロンプトなしロジック
  (`KEYRING_SERVICE`, `reminders_service_root`, `persist_state`, `try_resume_session`)を集約。
  対話的な分岐(いつパスワード/2FAを聞くか)はCLI側に残した。

## 遭遇した問題と対処
- **clapのバグ発覚**: `apple_id` フィールドが `global = true` かつ実質必須(`String`型)だったため、
  workspace化に伴うCargo.lockの再解決で `clap` のバージョンが変わり、
  「Global arguments cannot be required」という debug_assert に引っかかりビルド時にpanic。
  → `apple_id` を `Option<String>` に変更し、`TestNotify` 以外のコマンド実行時に
  手動で必須チェックするよう修正。

## 検証結果
- `cargo build --workspace` / `cargo test --workspace`(SRP/CRDTベクタ5件)/
  `cargo clippy --workspace --all-targets -- -D warnings` 全て通過。
- `cargo run -p reminder-proxy-client -- --apple-id ... lists` で実データ取得を確認、
  分割前と同じ出力(実データも意図せず変更されていないことを確認)。
