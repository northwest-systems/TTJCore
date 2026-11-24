# TTJCore protocol

TTJCore 0.1.1のCLI・JSON-RPC contractです。`protocol`は`TTJ/1.1.0`です。UIやLLMなどのクライアントはCoreへ直接接続し、この文書と`ttjcore methods`でinterfaceを確認します。

TTJ自体の作業規則は同梱の[TTJ 1.1.0規格本文](ttj-1.1.0/references/specification.md)、項目と例は[JSON参照形式](ttj-1.1.0/references/json-format.md)を参照します。いずれもネット接続なしで読めます。

## 接続とCLI

CoreはDBへの接続を所有します。clientとの通信はWindowsではnamed pipe、Linux・macOSではUnix domain socketです。TCPポートは開きません。

```sh
ttjcore ensure
ttjcore methods
ttjcore call project.list
ttjcore call METHOD --params JSON
ttjcore call METHOD --file /path/to/params.json
ttjcore rpc
ttjcore stop
```

- `ensure`はDBを初期化し、Coreを常駐起動する。すでに起動中なら再利用する。
- `call`は1つのmethodを呼び、JSON-RPC responseをstdoutへ返す。paramsを省略すると`{}`。
- `rpc`はstdio bridge。stdin・stdoutを開いたまま複数のrequestやeventを扱える。
- `methods`はCoreを起動せずmethod・fieldの情報を返す。同じ情報を`rpc.discover`でも取得できる。
- `stop`は同じprofileのCoreを停止する。TTJのrecordは変更しない。

profileは`--home <data-dir>`、未指定時は`TTJCORE_HOME`で選びます。両方を省略するとWindowsは`LOCALAPPDATA/.ttjcore`、Linux・macOSは`$HOME/.ttjcore`です。すべてのclientで同じprofileを使ってください。

```sh
ttjcore --home /path/to/profile ensure
ttjcore --home /path/to/profile call project.list
ttjcore --home /path/to/profile rpc
```

標準DBはprofile内の`core.sqlite`です。`TTJCORE_DATABASE_URL`はensure・init・serveが使用し、call・rpcは使用しません。起動済みのCoreはDB設定を変更しても切り替わりません。

## JSON-RPC

UTF-8の1行にJSON-RPC 2.0 requestまたはbatchを入れ、改行で区切ります。responseとeventも1行ずつ返ります。stdoutに診断messageは混在させません。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.1.0","id":"query-1","method":"project.snapshot","params":{"id":"<project-id>"}}
```

`id`はstring・number・nullを使用できます。通常のrequestで`id`を省略するとnotificationとして処理し、responseを返しません。batchではnotificationを除いたresponseの配列を返します。`events.subscribe`と`core.stop`は`id`付きの単独requestを使ってください。

call・rpcはprofileの接続情報からtokenを読み、IPC requestの`auth`へ自動で付与します。直接IPCへ接続するclientにも`auth`が必要です。接続情報を保存する`connection.json`やtokenを共有・記録しないでください。

responseにはrequestと同じ`protocol`・`id`、および`result`または`error`が入ります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.1.0","id":"query-1","error":{"code":-32004,"message":"Record not found"}}
```

## Recordとmethod

Project → Task → Ticket → Jobの階層を持ち、RunはJobに属します。Ticketは1つのTaskに所属します。KnowledgeはProjectに属します。

create入力の`id`は省略すると、7種それぞれの接頭辞（`pj-`・`tsk-`・`tkt-`・`job-`・`run-`・`wkr-`・`ssn-`）と英数字8桁の系列、1始まりのインクリメントで生成します（例：`tsk-12abcd00-1`）。明示IDは英数字6桁または8桁の系列も許容し、増分のゼロ埋めを拒否します。削除済み・物理削除済みを含むIDを再利用しません。KnowledgeのIDは別形式です。

| record | required field | optional field |
|---|---|---|
| Worker | name | id |
| Session | worker_id | id, started, ended |
| Project | name | id |
| Task | objective, criteria。params.parentにProject ID | id, deadline, resume, handoff |
| Ticket | task_id, worker_id, session_id, objective | id, jobs, exclusions, depends, refs, artifacts, resume |
| Job | objective, criteria | id, instructions |
| Knowledge | content, basis。params.parentにProject ID | id, supersedes |

Jobは`ticket.create`の`record.jobs`に完全な定義を並べます。参照結果でもJobオブジェクト配列を返し、各Jobの`runs`にRunを内包します。Ticketのworker_id・session_idとJob列・計画は確定後に変更できません。別Sessionへの移行は旧Ticketを閉鎖し、refsに旧Ticketを参照する新Ticketを作成します。refsは規格の`ticket_id`と任意の`kind`・`note`を使い、開始条件として解釈しません。

| field | 型・扱い |
|---|---|
| actor | 空でないstring配列。実際のparticipantを指定 |
| reason | 空でないstring |
| criteria | 空でないstring配列 |
| exclusions | Ticketの除外事項を示すstring配列 |
| depends | Ticket ID配列。循環は不可 |
| handoff | Taskの揮発性単一オブジェクト。completed時に除去 |
| resume | string |
| deadline | timezone付きRFC 3339 string |
| worker_id, session_id | Ticketの固定担当を示すID |
| revision | 最新の参照結果にあるinteger |

| method | params・response |
|---|---|
| `<kind>.get` | `id`。record wrapperを返す |
| `<kind>.list` | optionalの`parent`・`project`で絞り込み。`items`と`cursor`を返す |
| `<kind>.create` | `actor, reason, record`。Job・Runを除く |
| `<kind>.update` | `actor, reason, id, revision, patch`。Runを除く |
| `<kind>.delete` | `actor, reason, id, revision`。Job・Runを除く |
| `project.snapshot` | Projectの`id`。`project, tasks, timeline, knowledge, dependencies, cursor`を返す |
| `timeline.list` | `project`。履歴の配列を返す |
| `status.set` | `actor, reason, id, revision, status` |
| `task.purge` | `actor, id, revision`。終了したTaskと全配下を物理削除し要約履歴を保持 |
| `run.start` | `actor, reason, job, revision`。revisionはJobの値 |
| `run.finish` | `actor, reason, id, revision, result`。id・revisionはRunの値 |
| `core.stop` | IPC・rpc経由でCoreを停止。recordの変更とは別の操作 |

`<kind>`は`project`・`task`・`ticket`・`job`・`run`・`knowledge`・`worker`・`session`です。削除済みをget・list・timeline.listで読む場合は`include_deleted:true`を指定します。

record wrapperには`kind, record, revision, deleted, started, active, constraints, display`が入ります。`kind`は対象の種類、`record`がTTJの内容です。変更responseの`result.operation`がoperation ID、`result.value`が変更結果です。`run.start`のvalueは`run`・`job`のwrapperを含みます。

`timeline.list`とsnapshotの`timeline`には、各履歴の対象種別を`kind`で返します。値は`project`・`task`・`ticket`・`job`・`run`・`knowledge`・`worker`・`session`です。物理削除後の要約履歴にも種別を保持します。

`project.snapshot`の`tasks`は`{task, tickets}`、各Ticketは`{ticket, jobs}`です。これらのrecordはwrapperで返り、`jobs`は実行順です。snapshotの`knowledge`はactiveなKnowledgeを返します。

`dependencies`は、現在のTicketとtimelineの`depends`が参照するTicketのwrapper配列です。別Project・削除済みの参照先も含め、UIは`record.objective`を表示名に使えます。既存の`tasks`・`timeline`の構造は変わりません。

## CRUDと再送

次の内容を`project.json`へ保存し、`ttjcore call project.create --file project.json`で作成できます。

```json
{"actor":["operator"],"reason":"Projectを作成する","operation":"example-project-create","record":{"name":"Example"}}
```

作成IDは`result.value.record.id`に返ります。以降の`<project-id>`・`<task-id>`などは、実際に返されたIDへ置き換えてください。

Ticketの作成paramsの例:

```json
{"actor":["operator"],"reason":"作業計画を登録する","operation":"example-ticket-create","record":{"task_id":"<task-id>","worker_id":"<worker-id>","session_id":"<session-id>","objective":"案内を完成させる","jobs":[{"objective":"案内を書く","criteria":["必要事項がある"]},{"objective":"GitHubでreviewする","criteria":["review結果がある"],"display":{"icon":"github"}}]}}
```

変更には`actor`・`reason`が必要です。Task・Ticket・Jobのupdateとstatus.setでは、実行由来なら`run`に現行Run IDを指定し、actorを固定の`[worker_id, session_id]`にします。省略・別Session・終了Session・現在Job以外・終了済みRunからの更新を拒否します。

管理経路は`mode:"management"`を明示し、actorに許可された管理主体を1件指定します。許可主体はCore起動時の`TTJCORE_MANAGEMENT_ACTORS`（JSON string配列）で設定し、既定では管理権限を付与しません。例えばPowerShellでは`$env:TTJCORE_MANAGEMENT_ACTORS = '["operator"]'`を設定してCoreを起動します。設定変更時はCoreを再起動します。ライブラリでは`Store::with_management_actors`で同じ許可主体を設定できます。認証は規格どおり外部環境の責務で、Core profileのIPC credentialは信頼された呼び出し元だけに渡し、gatewayは認証済み主体をactorへ対応付けます。modeや任意のactor名の指定だけでは管理権限を得られません。管理経路では実行元を表す`run`を受け付けません。

Taskの範囲・期限、Ticketのdepends、Sessionのendedの変更とdocument.importは管理経路を使います。Sessionの終了は取り消せず、同じSessionの一時切断・再接続ではendedを設定しません。`task.purge`はreasonを省略でき、削除操作として自動記録します。`operation`は省略すると生成しますが、再送するclientは事前に固有の値を指定してください。同じoperationと同じrequestの再送は、生成済みIDを含む同じ結果を返します。同じoperationで内容を変えるとconflictになります。物理削除の対象となった過去の操作は失効し、再送するとconflictになります。

update・delete・status.set・Runの操作は直前のgetまたはsnapshotからrevisionを取得してください。conflict時は最新のrecordを読み、変更内容を判断し直します。

| record | updateできるfield |
|---|---|
| Project | name |
| Task | objective, criteria, deadline, resume, handoff |
| Ticket | depends, refs, artifacts, resume |
| Job | result, artifacts, resume |
| Knowledge | status |
| Worker | name |
| Session | ended |

completed・canceledのrecordは変更できません。Ticketのdependsは実行開始後に変更できません。patchの未指定fieldは維持します。deadline・resume・handoff・Job.result・Session.endedをnullにするとfieldを除去し、空の配列は`[]`で指定します。

削除は論理削除です。配下のTask・Ticket・Jobがすべてcompletedまたはcanceledである必要があり、履歴・ID・dependsの参照は残します。Jobは所属Ticketと一緒に削除し、Runの個別削除はできません。Task・Ticketを削除してもProjectのKnowledgeは残ります。

### Taskの物理削除と容量回収

`task.purge`はTask自身と配下のTicket・Jobがすべてcompletedまたはcanceledで、有効なRunがない場合に使います。対象Taskと全Ticket・Job・Runを同じtransactionで物理削除します。他TaskのTicketからdepends・refs・Run.wait.targetで参照されている場合は、削除済みのTicketからの参照も含めて拒否します。ProjectのKnowledgeは削除対象に含みません。

```json
{"actor":["operator"],"operation":"example-task-purge","id":"<task-id>","revision":7}
```

履歴は対象名・日時・操作者・理由・状態変化などの要約を残し、criteria・instructions・resultなどの詳しい内容を除きます。保持が必要な詳しい内容は、日々の作業でProjectのKnowledgeへ記録してください。対象の過去の再送request・responseは失効記録へ置き換え、古い再送による再作成を防ぎます。過去のIDは再利用しません。

responseの`value`は`id, deleted:true, physical:true, records, storage`です。`records`は削除したrecord数です。SQLiteではcommit後にVACUUMとWALの回収を試み、`storage`を成功時は`reclaimed`、回収未完了なら`pending`にします。`pending`でもデータの削除はcommit済みです。同じpurge操作の再送は削除を重ねず、容量回収を再試行します。容量回収の結果だけは再送時に変わる場合があります。PostgreSQLでは`reusable`を返し、削除領域はDB側の再利用対象になります。DBファイルを縮める処理は行いません。

既存の論理削除済みTaskも、`task.get`へ`include_deleted:true`を付けて最新revisionを取得すれば`task.purge`で整理できます。通常の一覧からは表示されません。物理削除後は`include_deleted:true`でもrecordを取得できず、要約履歴だけをtimelineから参照します。

Knowledgeの`content`・`basis`を訂正する場合は、新しい`knowledge.create`の`supersedes`に旧IDを指定します。同じProjectのactiveなKnowledgeが対象です。旧recordはsupersedとなり、content・basisを保持します。activeなKnowledgeは削除できません。

## Runとstatus

statusは`todo`・`doing`・`waiting`・`paused`・`blocked`・`completed`・`canceled`です。

`run.start`にJobのrevisionと実際のactorを渡します。Runのidは省略すると自動発行します。`run.finish`に返されたRunのid・revisionとresultを渡します。Runの終了だけではJobはcompletedになりません。

Job・Taskをcompletedにするには、status.setに`criteria_met:true`と確認根拠のresultが必要です。Jobは先行Jobとdependsを満たし、開始済みでactiveなRunが残っていない必要があります。Ticketは全Jobがcompletedまたはcanceledになるとcompletedへ集約され、Ticket自身のcanceledを優先します。Taskは少なくとも1つのTicketを持ち、全Ticketがcompletedまたはcanceledである必要があります。

| option | 扱い |
|---|---|
| release:true | pausedを解除する際に指定 |
| resolved:true | waiting・blockedの原因を解消したことを確認して指定 |
| criteria_met:true, result | Job・Taskをcompletedにする際に指定 |
| run | 実行由来のTask・Ticket・Job変更で必須。有効なRunとTicketの固定worker_id・session_idを検証。終了・失効したRunは使用不可 |
| retry_of, safety | run.startで再試行する場合の閉じた元Runと停止・安全確認内容。Ticket内に過去Runがある開始ではsafetyが必要 |

同じTicketの別Jobの同時実行、未完了のdependsを飛ばす開始、completed・canceledからの復帰を拒否します。同一Ticketで開いたRunは最大1件です。run.start・run.finish・run.waitのactorは`["<worker-id>","<session-id>"]`とし、固定された担当と一致させます。

CoreはRunとDB内の更新権を管理します。外部processの起動・監視・OS上の停止は行いません。停止要求はevent・constraintsに記録されるため、実行側が実際の停止を確認して記録してください。

## 終了後の外部証拠

`job.accept_evidence`は実行終了後に届く承認などを取り込む専用経路です。`id, revision`は現在Job、`evidence_run, run_revision`はそのJobの閉じたRunを指します。`evidence`に待機条件・成果との照合内容を記録し、未解決waitがあれば`wait_met:true`を必須とします。`status`は`doing`・`todo`・`completed`で、completedには`criteria_met:true`と`result`も必要です。固定の適格Session（actorはworker_id/session_id）または許可された管理主体（mode=management）のみが使えます。管理経路にあるrun禁止は、この専用経路でも維持します。evidence_runは証拠の対象参照であり、旧Runによる実行権ではありません。

対象Ticketがpaused・blocked・終端、親Taskがpaused・終端、現在Jobがwaiting/doing以外、または有効Runが残る場合は取り込みを拒否します。照合済みのRun.wait.resolvedとJobの待機解除・完了、Ticketの集約、監査記録を同じtransactionで確定します。どのガードに失敗しても一切変更しません。Runのresult・endedは変更せず、終了済みRunの一般更新も引き続き拒否します。解決したwaitは依存グラフから外し、正式な履歴として保持します。

## TTJファイルの入出力

`document.export`はoptionalの`project`（Project ID）または`tickets`（Ticket ID配列）で絞り、規格の5キーだけを持つTTJ文書を返します。単一Projectとルートtickets、その担当Worker/Sessionを含めます。`document.import`には`mode:"management", actor, reason, documents`を指定し、documentsにTTJ文書の配列を渡します。分割ファイルを結合して検証し、全レコードと履歴を一括保存します。既存の同一Worker/Sessionは共有でき、ほかの既存IDの上書きは拒否します。Ticketファイルだけを追加する場合は対応Task・Worker・Sessionを先に登録してください。

```sh
ttjcore export <project-id> --file combined.json
ttjcore export <project-id> --directory exported
ttjcore import --file exported/project.json --file exported/tickets/<ticket-id>.json --actor '["operator"]' --reason 'Import verified records'
```

分割時は`project.json`と`tickets/<ticket-id>.json`を出力します。出力先は空のディレクトリに限り、非空の出力先・同時書き込みを拒否するため、既存Ticketファイルと新しいスナップショットを混在させません。exportは単一のDBスナップショットを読みます。論理削除済み・別Project・選択外など、文書に含まれない完了依存先を持つ場合は明示的に拒否します。

Job内のRun.waitはpaused中も保存し、importで未解決待機ガードを復元します。pausedの背後に隠れたTask待機・障害など、規格の5キーと既存フィールドでは復元不能なガードがある場合、exportは情報を落とした成功を返さず拒否します。元データ側で保持中の制約を解消するか、復元可能な状態へ戻してから再度exportします。新たな制御フィールドをTTJ JSONへ追加しません。

未終了Runのimportでは、現在Jobへの所属・全先行Jobの終端・dependsの全完了・適格な固定Sessionを検証します。paused/waiting/blockedの既存試行の復元は許可しますが、そのまま新しい実行を開始することはできません。再開時には残存ガードと安全確認を評価します。空配列は省略し、読み込み時は空配列へ戻します。Task.project・Task.tickets・Runの逆参照を出力しません。`project.snapshot`はUI用のwrapper・timeline・依存先を含む参照APIで、ファイル保存にはdocument.exportを使います。1.0.0のDBとJSONは自動変換せず、異なるprotocolとして拒否します。

`run.wait`のparamsは`actor, reason, id, revision, wait`です。waitは単一オブジェクトで、更新で省略した内部フィールドは維持し、target・condition・since・resume_point・timeout・resolvedを保持します。別Ticketの完了待ちはtargetにTicket IDを指定します。未解決の待機はpaused・blocked中もdependsと同じグラフで循環検証し、resolved日時のある待機は履歴に残して循環辺から除きます。待機解除前にRunが終了した場合はjob.accept_evidenceで確認可能な応答を取り込みます。終了済みRunをrun.waitや実行元run指定で直接更新することはできません。

Ticket中止ではstatus.setにcancel_reasonが必須です。Task中止では理由を子Ticketのcancel_reasonとして記録します。中止は未終端の子孫へ反映し、有効Runにresult=canceledを記録して閉じ、停止要求を残します。後続の開始には実行側による停止・安全確認が必要です。

## Event

LLMやagentはCoreから直接eventを取得します。UIを経由しません。

| method | 動作 |
|---|---|
| events.list | cursorより後のeventを即時取得 |
| events.wait | eventまたはtimeout_msまで待機。既定20秒、最大25秒 |
| events.subscribe | rpc・IPC接続を維持し、ttj.changed notificationを受信 |

optionalのprojectで絞り込めます。cursor省略時は現在位置から開始します。返されたcursorはProjectの絞り込みにかかわらず全体の位置です。そのまま次の取得に渡してください。timeout時はeventsが空になります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.1.0","id":"watch-1","method":"events.subscribe","params":{"project":"<project-id>","cursor":0}}
```

最初のresponseに未取得のeventを返し、その後は同じ接続へnotificationを送ります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.1.0","method":"ttj.changed","params":{"cursor":42,"events":[{"id":42,"project":"<project-id>","target":"<job-id>","event":"status.changed","operation":"example-complete","revision":3,"timestamp":"2026-01-01T00:00:00Z","status":"completed"}]}}
```

eventはcommit済みの履歴を参照します。同じoperationの再送では新しいeventを作りません。切断後は最後に受け取ったcursorから再接続します。クライアントはevents.waitまたはevents.subscribeで変更を監視し、必要に応じてsnapshotを更新します。

eventの種類は`record.created`・`record.updated`・`record.deleted`・`status.changed`・`record.imported`・`run.wait_resolved`・`evidence.accepted`・`run.wait_changed`・`run.started`・`run.finished`・`execution.stop_requested`・`knowledge.superseded`です。eventは変更の概要なので、詳細はgetまたはsnapshotで取得します。Runへのexecution.stop_requestedを受けた場合は、run.getのconstraintsにあるstop_requestedを確認し、実行側で停止に対応してください。

実行するagentはrun.start・run.finish・status.setで状態を記録します。CoreはCodex・Claude Codeのprocessを自動で検出・操作しません。

## Display

create・updateのトップレベルにdisplayを指定できます。summaryはカードの概要、iconは任意のicon keyです。ticket.createの各Job定義にもdisplay.iconを指定できます。TTJのrecord本体から分離して保存し、wrapperのdisplayと履歴に返します。

```json
{"mode":"management","actor":["operator"],"reason":"summaryを更新する","operation":"example-display-update","id":"<ticket-id>","revision":1,"patch":{},"display":{"summary":"案内を確認する"}}
```

## Error

| code | 意味 |
|---|---|
| -32700 | JSONを解釈できない |
| -32600 | JSON-RPC requestが不正 |
| -32601 | methodが存在しない |
| -32602 | fieldまたはprotocolが不正 |
| -32004 | recordが見つからない |
| -32009 | revisionまたはoperationのconflict |
| -32010 | TTJの規則による拒否 |
| -32000 | DB操作の失敗 |
| -32001 | IPC認証の失敗 |

CLIのcallはRPC error時にexit code 2、起動・接続などの失敗時に1を返します。
