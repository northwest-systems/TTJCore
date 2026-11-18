# TTJCore

[TTJ 1.0.0](https://github.com/northwest-systems/TTJ)のProject・Task・Ticket・Job・Run・長期知識を管理するRust製CLIです。GUIは[herdr Mod](https://github.com/northwest-systems/herdr-mods-TTJ)が担当します。

基本のDBはSQLiteです。接続URLを変更すると任意のPostgreSQLへ接続できます。保存にはSeaORMを使い、作業データ・変更履歴・再送防止の結果を同一トランザクションで確定します。TTJCore自身はTCPポートを開きません。

## ビルド

Rustのstableツールチェーンと、そのOSでRustをリンクできる開発ツールが必要です。SQLiteドライバーのビルドにはCコンパイラーも使用します。

~~~sh
cargo build --locked --release
~~~

実行ファイルはtarget/release/ttjcore（Windowsではttjcore.exe）です。PATHへインストールする場合:

~~~sh
cargo install --path . --locked
~~~

Cargoが取得する直接依存は次のとおりです。固定した推移依存はCargo.lockに記録しています。

| クレート | 用途 |
|---|---|
| sea-orm | SQLite・PostgreSQL共通のDB操作とトランザクション |
| tokio | ORM・IPC・変更通知を動かす非同期実行基盤 |
| interprocess | Windows named pipe・Unix domain socketの共通API |
| serde・serde_json | JSONの入出力 |
| time | タイムゾーン付き日時の検証・生成 |
| uuid | 省略された操作IDの生成 |

## DBを初期化する

既定の接続先は、起動ディレクトリに作るsqlite://ttjcore.sqlite?mode=rwcです。

~~~sh
ttjcore init
ttjcore serve
~~~

別のSQLiteファイルやPostgreSQLを使う場合はTTJCORE_DATABASE_URLを設定してからinitを実行します。SQLiteファイルの親ディレクトリ、PostgreSQLのDB・接続ユーザーは事前に用意してください。

PowerShell 7:

~~~powershell
$env:TTJCORE_DATABASE_URL = 'sqlite://ttjcore.sqlite?mode=rwc'
ttjcore init
~~~

Bash:

~~~sh
export TTJCORE_DATABASE_URL='sqlite://ttjcore.sqlite?mode=rwc'
ttjcore init
~~~

PostgreSQLの接続URLはpostgresql://<role>:<password>@<host>:<port>/<database>の形式です。パスワードはURLエンコードしてください。接続先の認証・TLS設定に応じて接続オプションを指定できます。接続情報をGitへ保存しないでください。

initは初期スキーマを作成します。異なるprotocolのDBは拒否します。初期化後、同じDB設定でttjcore serveを起動しておき、別の端末からcallまたはrpcで接続します。停止はserveを起動した端末でCtrl+Cです。

## 常駐Coreへの接続

serveがDBへの接続を所有します。callとrpcはWindowsではnamed pipe、Linux・macOSではUnix domain socketを通じて同じCoreへ接続します。TTJCORE_DATABASE_URLを使うのはinitとserveだけです。

TTJCORE_HOMEは接続先のプロファイルです。省略するとWindowsではLOCALAPPDATA/.ttjcore、Unixでは$HOME/.ttjcoreを使います。別のプロファイルを使う場合は、serve・クライアント・Herdrに同じ値を設定してください。Unixでは他ユーザーが書き込めないディレクトリを使います。

プロファイルにはローカル接続情報と接続用トークンが保存されます。Gitへ保存せず、共有しないでください。同一プロファイルで起動できるCoreは1つです。DB接続エラーはserveの起動時、Core未起動・プロファイルの不一致はクライアント接続時に確認します。

## CLIで作成する

callはJSON-RPC要求を組み立て、結果をJSONで標準出力へ返します。以下の--paramsの値はJSONファイルに保存し、--file <params.json>でも渡せます。変更操作にはactorとreasonが必要です。

~~~sh
ttjcore call project.create --params '{"actor":["operator"],"reason":"作業領域を作る","operation":"create-demo-project","record":{"id":"demo","name":"デモプロジェクト"}}'
ttjcore call task.create --params '{"actor":["operator"],"reason":"成果目標を定める","operation":"create-demo-task","record":{"id":"prepare","project":"demo","objective":"案内を用意する","criteria":["確認済みの案内がある"]}}'
ttjcore call ticket.create --params '{"actor":["operator"],"reason":"固定した作業計画を作る","operation":"create-demo-ticket","record":{"id":"invitation","task":"prepare","objective":"案内文を完成させる","jobs":[{"id":"draft","objective":"案内文を書く","criteria":["必要事項が記載されている"]},{"id":"review","objective":"案内文を確認する","criteria":["確認結果が残っている"]}]}}'
ttjcore call project.snapshot --params '{"id":"demo"}'
~~~

Ticketと全Jobは一括作成します。JobのID列はサーバーが生成し、作成後の追加・削除・並べ替えや目標・条件・手順の変更を拒否します。

## 参照・更新・削除

~~~sh
ttjcore call project.list
ttjcore call ticket.get --params '{"id":"invitation"}'
ttjcore call job.list --params '{"parent":"invitation"}'
ttjcore call timeline.list --params '{"project":"demo"}'
~~~

参照結果のrecordがTTJの内容、revisionが更新用の世代です。変更結果はresult.valueに入り、result.operationに操作IDを返します。更新・削除・状態変更には直前に取得したrevisionを指定します。

~~~sh
ttjcore call ticket.update --params '{"actor":["operator"],"reason":"担当を決める","operation":"assign-invitation","id":"invitation","revision":1,"patch":{"assignee":["operator"]}}'
~~~

上のrevisionは例です。実際の取得値を使ってください。省略した項目は維持し、担当の継承へ戻す場合はassigneeをnullにします。空の一覧は[]で明示します。

| 対象 | 作成・更新・削除 |
|---|---|
| Project | create、nameのupdate、配下の作業がすべて終端ならdelete |
| Task | create、非終端時の目標・条件・期限のupdate、終端後のdelete |
| Ticket | Jobと一括create、担当・申し送り・再開条件等のupdate、終端後のdelete |
| Job | Ticket内でcreate、担当・再開条件のupdate、Ticketとともにdelete |
| Run | run.startで作成、run.finishで結果を確定 |
| 知識 | knowledge.create、採用状態のupdate、廃止後のdelete |

削除は論理削除です。通常の一覧から除外し、履歴・依存の完了記録・IDは保持します。include_deleted=trueで削除済みレコードを参照できます。Task・Ticketを削除してもProjectの知識は残ります。知識の内容や根拠の訂正は、新しい記録をsupersedesで旧記録に結び付けます。

## 実行と状態変更

run.startにJobのrevision、Runのid、実参加者のactorを渡します。run.finishにはRun自身のrevisionと結果を渡します。

~~~sh
ttjcore call run.start --params '{"actor":["operator"],"reason":"開始条件を確認した","operation":"start-draft","id":"draft-run","job":"draft","revision":1}'
ttjcore call run.get --params '{"id":"draft-run"}'
~~~

Runの終了だけではJobを完了しません。status.setでcriteria_met=trueとresultの確認根拠を渡して完了します。Ticketは全Jobがcompletedまたはcanceledになれば自動でcompletedになり、Ticket自身のcanceledを優先します。Taskは全Ticketの終端と、Task自身の完了条件の確認が必要です。

status.setには次を指定できます。

| 項目 | 用途 |
|---|---|
| id・revision・status | 対象、取得時の世代、遷移先 |
| actor・reason・operation | 変更主体、理由、再送用の操作ID |
| criteria_met・result | Task・Jobの完了条件の確認と根拠 |
| release=true | 明示保留を解除する |
| resolved=true | 待機・障害の解除を確認した |
| run | 参照するRunの実行権・世代も確認する |

同じTicketの別Jobの同時実行、未完了の依存先を飛ばす開始、終端からの復帰を拒否します。同一Jobには担当ごとに共同作業のRunを作成できます。再試行は新Runにし、必要ならretry_ofとsafetyに前Runと安全確認を指定します。

Run APIは実行の記録とDB内の更新権を管理します。外部プログラムの起動・監視・OS上の停止は行いません。停止要求は履歴・管理情報に記録されるため、実行側が実際の停止や安全条件を確認してください。

## LLM・herdr向けプロトコル

~~~sh
ttjcore rpc
~~~

標準入力の1行につきJSON-RPC 2.0要求またはバッチを受け取り、応答を標準出力へ1行で返します。要求ルートにprotocolを指定します。

~~~json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","id":"query-1","method":"project.snapshot","params":{"id":"demo"}}
~~~

rpcは常駐Coreへの標準入出力ブリッジです。通常のJSON-RPC通知要求には応答しません。stdoutには応答・変更通知のJSONだけを出力します。callとrpcは同じ処理・規則を使い、接続用トークンは自動で渡します。

### 変更を監視する

LLM・実行エージェントはCoreから直接変更を取得します。UIを経由しません。参照結果のcursorを次の取得に渡せば、その後に確定した履歴を取得できます。

| メソッド | 動作 |
|---|---|
| events.list | cursorより後の変更を即時取得 |
| events.wait | 変更またはtimeout_msまで待つ。既定20秒、最大25秒 |
| events.subscribe | rpc接続で購読し、変更時にttj.changed通知を受信 |

projectを指定すると対象Projectの変更に絞れます。cursorを省略すると現在位置から開始します。応答のcursorはProjectの絞り込みにかかわらず全体の取得位置なので、返された値をそのまま次に渡してください。長時間待機のタイムアウトではeventsが空になります。

~~~sh
ttjcore call events.wait --params '{"project":"demo","cursor":0,"timeout_ms":20000}'
~~~

rpcへの購読要求例:

~~~json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","id":"watch-1","method":"events.subscribe","params":{"project":"demo","cursor":0}}
~~~

最初の応答に未取得の変更を返し、その後は同じ接続へ通知します。

~~~json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","method":"ttj.changed","params":{"cursor":42,"events":[{"id":42,"project":"demo","target":"draft","event":"status.changed","operation":"complete-draft","revision":3,"timestamp":"2026-01-01T00:00:00Z","status":"completed"}]}}
~~~

切断後は最後に受け取ったcursorで再接続できます。通知はコミット済みの履歴を参照し、同じoperationの再送では新たな変更通知を作りません。購読中はrpcの接続を開いたままにしてください。Herdr Modはevents.waitを使うため、変更のない間にスナップショットを繰り返し取得しません。

実行エージェントはrun.start・run.finish・status.setで状態を記録してください。CoreはCodex・Claude Codeのプロセスを自動で検出・操作しません。

ttjcore methods、またはrpc.discoverでメソッドと必要項目を確認できます。更新を再送する場合は同じoperationと同じ要求内容を使います。同じ操作IDで異なる内容を送ると競合になります。操作IDを省略した場合は生成しますが、再送を予定するクライアントはあらかじめ指定してください。

| エラーコード | 意味 |
|---|---|
| -32700 | JSONを解釈できない |
| -32600 | JSON-RPC要求が不正 |
| -32601 | メソッドが存在しない |
| -32602 | 入力項目またはprotocolが不正 |
| -32004 | 対象が見つからない |
| -32009 | revisionまたは操作IDの競合 |
| -32010 | TTJ規則による拒否 |
| -32000 | DB操作の失敗 |
| -32001 | ローカルCoreへの認証失敗 |

CLIのcallはRPCエラー時に終了コード2、起動・接続等の失敗時に1を返します。

## テスト

通常はSQLiteだけで実行できます。

~~~sh
cargo test --locked
~~~

PostgreSQLは任意の接続先への互換性テスト用です。付属のCompose設定はテスト専用で、メモリー上の一時DBを起動します。PostgreSQL 18の公式イメージをDockerが取得します。DBポートは既定でホストのループバック15432にだけ公開します。

PowerShell 7:

~~~powershell
$env:TTJCORE_POSTGRES_PASSWORD = [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(24)).ToLowerInvariant()
docker compose -f compose.postgres-test.yaml -p ttjcore-test up -d --wait
$env:TTJCORE_TEST_POSTGRES_URL = "postgresql://ttjcore_test:$($env:TTJCORE_POSTGRES_PASSWORD)@127.0.0.1:15432/ttjcore_test"
cargo test --locked postgres_contract -- --ignored
docker compose -f compose.postgres-test.yaml -p ttjcore-test down
~~~

別のPostgreSQLでテストする場合もTTJCORE_TEST_POSTGRES_URLで指定できます。テストはレコードを作成・更新・論理削除するため、専用のテストDBを使ってください。TTJCORE_DATABASE_URLとは別の設定なので、通常利用のSQLiteは切り替わりません。

## ライセンス

[Apache-2.0](LICENSE)。