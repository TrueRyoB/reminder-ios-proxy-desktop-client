# 0005: MSVC Build Tools インストールをユーザーが明示的に承認

**日時**: 2026-07-26

## 決定
ユーザーから明示的な許可を得た:「インストールしてください。パス登録もよろしくお願いします。」

## 実施内容
- 既存のVisual Studio 2022 Community (`C:\Program Files\Microsoft Visual Studio\2022\Community`)
  に対し、`Microsoft.VisualStudio.Workload.VCTools`(C++ビルドツール、`--includeRecommended`なしの
  最小構成)を追加インストール。事前にCドライブ空き容量404GBを確認済み、問題なし。
- 「パス登録」については、システム全体のPATH環境変数を変更するのではなく、
  **プロジェクトローカルの`.cargo/config.toml`でlinkerを明示指定**する方式を採用する。
  理由: このマシンではGit for Windows/MSYS2にも`link.exe`という同名コマンド
  (POSIXのhardlinkユーティリティ)が存在し、グローバルPATHの並び順を変更すると
  他のツール(Git Bash等)の挙動に予期せぬ影響を与えるリスクがある。Cargoの
  `.cargo/config.toml`はプロジェクト単位で有効なため、この問題を副作用なく解決できる。
