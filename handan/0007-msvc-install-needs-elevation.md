# 0007: MSVC Build Toolsインストール、2回目は管理者権限不足で失敗(exit 5007)

**日時**: 2026-07-26

## 問題
2回目の試行(`--wait`引数の誤りを修正後)は exit code 5007。インストーラーログを
確認したところ原因は明確:
「Commands with --quiet or --passive should be run elevated from the beginning.」
= `--quiet`(サイレント)モードでの実行には、プロセス自体が最初から管理者権限で
起動している必要がある。現在のPowerShellセッションはその状態ではなかった。

## 対応
`Start-Process -Verb RunAs` で管理者権限昇格を試みる。ただしこれはUACの同意
ダイアログを伴う可能性があり、その場合はユーザーが物理的にPCの前にいないと
「はい」をクリックできず失敗する。UACが「管理者に通知しない」設定であれば
非対話的に成功する可能性もある。結果次第で対応を分ける。
