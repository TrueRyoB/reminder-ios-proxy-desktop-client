# 0004: GUI-2 が環境要因でブロック — MSVC/GNUどちらも即座には動かない

**日時**: 2026-07-26
**タスク**: GUI-2 (空のTauriスキャフォールド作成)

## 状況
Tauriスキャフォールド自体は作成済み(`gui/`, workspace membersにも追加済み、
`reminder-core`依存も配線済み)。`cargo check --workspace` は問題なく通る
(リンク不要な段階までは全て正常)。しかし最終的なリンクが両toolchainで失敗する:

### GNU toolchain (`stable-x86_64-pc-windows-gnu`, このマシンのデフォルト)
`cargo build -p gui` が `x86_64-w64-mingw32-gcc` でのリンク時に
`ld returned 1 exit status` で失敗。`--verbose` でも具体的な未解決シンボル等の
詳細エラーテキストが得られず(collect2.exe側の出力がこの環境では拾えない)。
Web検索の結果、Tauri + webview2-com + GNU toolchain の組み合わせは
tauri-apps/tauri リポジトリに複数の未解決Issue(#12257, #10843, #4319等)が
あり、既知の未解決な相性問題と判明。簡単な回避策は見当たらない。

### MSVC toolchain (`stable-x86_64-pc-windows-msvc`)
`rustup override set` で切り替えてビルドを試みたところ、`link.exe` が
Git for Windows / MSYS2 に含まれる同名の別コマンド(POSIXのhardlinkユーティリティ)に
解決されてしまいエラー(「link: extra operand」)。さらに `vswhere.exe` で確認したところ、
そもそも **Visual Studio に「C++によるデスクトップ開発」ワークロード
(Microsoft.VisualStudio.Component.VC.Tools.x86.x64)自体がインストールされていない**
ことが判明。つまりPATHの問題以前に、本物のMSVCリンカーがこのマシンに存在しない。

## 決定: ユーザー確認なしにVisual Studio Build Toolsをインストールしない
C++ワークロードの追加インストールは数GBの新規ダウンロード・システム変更を伴う。
ユーザーが離席中で完了確認ができない状況でこれを無断で実行するのは、
「時間・システムへの影響が大きい判断は事前確認」という原則、および
ユーザー自身が設定した「律速段階(主に時間)のものは最終系タスクの傘下に送り込み、
今すぐ取り組めるものと区別する」という運用ルールの両方に照らして避けるべきと判断。

## 対応
- GUI-2を「完了」にはせず、Kanbanの Blocked 列に移動。
- 律速段階の詳細: 以下いずれかの対応が必要
  1. Visual Studio Installerで「C++によるデスクトップ開発」ワークロードを追加
     (推奨。Tauri公式もMSVCを推奨)
  2. または、GNU toolchainでのTauriリンクエラーをさらに調査する
     (上流でも未解決の既知問題のため成功する保証はない)
- `cargo check --workspace` はリンク不要なので通る。Rustコード自体の記述・
  型チェックは続けられるが、「実際にウィンドウが起動するか」の検証はできない。
- **今すぐ取り組めるもの**として、フロントエンド側(Framework7セットアップ、
  基本レイアウト)は`npm run dev`(Vite単体のプレビュー、Tauri不要)で独立して
  検証できるため、そちらを先に進める。
