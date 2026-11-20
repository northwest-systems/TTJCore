# TTJ JSON参照形式 v1.0.0

- [定義] 以下は項目ごとの記入方法と例であり、ラベルはその行だけに適用する。
- [必須] 作業規則は [規格本文](specification.md) に従う。
- [必須] `protocol: string`：独立したJSON文書のルートに `"TTJ/1.0.0"` を記載する。
- [定義] `protocol` の値は規格名と版を1項目で識別する。
- [定義] 同じ文書の子レコードは同じ版として扱い、各レコードへの転記は不要。
- [任意] `protocol` は生成側で補完し、以下の項目一覧では共通の記載を繰り返さない。
- [任意] 各レコードを別ファイル・別形式で保存する。
- [例] ID・名称・担当・時刻はすべて架空の値。
- [定義] `objective` はTask・Ticket・Jobすべてで「何を達成したいか」を表す。
- [定義] `criteria` はTask・Jobの完了を確認する条件の一覧。
- [必須] 新規作成時の作業状態は、未指定でも `todo`。
- [定義] 任意の一覧を省略した新規レコードは、その一覧を空として扱う。
  - [定義] Jobの `assignee` は例外とし、第4節の担当継承に従う。
- [必須] 更新で省略した項目は既存値を維持し、空にする場合は明示する。
- [定義] `project`・`task`・`job`・`target` 等の参照フィールドは対象のIDを表す。
- [推奨] フィールド名はできるだけ1単語とし、同じ意味の別名を増やさない。

## 1. Project

- [必須] `id: string`：Projectを一意に参照する識別値。
- [必須] `name: string`：何の領域か分かる名前。
- [推奨] IDにも対象を判別できる短い識別値を使う。
- [任意] `knowledge`：長期知識がある場合だけ記載する。
- [任意] リポジトリ等の追加情報は必要な場合だけ保持する。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "study-meetup",
  "name": "勉強会の運営"
}
```

## 2. Task

- [必須] `id: string`：Taskの識別値。
- [必須] `project: string`：所属するProject。
- [必須] `objective: string`：達成したい目標。
- [必須] `criteria: array<string>`：Task全体の完了を確認する条件を1件以上書く。
- [任意] `deadline: string`：期限があるTaskだけに完了期限を書く。
- [必須] `deadline` を記載する場合は、タイムゾーン付きのISO 8601日時にする。
- [定義] `deadline` を省略したTaskは期限未設定として扱う。
- [禁止] 期限をTicket・Jobへ重複記入させる。
- [定義] 期限を過ぎても、それだけで作業状態は変更しない。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "prepare-invitation",
  "project": "study-meetup",
  "objective": "勉強会の案内を用意する",
  "criteria": [
    "日時・会場・申込方法が正しい案内文がある"
  ],
  "deadline": "2026-01-08T18:00:00+09:00"
}
```

## 3. Ticket

- [必須] `id: string`：Ticketの識別値。
- [必須] `task: string`：所属するTask。
- [必須] `objective: string`：達成したい目標。
- [必須] `jobs: array<string>`：JobのIDを実行順に1件以上並べる。
- [任意] `assignee: array<string>`：既定の担当一覧。
- [任意] `depends: array<string>`：前提となるTicketのID一覧。
- [任意] `handoffs: array<string>`：短期の申し送り。
- [禁止] Ticket独自の `criteria` を追加する。
- [必須] Ticketと全Jobの定義は正式作成時に一括確定する。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "create-invitation",
  "task": "prepare-invitation",
  "objective": "案内文を完成させる",
  "jobs": [
    "write-invitation",
    "review-invitation"
  ]
}
```

## 4. Job

- [必須] `id: string`：Jobの識別値。
- [必須] `objective: string`：達成したい目標。
- [必須] `criteria: array<string>`：Jobの完了を確認する条件を1件以上書く。
- [任意] `instructions: string`：目標だけでは手順が伝わらない場合だけ補う。
- [任意] `assignee: array<string>`：Job固有の担当がある場合だけ指定する。
- [定義] 担当を省略したJobはTicketの担当を使う。
- [定義] 親と順序はTicketの `jobs` から分かるため、再記入しない。
- [例] 次は前節の2つのJobを別レコードとして表す。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "write-invitation",
  "objective": "必要事項を揃えた案内文を作る",
  "criteria": [
    "日時・会場・申込方法が記載されている"
  ]
}
```

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "review-invitation",
  "objective": "案内文の記載が正しいことを確認する",
  "criteria": [
    "指定した3者全員が元の情報と照合し、確認結果を残している"
  ],
  "assignee": [
    "reviewer-one",
    "reviewer-two",
    "reviewer-three"
  ]
}
```

## 5. 複数担当

- [定義] `assignee` は人・LLM・LLMセッション等を区別できる参照の一覧。
- [必須] 担当が1件でも複数件でも一覧にする。
- [任意] 識別値や割り当て方法は運用で決める。
- [禁止] 担当の種別やセッションの対応表を追加要求する。
- [任意] 同じJobの複数担当は共同で作業する。
- [必須] 必要人数や全員の確認は `criteria` に書き、実際の結果で検証する。
- [禁止] 担当の割り当てだけでレビュー完了とみなす。
- [例] 次はTicketの既定担当をLLMセッションの識別値にした抜粋。
- [例] 前節のレビューJobは既定担当を使わず、指定した3者で実施する。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "create-invitation",
  "assignee": [
    "llm-drafting-context"
  ]
}
```

## 6. 終了判定

- [必須] Jobは自身の `criteria` をすべて満たしたら `completed`、中止を判断したら `canceled` にする。
- [必須] 先行Jobが `completed` または `canceled` なら、後続Jobを開始できる。
- [必須] Ticketは全Jobが `completed` または `canceled` になれば `completed` にする。
- [必須] Ticket自身がキャンセル済みなら `canceled` を維持する。
- [定義] 全Jobが中止でもTicketの処理は終了し、目標達成を保証しない。
- [必須] Taskの目標達成はTask自身の `criteria` と成果から確認する。
- [例] 次はレビューJobを中止してTicketの処理が終了した状態の抜粋。

```json
{
  "protocol": "TTJ/1.0.0",
  "ticket": {
    "id": "create-invitation",
    "status": "completed"
  },
  "jobs": [
    {
      "id": "write-invitation",
      "status": "completed"
    },
    {
      "id": "review-invitation",
      "status": "canceled"
    }
  ]
}
```

## 7. 依存と再開条件

- [任意] `depends` に、先に処理を終える必要があるTicketを指定する。
- [必須] 指定した全Ticketが `completed` になるまで開始しない。
- [定義] 前提Ticketが完了していても、必要な成果が存在するかは別途確認する。
- [例] 次は別Taskにも属せる2つのTicketを待つ抜粋。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "publish-invitation",
  "depends": [
    "create-invitation",
    "confirm-distribution"
  ],
  "status": "waiting"
}
```

- [任意] `resume: string` は、参照や申し送りだけでは再開条件が分からない場合だけ書く。
- [定義] 書く内容は「何を確認できれば再開できるか」の1文。
- [禁止] 理由・対象・再開条件へ同じ内容を重複記入させる。
- [必須] 保留中も未解消の課題を保持し、実行再開には明示解除が必要。

```json
{
  "protocol": "TTJ/1.0.0",
  "status": "waiting",
  "resume": "承認者から確認結果を受け取る"
}
```

## 8. 課題への対応

- [必須] 元Ticketは保留中・終端を除いて `blocked` にし、`handoffs` に課題と再開に必要な確認を書く。
- [例] 次は会場情報が未確定で止まったTicketの抜粋。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "create-invitation",
  "status": "blocked",
  "handoffs": [
    "課題：会場情報が未確定のため案内文を作成できない。",
    "再開：確定した会場情報を受け取り、記載可能か確認する。"
  ]
}
```

- [必須] 課題への対応は通常のTicketとして作り、`handoffs` に元TicketのIDと対応内容を書く。
- [禁止] 専用の関係フィールドや分類を追加する。
- [推奨] 対応するTicketに元Ticketの完了待ちを指定しない。
- [例] 次は同じTaskに作成する通常のTicketとJob。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "resolve-venue",
  "task": "prepare-invitation",
  "objective": "案内に必要な会場情報を確定させる",
  "jobs": [
    "confirm-venue"
  ],
  "handoffs": [
    "元Ticket：create-invitation。会場情報が未確定という課題に対応する。",
    "次：会場情報を確定し、元Ticketで再開可能か確認する。"
  ]
}
```

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "confirm-venue",
  "objective": "会場と必要な案内情報を確定する",
  "criteria": [
    "会場と案内に記載する情報を確認できる"
  ]
}
```

- [必須] 対応後は元Ticketで実際の課題解消を確認してから再開する。
- [禁止] 対応するTicketが完了しただけで、元Ticketの課題が解消したとみなす。

## 9. 短期の申し送り

- [任意] `handoffs: array<string>` に再開に必要な内容だけを1行1件で書く。
- [例] 「判明：内容」「課題：内容」「失敗：操作 → 結果。再試行：条件」「次：操作」の形で書ける。
- [禁止] 各行にID・分類・時刻を追加要求する。
- [必須] 条件が変わっていない失敗を根拠なく繰り返さない。
- [定義] Ticket終了後は長期記憶として扱わず、保存・削除方法と保持期限は運用で決める。

```json
{
  "protocol": "TTJ/1.0.0",
  "handoffs": [
    "判明：日時は確認済み。申込方法は再確認が必要。",
    "失敗：過去の申込先を確認 → 廃止済み。再試行：新しい申込先を受領した場合だけ。",
    "次：主催者に現在の申込方法を確認する。"
  ]
}
```

## 10. 長期知識

- [任意] Projectの `knowledge` に再利用する知見を残す。
- [必須] 記録する場合は `content: string` に残す内容を書く。
- [必須] 記録する場合は `basis: string` に根拠・判断理由・出典をまとめる。
- [必須] 元のTask・Ticketがなくても内容と判断経緯を理解できる。
- [必須] 知識は `id` で識別し、採用状態と変更履歴を保持する。
- [定義] 知識の `status` は `active`・`supersed` の2値。
- [必須] 新規記録は未指定でも `active` とし、通常は `active` だけを参照する。
- [定義] 将来の予定も、その予定を現在の知識として保持するため `active`。
- [必須] 採用中の旧知識を置換する場合は `supersedes` に同じProjectの旧知識のIDを書く。
- [必須] 置換時は旧記録の `supersed` と新記録の `active` を同時確定する。
- [必須] 振り返り時は過去の内容・根拠・採用理由も参照できる。
- [必須] 後継がない知識を使わなくなった場合も、理由を履歴に残して `supersed` にする。
- [任意] 内容を変えず再採用する場合は、理由を残して `active` に戻す。
- [例] 次は過去の記録、現在使う記録、現在の予定を表示した例。

```json
{
  "protocol": "TTJ/1.0.0",
  "knowledge": [
    {
      "id": "invitation-short-form",
      "status": "supersed",
      "content": "申込先を中心に短い案内を作る",
      "basis": "初回の案内作成時に、読む量を減らす方針として採用した。"
    },
    {
      "id": "invitation-required-details",
      "status": "active",
      "content": "案内には日時・会場・申込方法をすべて記載する",
      "basis": "初回案内後に日時と会場の質問が相次いだため、必要情報を案内だけで確認できる方針へ変更した。",
      "supersedes": "invitation-short-form"
    },
    {
      "id": "invitation-automation-plan",
      "status": "active",
      "content": "次回は案内配信の自動化を検討する予定。",
      "basis": "準備作業を減らすため、次回の検討事項として記録する。効果と運用負担は今後確認する。"
    }
  ]
}
```

## 11. 結果と管理記録

- [任意] `result: string` に成果と確認結果をまとめる。
- [任意] 既存の実行・変更履歴に同じ情報があれば参照する。
- [禁止] 子の記録で確認できる結果を親へ転記させる。
- [例] 次はレビューJobがRunの確認結果を参照する抜粋。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "review-invitation",
  "status": "completed",
  "result": "invitation-review-first-run"
}
```

- [定義] RunはJobの1回の実行をまとめる記録。
- [必須] `id: string`：実行を識別するID。
- [必須] `job: string`：実行したJobのID。
- [必須] `actor: array<string>`：実際に参加した担当。
- [必須] `started: string`：実際の開始時刻。
- [必須] 終了時には結果を記録し、時刻を確認できる場合は `ended` も残す。
- [任意] `result: string` に結果をまとめ、同じ情報が既存履歴にあれば参照する。
- [定義] `generation` は失効した実行を区別する管理用の世代。
- [必須] 再試行・実行の再起動は新しいRunにし、前の実行の停止または安全条件を確認する。
- [禁止] 同じ操作の再送を新しいRunとして二重登録する。
- [任意] プロセスを監視する場合だけ `activity`・`observed` を同じRunに加える。
- [定義] `activity` は実行プロセスの観測、`observed` は観測時刻。
- [例] 次は実行中のRunであり、計画作成時に埋める欄ではない。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "invitation-draft-first-run",
  "job": "write-invitation",
  "actor": [
    "llm-drafting-context"
  ],
  "started": "2026-01-01T01:00:00Z",
  "generation": 1,
  "activity": "alive",
  "observed": "2026-01-01T01:00:00Z"
}
```

- [例] 次は人間の共同作業を終えたRunであり、プロセス観測を持たない。
- [必須] Runが終わってもJobの完了は `criteria` と結果から別に判定する。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "invitation-review-first-run",
  "job": "review-invitation",
  "actor": [
    "reviewer-one",
    "reviewer-two",
    "reviewer-three"
  ],
  "started": "2026-01-01T02:00:00Z",
  "ended": "2026-01-01T02:30:00Z",
  "result": "3者全員が元の情報との一致を確認し、それぞれの結果を記録した。"
}
```

- [必須] 現在状態と別に、変更前後・主体・時刻・理由・確定世代を追跡する。
- [定義] `id`・`operation` はイベントと一括操作、`target`・`event` は対象のIDと変更の種類。
- [定義] `from`・`to`・`actor`・`timestamp`・`reason`・`revision` は変更前後・主体・時刻・理由・世代。
- [定義] `run`・`generation` は関連する実行の参照。
- [任意] 管理情報は更新手段で補完し、同等の履歴形式を使う。
- [禁止] 補完できない事実を捏造する、または各階層へ手入力させる。

```json
{
  "protocol": "TTJ/1.0.0",
  "id": "invitation-draft-started",
  "operation": "start-invitation-draft",
  "target": "write-invitation",
  "event": "status.changed",
  "from": "todo",
  "to": "doing",
  "actor": [
    "llm-drafting-context"
  ],
  "timestamp": "2026-01-01T01:00:00Z",
  "reason": "開始条件と実行権を確認した",
  "revision": 2,
  "run": "invitation-draft-first-run",
  "generation": 1
}
```
