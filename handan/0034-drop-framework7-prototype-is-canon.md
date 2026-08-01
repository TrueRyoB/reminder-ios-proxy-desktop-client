# 0034: Framework7 を全廃し、Gate 2 プロトタイプを見た目の正とする

**日時**: 2026-08-01

## 経緯
0033 の実装を実機で確認したユーザーから「UIが壊れている。web版(プロトタイプ)の
完全コピーの方が良かったまである。新しいタスク追加のカードがヘッダーに隠れて
いる。元々あったUIは無プランで作られた最低最悪のもの」との裁定。

直接原因: Framework7 の `.navbar` は固定配置でコンテンツに覆い被さる設計のため、
その直後に通常フローで置いたクイック投函行(#quickrow)がナビバーの下に隠れた。
この種の衝突は F7 レイアウトの上にカスタム UI を重ねる限り構造的に再発する。

## 判断
- **design/draft/dashboard-prototype.html(Gate 2 でユーザーが触って承認した
  唯一の接地物)を見た目の正とし、gui/ はその移植とする。**
- Framework7 を import から全廃(レイアウト・シート・ダイアログ・トースト・
  ソート全て)。代替: 自前の軽量シート/トースト/プリローダ、confirm/prompt は
  WebView2 ネイティブ、並び替えは HTML5 DnD(皿と同方式)。
- **overflow 規律**(ユーザー指示): 件数が伸びうるセクションには必ず
  `max-height + overflow-y:auto` を付け、崩れを局所に閉じ込める。適用箇所:
  締切支配ゾーンの続き(280px)/候補列・皿(52vh)/皿の進行(320px)/サイドバー。
- サイドバーはハンバーガー開閉をやめ常設(デスクトップアプリの前提に合わせ、
  プロトタイプと同形)。
- この方針は auto-memory(gui-prototype-is-canon)にも保存済み。

## 副産物
- フロントバンドルが 801KB → 31KB(gzip 214KB → 11KB)。
- フラグトグルが実 <button> になり、キーボード操作の特別対応(QA-I の tabindex
  ハック)が不要になった。

## 検証
`npx tsc --noEmit` / `vite build` 通過。デバッグ exe を Vite 開発サーバ
(localhost:1420)に接続して起動、`Responding: True`。
なお 0033 で「localhost refused」が出たのは、デバッグビルドは devUrl に接続する
仕様(フロント埋め込みはリリースのみ)なのに Vite を立てずに exe を直接起動した
ため。開発時は `npx vite --port 1420` 常駐 + exe、または `cargo tauri dev` が正規。

## 残課題
実機視覚 QA の再実施(0033 の4観点)。stylesheet 層(design/stylesheet/)との
突き合わせは未実施 — プロトタイプ CSS が正になった今、余白・配色の整合は
stylesheet 層の判断として別途。
