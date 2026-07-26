# 0025: QA-A(読み込み速度)実測 → 根本原因特定 → 修正(約100倍高速化)

**日時**: 2026-07-26

## 実測結果(修正前)
Rust側に`tracing`タイミング計測を追加(`tracing_subscriber`をGUIに新規導入、
`--env-filter`のデフォルトを"info"に設定し`cargo tauri dev`のターミナルで
即座に確認できるようにした)。実アカウントに対する実測値:

| 呼び出し | 所要時間 |
|---|---|
| try_resume(セッション再開) | 2859ms |
| **list_lists** | **33122ms (33秒!)** |
| list_reminders(7リスト、並列実行) | 321ms〜3600ms(最大3.6秒) |

`list_lists`が突出して異常な値であり、これが体感速度の遅さの支配的要因と判明。

## 根本原因
`CloudKitClient::changes_all`(core/src/cloudkit.rs)が`syncToken`を
一切永続化・再利用しておらず、**毎回このアカウントのZone変更履歴全体を
作成時点から再生していた**。この account は履歴が長い(274件のリマインダーを
含むリストもある)ため、33秒という異常値につながっていた。

`list_reminders`側(`/records/query`の`reminderList`複合クエリ)は
`changes/zone`を使わないため、この問題の影響を受けていない(が、
別の高速化余地がある。後述)。

## 修正内容
1. `CloudKitClient::changes_all`のシグネチャを変更:
   `sync_token: Option<&str>`を受け取り、`(Vec<Value>, Option<String>)`
   (取得レコード + 最終sync token)を返すように変更。
2. `core/src/reminders.rs`: `ListCache`構造体(sync_token + レコードの
   HashMap)を新設。`RemindersService::lists_cached(&self, cache: &mut ListCache)`
   を追加 -- 差分(upsert/削除)をキャッシュにマージして現在の全件を返す。
   既存の`lists()`(CLIが使う、常にフルリプレイ)は非破壊的に維持
   (シグネチャ変更のみ、CLIの呼び出し側は無変更で動作)。
3. `core/src/session_store.rs`: `load_list_cache`/`save_list_cache`を追加
   (`list_cache.json`として永続化)。
4. `gui/src-tauri/src/commands.rs`: `list_lists`コマンドを
   `lists_cached`経由に変更、呼び出しごとにキャッシュを読み込み→
   同期→保存。

## 実測結果(修正後)
| 呼び出し | 所要時間 |
|---|---|
| try_resume | 3011ms |
| **list_lists(差分同期)** | **323ms** |
| list_reminders(7リスト、並列実行) | 321ms〜3536ms |

**list_lists: 33122ms → 323ms(約100倍)**。アプリ起動〜ダッシュボード表示
までの体感時間は、体感上支配的だった33秒の待ち時間がほぼ解消された。

## 未達成の目標値について
ユーザーの「0.1秒帯」という目標は、実サーバー(Apple CloudKit)との
通信を伴う設計である以上、現実的ではないと判断する。try_resume(~3秒)や
list_reminders(最大3.5秒、並列)は依然としてネットワーク往復に依存する。
今回の修正は「異常に遅い部分(33秒)」を「妥当な範囲(0.3秒)」に
是正したものであり、体感上の「遅すぎる」という問題は大きく解消された
はずだが、数秒単位の待ち時間そのものはネットワーク前提である限り残る。

## 追加で発見した高速化余地(次のタスクとして記録・未実装)
`list_reminders`も、各リストごとに`/records/query`でreminderListの
フル取得を行っており、リストのサイズ(reminder数)に比例して時間がかかる
(274件のリストで3.5秒)。ListとReminderは同一のCloudKit Zone
("Reminders")に存在するため、`changes/zone`の`desiredRecordTypes`に
`["List", "Reminder"]`両方を指定すれば、**理論上は1回の差分同期呼び出しで
両方を取得でき、list_lists + N回のlist_remindersを丸ごと置き換えられる**。

今回はこれを実装しなかった。理由: create/update/delete/reorder操作後の
ローカルキャッシュ整合性(作成直後のリマインダーが次の差分同期まで
見えない、といったずれ)を正しく設計・検証する必要があり、単独の
list_lists修正よりもリスクと影響範囲が大きい。QA-A2として別途
検討することとする。

## 検証内容
1. `cargo clippy -p reminder-core -p reminder-proxy-client -p gui --all-targets -- -D warnings` — 通過。
2. `cargo test --workspace --exclude gui` — 既存5テスト通過。
3. `cargo tauri dev`を2回実行(1回目でキャッシュ生成、2回目で差分同期の
   高速化を実測)し、上記の実測値を取得。実際にファイルが永続化され
   (`list_cache.json`、sync_token + 7件のList record)、2回目の起動で
   確実に読み込まれ活用されていることを確認。

## 副次的なトラブル: ビルドキャッシュ破損
作業中に`link.exe`が`LNK1207(incompatible PDB format)`で失敗する現象が
発生。`target/debug/incremental`の個別削除では直らず、原因はこのセッション中に
繰り返し`cargo build`/`cargo tauri dev`を強制終了(Stop-Process -Force)して
きたことによるビルド成果物の破損と推測。`cargo clean`(17GB削除)からの
フルリビルドで解消した。

## 次のタスク
QA-A2(新規、バックログ): list_reminders/list_lists統合による
reminders単位の差分同期。QA-C2/C3(操作フィードバック)へ進む。
