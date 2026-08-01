# 0037: Windows リリース準備(製品名 reminder・アイコン・release ワークフロー)

**日時**: 2026-08-01

## 依頼と方針

「GitHub で desktop アプリをリリース可能にしてほしい。Windows と Mac 両方」→
監査中に **Windows のみに縮小**する指示。アプリ名は `reminder`、アイコンは
SVG から作成。

Mac を切ったのは正しい判断だった(調査で判明したこと):
- `rust-toolchain.toml` が `stable-x86_64-pc-windows-msvc` を**ホストごと固定**
  しているため、macOS ランナーでは rustup が Windows ツールチェーンを取りに行き失敗する
- `core/Cargo.toml` の `notify-rust` が `[target.'cfg(windows)'.dependencies]`
  配下にあり、`core/src/notify.rs` は非 Windows でコンパイル不能
- つまり Mac 対応は「CI を足す」話ではなく**移植**であり、しかも手元(Windows)
  では一切検証できない

## リリース準備監査の結果(未達6件)

| # | 事象 | 根拠 |
|---|---|---|
| **B1** | **実バグ**: `keyring` が in-memory mock ストアで動作していた | `keyring 3.6.3` は `[features]` に `default` を持たず、`lib.rs:296` が `#[cfg(all(target_os = "windows", not(feature = "windows-native")))] pub use mock as default;`。`windows-native` 未指定だったため `commands.rs:326` の「完全失効時の自動再ログイン用にパスワードを資格情報マネージャーへ保存」が**再起動を跨いで一切機能していなかった** |
| R1 | `productName: "gui"` → 成果物 `gui_0.1.0_x64-setup.exe`、インストール先 `%LOCALAPPDATA%\gui`、ウィンドウタイトルも `gui` | tauri.conf.json |
| R2 | アイコンが Tauri テンプレートの既定ロゴのまま | icons/ |
| R3 | release ワークフロー・タグが**存在しない**(`git tag` 空)。ci.yml のみ | .github/ |
| R4 | `authors = ["you"]` / `description = "A Tauri App"`、`bundle.publisher`/`copyright` 未設定 | gui/src-tauri/Cargo.toml |
| R5 | 既定ウィンドウ 800x600・最小サイズ無し。今回のサイドバー+ダッシュボード構成には狭い | tauri.conf.json |
| R6 | `framework7 ^9.1.1` が package.json に残存(コード参照 0 件、0034 で撤去済み) | gui/package.json |

## 対応

1. **B1**: `core` / `cli` / `gui/src-tauri` の3マニフェストで
   `keyring = { version = "3", features = ["windows-native"] }`。理由を
   コメントで残した(再発すると**無症状**で壊れるため)。Cargo.lock の keyring
   エントリに `byteorder` / `windows-sys 0.60.2` が入ったことで有効化を確認。
2. **R1**: `productName` / `mainBinaryName` / ウィンドウタイトル / `<title>` を
   `reminder` に。`identifier` は変更していないので、データ保存先
   (`%APPDATA%\reminder-proxy-client\data`、`directories` 由来)は不変。
   ただし**インストール先は `%LOCALAPPDATA%\reminder` に変わる**ので、旧 `gui`
   のインストールが残っていれば別アプリとして併存する。
3. **R2**: `gui/src-tauri/app-icon.svg` を新規作成し `npx tauri icon` で
   `.ico` / PNG 一式 / `.icns` を再生成。生成された `icons/android`・`icons/ios`
   は削除(モバイルはスコープ外)。
   - 32x32(タスクトレイ)で潰れないことを最優先し、ベル1個の白シルエット+
     橙バッジのみ。内部ディテールと細線は無し
   - バッジ中心を `(800,224)` に置いた: これはタイル自身の右上角丸
     (`rx=224`)の**円弧中心と一致**する座標
   - バッジには**タイル色のリング**(r=132)を持たせ、ベル側を 0.88 倍+左下に
     ずらして約 134 units(32px 換算で約 4px)の間隙を確保
   - `#tile` グラデーションは `gradientUnits="userSpaceOnUse"`。既定の
     objectBoundingBox だとリングが自身の小さな bbox でサンプルされ、
     タイルと色がずれた輪として見えてしまう
   - **落とし穴**: XML コメント内に連続ハイフンは書けない。CSS 変数名を
     `--blue` と書いたため `tauri icon` が `InvalidComment` で panic した
     (2回踏んだ)。SVG 内のコメントでは変数名を無印で書く
4. **R3**: `.github/workflows/release.yml`。`windows-latest` 単独、
   `v*` タグ push + `workflow_dispatch`(タグ指定)、`tauri-apps/tauri-action`。
   **`releaseDraft: true`** — 成果物を確認してから公開できるようにした。
   `workflow_dispatch` では既定でブランチが checkout されるため
   `ref: ${{ github.event.inputs.tag || github.ref }}` を明示している
   (これが無いとタグとビルド版が食い違う)。
5. **R4/R5**: crate の description/authors/license、`bundle.publisher`
   `copyright` `category` 短長説明、NSIS 言語を Japanese+English、
   既定 1200x820・最小 900x600。
6. **R6**: `npm uninstall framework7`(package.json / package-lock.json 更新)。
   CI の `npm ci` は lock 同期を要求するため、lock の更新まで必須。

## 見送り・申し送り

- **macOS 対応**: 上記の理由で移植扱い。やるなら
  `rust-toolchain.toml` を `channel = "stable"` にし、`notify-rust` の cfg を
  `any(windows, target_os = "macos")` に広げ(lock には既に
  `mac-notification-sys` が入っている)、`keyring` に `apple-native` を足す。
  未署名 .app は Gatekeeper と Keychain ACL で追加の運用注意が必要。
- **コード署名なし**: SmartScreen 警告は避けられない。README と release
  ノートに明記した。証明書購入はユーザー判断。
- **自動アップデータ**: 未導入(0030 の判断を継続)。
- **remote URL**: `origin` は旧名 `reminder-ios-proxy-client.git` のまま。
  GitHub のリダイレクトで push は通っているが、`git remote set-url` は
  権限で実行できなかったため未変更。
