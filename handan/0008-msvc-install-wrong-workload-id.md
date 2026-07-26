# 0008: MSVC Build Toolsインストール、3回目はworkload ID自体が間違っていた

**日時**: 2026-07-26

## 問題
elevation自体は成功した(exit code 0で正常終了)が、ログを見ると
`Warning: Cannot find package: Microsoft.VisualStudio.Workload.VCTools in product graph.`
と出ていた。`Microsoft.VisualStudio.Workload.VCTools`というID自体が、この環境
(Visual Studio 2022 **Community** Edition)の製品グラフに存在しない。

## 原因の推測
`Microsoft.VisualStudio.Workload.VCTools` は単体の「Build Tools for Visual Studio」
SKU向けのworkload IDで、フルのVisual Studio Community/Professional/Enterprise
エディションでは「Desktop development with C++」に相当するIDは
`Microsoft.VisualStudio.Workload.NativeDesktop` である可能性が高い。

## 対応
`Microsoft.VisualStudio.Workload.NativeDesktop` で再試行する。
