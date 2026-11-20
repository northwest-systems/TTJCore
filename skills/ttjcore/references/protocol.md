# TTJCore protocol

TTJCore 0.1.0のCLI・JSON-RPC contractです。`protocol`は`TTJ/1.0.0`です。LLMとHerdrはCoreへ直接接続し、この文書と`ttjcore methods`でinterfaceを確認します。

TTJ自体の作業規則は同梱の[TTJ 1.0.0規格本文](ttj-1.0.0/specification.md)、項目と例は[JSON参照形式](ttj-1.0.0/json-format.md)を参照します。いずれもネット接続なしで読めます。

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
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","id":"query-1","method":"project.snapshot","params":{"id":"<project-id>"}}
```

`id`はstring・number・nullを使用できます。通常のrequestで`id`を省略するとnotificationとして処理し、responseを返しません。batchではnotificationを除いたresponseの配列を返します。`events.subscribe`と`core.stop`は`id`付きの単独requestを使ってください。

call・rpcはprofileの接続情報からtokenを読み、IPC requestの`auth`へ自動で付与します。直接IPCへ接続するclientにも`auth`が必要です。接続情報を保存する`connection.json`やtokenを共有・記録しないでください。

responseにはrequestと同じ`protocol`・`id`、および`result`または`error`が入ります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","id":"query-1","error":{"code":-32004,"message":"Record not found"}}
```

## Recordとmethod

Project → Task → Ticket → Jobの階層を持ち、RunはJobに属します。Ticketは1つのTaskに所属します。KnowledgeはProjectに属します。

create入力のfieldは次のとおりです。`id`は共通のoptional fieldで、省略するとCoreが小文字英数字8桁を発行します。Project・Task・Ticket・Jobにはそれぞれ`pj-`・`tsk-`・`tkt-`・`job-`を付けます（例：`tsk-12abcd00`）。Run・Knowledgeには接頭辞を付けません。自動発行で既存・削除済み・同時作成のIDと衝突した場合は再生成します。既存のUUIDや明示指定のIDはそのまま使用でき、削除済みを含む全recordと重複する指定IDは拒否します。

| record | required field | optional field |
|---|---|---|
| Project | name | id |
| Task | project, objective, criteria | id, deadline |
| Ticket | task, objective, jobs | id, assignee, depends, handoffs, resume, priority |
| Job | objective, criteria | id, instructions, assignee |
| Knowledge | project, content, basis | id, supersedes |

Jobは`ticket.create`の`record.jobs`に完全な定義を並べます。作成後のTicketでは`jobs`がID配列になります。Jobの追加・削除・並べ替え、Ticketのobjective、Jobのobjective・criteria・instructionsの変更は拒否します。

| field | 型・扱い |
|---|---|
| actor | 空でないstring配列。実際のparticipantを指定 |
| reason | 空でないstring |
| criteria | 空でないstring配列 |
| assignee | string配列。Jobで省略するとTicketから継承 |
| depends | Ticket ID配列。循環は不可 |
| handoffs | string配列 |
| resume | string |
| deadline | timezone付きRFC 3339 string |
| priority | integer |
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
| `run.start` | `actor, reason, job, revision`。revisionはJobの値 |
| `run.finish` | `actor, reason, id, revision, result`。id・revisionはRunの値 |
| `core.stop` | IPC・rpc経由でCoreを停止。recordの変更とは別の操作 |

`<kind>`は`project`・`task`・`ticket`・`job`・`run`・`knowledge`です。削除済みをget・list・timeline.listで読む場合は`include_deleted:true`を指定します。

record wrapperには`record, revision, deleted, started, active, constraints, display`が入ります。`record`がTTJの内容です。変更responseの`result.operation`がoperation ID、`result.value`が変更結果です。`run.start`のvalueは`run`・`job`のwrapperを含みます。

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
{"actor":["operator"],"reason":"作業計画を登録する","operation":"example-ticket-create","record":{"task":"<task-id>","objective":"案内を完成させる","jobs":[{"objective":"案内を書く","criteria":["必要事項がある"]},{"objective":"GitHubでreviewする","criteria":["review結果がある"],"display":{"icon":"github"}}]}}
```

変更には`actor`・`reason`が必要です。`operation`は省略すると生成しますが、再送するclientは事前に固有の値を指定してください。同じoperationと同じrequestの再送は、生成済みIDを含む同じ結果を返します。同じoperationで内容を変えるとconflictになります。

update・delete・status.set・Runの操作は直前のgetまたはsnapshotからrevisionを取得してください。conflict時は最新のrecordを読み、変更内容を判断し直します。

| record | updateできるfield |
|---|---|
| Project | name |
| Task | objective, criteria, deadline |
| Ticket | assignee, depends, handoffs, resume, priority |
| Job | assignee, resume |
| Knowledge | status |

completed・canceledのrecordは変更できません。Ticketのdependsは実行開始後に変更できません。patchの未指定fieldは維持します。deadline・resume・assigneeをnullにするとfieldを除去し、空の配列は`[]`で指定します。

削除は論理削除です。配下のTask・Ticket・Jobがすべてcompletedまたはcanceledである必要があり、履歴・ID・dependsの参照は残します。Jobは所属Ticketと一緒に削除し、Runの個別削除はできません。Task・Ticketを削除してもProjectのKnowledgeは残ります。

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
| run | Jobの変更時にRunを指定。有効なRun・参加actor・generationを検証。終了・失効したRunは使用不可 |
| retry_of, safety | run.startで再試行する場合の元Runと確認内容。停止未確認のRunがある場合はsafetyが必要 |

同じTicketの別Jobの同時実行、未完了のdependsを飛ばす開始、completed・canceledからの復帰を拒否します。同一Jobにはassigneeごとの共同作業のRunを作成できます。

CoreはRunとDB内の更新権を管理します。外部processの起動・監視・OS上の停止は行いません。停止要求はevent・constraintsに記録されるため、実行側が実際の停止を確認して記録してください。

## Event

LLMやagentはCoreから直接eventを取得します。UIを経由しません。

| method | 動作 |
|---|---|
| events.list | cursorより後のeventを即時取得 |
| events.wait | eventまたはtimeout_msまで待機。既定20秒、最大25秒 |
| events.subscribe | rpc・IPC接続を維持し、ttj.changed notificationを受信 |

optionalのprojectで絞り込めます。cursor省略時は現在位置から開始します。返されたcursorはProjectの絞り込みにかかわらず全体の位置です。そのまま次の取得に渡してください。timeout時はeventsが空になります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","id":"watch-1","method":"events.subscribe","params":{"project":"<project-id>","cursor":0}}
```

最初のresponseに未取得のeventを返し、その後は同じ接続へnotificationを送ります。

```json
{"jsonrpc":"2.0","protocol":"TTJ/1.0.0","method":"ttj.changed","params":{"cursor":42,"events":[{"id":42,"project":"<project-id>","target":"<job-id>","event":"status.changed","operation":"example-complete","revision":3,"timestamp":"2026-01-01T00:00:00Z","status":"completed"}]}}
```

eventはcommit済みの履歴を参照します。同じoperationの再送では新しいeventを作りません。切断後は最後に受け取ったcursorから再接続します。Herdr Modはevents.waitを使い、変更があったときにsnapshotを更新します。

eventの種類は`record.created`・`record.updated`・`record.deleted`・`status.changed`・`run.started`・`run.finished`・`execution.stop_requested`・`knowledge.superseded`です。eventは変更の概要なので、詳細はgetまたはsnapshotで取得します。Runへのexecution.stop_requestedを受けた場合は、run.getのconstraintsにあるstop_requested・revokedを確認し、実行側で停止に対応してください。

実行するagentはrun.start・run.finish・status.setで状態を記録します。CoreはCodex・Claude Codeのprocessを自動で検出・操作しません。

## Display

create・updateのトップレベルにdisplayを指定できます。summaryはカードの概要、iconは任意のicon keyです。ticket.createの各Job定義にもdisplay.iconを指定できます。TTJのrecord本体から分離して保存し、wrapperのdisplayと履歴に返します。

```json
{"actor":["operator"],"reason":"summaryを更新する","operation":"example-display-update","id":"<ticket-id>","revision":1,"patch":{},"display":{"summary":"案内を確認する"}}
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
