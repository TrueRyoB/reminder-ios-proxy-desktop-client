# 0009: workload追加だけではMSVCコンパイラ本体(v143)が入らなかった

**日時**: 2026-07-26

## 状況
`Microsoft.VisualStudio.Workload.NativeDesktop` の追加は今回受理され、実際に
チャンネルマニフェスト取得・カタログ確認等の処理が走り、exit code 0で終了した。
しかし完了後も `VC\Tools\MSVC`(実際のコンパイラ/リンカー本体)は作成されず、
`VC\Tools\Llvm`(別コンポーネント、Clang/LLVM)のみが存在していた。

## 推測
"NativeDesktop"はIDE全体としての「デスクトップ開発」ワークロードで、その中の
「必須」コンポーネント集合には実はMSVCコンパイラ本体(v143 build tools)が
含まれておらず、`--includeRecommended`を付けないと入らない可能性がある。

## 対応
ワークロード単位ではなく、**コンパイラ本体を指す具体的なコンポーネントID**
`Microsoft.VisualStudio.Component.VC.Tools.x86.x64` を直接 `--add` する。
これは`vswhere -requires`で最初に確認した際に使った、曖昧さのないIDである。
