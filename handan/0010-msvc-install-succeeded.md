# 0010: MSVC Build Tools インストール成功(5回目の試行で解決)

**日時**: 2026-07-26

## 結果
`Microsoft.VisualStudio.Component.VC.Tools.x86.x64` + `Microsoft.VisualStudio.Component.Windows11SDK.22621`
を直接 `--add` する方式で成功。ファイルシステムで直接確認(報告されたexit codeだけに頼らない、
これまでの教訓通り):

- `VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe` — 存在確認済み
- Windows Kits SDK `10.0.22621.0` — 存在確認済み

## 教訓の総括(このMSVCインストール一連の試行から)
1. ワークロードID(`NativeDesktop`)はIDE全体の「デスクトップ開発」を指し、その必須コンポーネント
   集合に実際のコンパイラ本体(MSVC v143 build tools)が含まれるとは限らない。
2. コンパイラそのものが欲しい場合は、ワークロード単位ではなく `vswhere -requires` で使う
   具体的なコンポーネントID(`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`)を直接指定するのが確実。
3. 背景タスクの報告する exit code は外側のシェルラッパーの終了コードであり、実際のインストーラの
   成否を保証しない。必ずファイルシステムを直接確認する。

## 次のステップ
グローバルPATHは変更せず(handan/0005の決定通り)、プロジェクトローカルの `.cargo/config.toml` で
MSVCターゲット(`x86_64-pc-windows-msvc`)を明示指定する。GNUターゲットはTauri+webview2-comの
既知の未解決バグがあるため使わない。
