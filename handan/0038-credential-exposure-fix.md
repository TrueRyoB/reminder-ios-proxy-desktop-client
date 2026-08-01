# 0038: 認証情報の保存方式の見直し(DPAPI 封緘 / パスワード非保存)

**日時**: 2026-08-02

## 指摘

> windows credentials に iOS のセッション情報を保持していますが、
> どのアプリも参照可能となっています

## 実測した現状(指摘の精緻化)

指摘の**機序は正しい**。ただし当日の実機の状態は指摘とは少しずれていた。

1. **Credential Manager の実態は 0 件だった**。`cmdkey /list` で
   `reminder-proxy-client` に一致するエントリは無し。理由は 0037 の B1 —
   `keyring` がバックエンド機能未指定で mock(プロセス内メモリ)にフォールバック
   しており、**書き込みが一度も成立していなかった**ため。
   つまり Credential Manager 経由の露出は**潜在**であり、B1 を直した v0.1.0 で
   次に生パスワードログインをした瞬間に**顕在化する**ところだった。
2. **実際に今漏れていたのはファイルの方**。
   `%APPDATA%\reminder-proxy-client\data\` の
   - `auth_state.json`(1601 B) — session token / trust token / client id / apple_id
   - `cookies.json`(7260 B) — iCloud セッション Cookie
   がいずれも**平文**。先頭 20 バイトを読むだけで Apple ID が見えた。
   トークンは**パスワードも 2FA も要さずアカウントにアクセスできる**ので、
   資産価値としてはこちらの方が高い。
3. GUI は**パスワードを書くだけで一度も読んでいなかった**
   (`commands.rs:326` に `set_password` のみ、`get_password` は CLI 側だけ)。
   すなわち GUI にとっては**利益ゼロの純負債**だった。

## 対応

1. **パスワードを保存しない**
   - GUI: 保存処理を削除。さらに起動時に
     `bootstrap::forget_stored_password` を呼び、旧版が残した資格情報を消す。
   - GUI: `AuthState::AwaitingTwoFactor` から `password` フィールドを削除。
     2FA 待ちの間パスワードをメモリに保持する理由が無くなったため
     (`validate_trusted_device_code` / `trust_session` はセッション側で完結)。
   - CLI: 保存は `--save-password` の明示オプトイン時のみ。保存時は
     「同一ユーザーの他プロセスから読める」旨を警告出力。`forget-password`
     サブコマンドで削除可能。
   - GUI から `keyring` 依存自体を撤去。
   - 補足: trust token が失効した状態での再ログインは Apple が 2FA を要求するので、
     パスワードを保存しても「無人で復帰」は元々成立しにくい。利益は小さかった。
2. **セッションファイルを DPAPI で封緘**(`core/src/dpapi.rs` 新規)
   - `CryptProtectData`(ユーザースコープ、`CRYPTPROTECT_UI_FORBIDDEN`)。
     `windows-sys` は既に lock 内にあり新規パッケージは増えない。
   - ファイル形式は `RPC1` マジック + 封緘済み blob。マジックが無いファイルは
     旧版の平文とみなし、**初回読み取り時にその場で封緘し直す**
     (`session_store::read_sealed`)。再ログインは不要。
   - 復号不能(別ユーザー/別マシンで封緘、破損)は「最初からやり直す」に
     縮退させ、エラーを上に投げない。
   - `list_cache.json` と `proxy_store.json` は資格情報ではないので平文のまま
     (後者は手編集可能であることが仕様)。
3. **README に保存場所と保護範囲の表**を追加。

## 誇張しないこと(脅威モデル)

DPAPI ユーザースコープが防ぐのは「**ファイルを入手しただけの相手**」——
バックアップ、クラウド同期に拾われたコピー、同一マシンの別アカウント、
オフラインのディスクイメージ。これらは Windows のログオン秘密なしに復号できない。

**防がないのは同一ユーザーで動く他プロセス**。同じ entropy で
`CryptUnprotectData` を呼べるし(entropy はバイナリ内にあり秘密ではない)、
プロセスメモリも読める。Windows は通常のデスクトップアプリに
**アプリ単位の分離を提供しない**——これは Credential Manager も同じで、
「Credential Manager だから危ない / ファイルだから安全」ではなく、
**どちらも同一ユーザー境界しか無い**。同一ユーザーのコードまで排除したければ、
ディスクに置かないマスターパスフレーズ(毎回入力)しか手が無い。これは UX の
決定なので勝手には入れず、README に選択肢として明記した。

## 検証

- `cargo test` **12件**(8 → 12)。追加分:
  - `dpapi`: 往復 / 空入力 / 改竄 blob の拒否
  - `session_store`: 封緘して書かれること(トークン文字列がファイル中に現れない)
    / **平文からのその場移行**(セッションを失わない、二度目も読める)
    / 復号不能時に空状態へ縮退
- `clippy --workspace --all-targets -D warnings` / `cargo build --release -p gui` 通過。
- edition 2024 では `unsafe fn` の本体が暗黙 unsafe ではないため、
  内側に `unsafe {}` が要る(踏んだ)。

## 申し送り

- **実機のファイルはまだ平文**。次回アプリ起動時に自動で封緘される
  (起動時の `load_auth_state` が移行を駆動する)。移行前のバックアップは
  セッションのスクラッチパッドに退避済み。
- **v0.1.0 のドラフトリリースは公開しないこと**。B1 修正済み=パスワードを
  実際に Credential Manager へ書く最初のビルドであり、かつトークンは平文のまま。
  破棄して v0.1.1 に差し替える。
