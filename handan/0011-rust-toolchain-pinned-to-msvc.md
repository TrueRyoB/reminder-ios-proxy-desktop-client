# 0011: rust-toolchain.toml でプロジェクト全体をMSVCツールチェーンに固定

**日時**: 2026-07-26

## 決定
グローバルなデフォルトツールチェーン(`stable-x86_64-pc-windows-gnu`)は変更せず、
プロジェクトルートに `rust-toolchain.toml` を追加して、このリポジトリ内でのみ
`stable-x86_64-pc-windows-msvc` が自動選択されるようにした。

```toml
[toolchain]
channel = "stable-x86_64-pc-windows-msvc"
```

## 理由
- GNUターゲットは Tauri + webview2-com の未解決リンクバグ(handan/0004参照)があり使えない。
- グローバルPATH/デフォルトツールチェーンを変更すると、このマシン上の他のRustプロジェクトに
  も影響するため、handan/0005の決定(プロジェクトローカルな解決を優先)を踏襲した。
- `rust-toolchain.toml` はrustupが自動検出するため、`cargo build`(`+toolchain`指定なし)で
  workspace全体(core/cli/gui)がリンクエラーなくビルドできることを確認済み。

## 検証結果
- `cargo build --workspace` (rust-toolchain.toml適用後、`+`指定なし): 成功、リンクエラーなし。
- `rustc --version` は `x86_64-pc-windows-msvc` 版を指す。
