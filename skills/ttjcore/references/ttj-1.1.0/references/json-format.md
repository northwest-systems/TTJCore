# TTJ JSON参照形式 v1.1.0

- [定義] 以下は項目ごとの記入方法と例であり、ラベルはその行だけに適用する。
- [必須] 作業規則は [規格本文](specification.md) に従う。
- [必須] **配列と単一オブジェクトの原則、およびルートスキーマ構造（`project.tasks[]` とルート `tickets[].jobs[].runs[]` の分離スキーマ）**：0〜複数があり得る項目（`workers`, `sessions`, `knowledge`, `tasks`, `tickets`, `jobs`, `runs` 等）のみ配列（空配列 `[]`・1件・複数件すべて許容、空配列時は記載省略可）とし、1つだけに定まっている項目（ルートの `project`、`Task.handoff`、`Run.wait`）は配列にせず単一オブジェクトとする。すべての TTJ JSON 文書は以下の単一ルートスキーマに統一する。ルート直下に `tasks`・`jobs`・`runs`・`handoff`・`knowledge` 等を直接配置する抜粋フラット形式や、ルートの `project` を配列 `projects` にする構造、`Task` の内側に `tickets` をネストする構造、または `Ticket.jobs` を ID 文字列配列にして `Job` 実体を外に出す二重管理は行わない。
  - [必須] **Root オブジェクトのキー（以下の5つのみ。配列はいずれも空配列時は記載省略可、単一オブジェクト `project` も未記載時は省略可）**：
    - `protocol`: `"TTJ/1.1.0"`（必須）
    - `workers`: `array<Worker>`（0〜複数あり得るため配列、空配列時は省略可）
    - `sessions`: `array<Session>`（0〜複数あり得るため配列、空配列時は省略可）
    - `project`: `Project`（1つに定まっているため単一オブジェクト。Ticket 分割ファイル等で記載しない場合は省略可。内部に `id`, `name`, `knowledge[]`, `tasks[]`（および `tasks[].handoff`）を持ち、`Task` の中に `tickets` は持たない）
    - `tickets`: `array<Ticket>`（0〜複数あり得るため配列、空配列時は省略可。各 `Ticket` は親 Task を指す `task_id` を必須とし、内部に `jobs[].runs[]` を持つ）
  - [必須] **階層の責務と逆参照の整理**：Project / Task 階層は単一オブジェクト `project` 配下の `project.tasks[]`（および `project.knowledge[]`・`project.tasks[].handoff`）で構成し、親要素の内側に置かれる `Task.project`・`Handoff.task_id` 等の逆参照や `Task` 内の `tickets`・`jobs` は持たない。Ticket 以下の作業階層はルート直下の `tickets[].jobs[].runs[]` に分離し、各 `Ticket` が必ず `task_id`（`tsk-*`）で親 Task を参照する（`Job` や `Run` は親の内側に置かれるため逆参照を持たない）。
- [必須] **省略規則と単一スキーマでのファイル分割**：すべての配列フィールドは、空配列 `[]`・1要素 `[ {...} ]`・複数要素 `[ {...}, {...} ]` のいずれも許容し、空配列 `[]` の場合はそのフィールドの記載自体を省略できる（省略時は空配列 `[]` として扱う）。また、単一オブジェクトの `project` も Ticket 分割ファイル等で記載しない場合は省略できる。この規則により、**100% 同一のルート JSON スキーマ** のまま、チケット単位で別ファイルに分けて排他操作する運用と1ファイルにまとめる運用の双方を扱える。
  - [定義] **Task 側のファイル（Project / Task 管理ファイル）**：`protocol` と単一オブジェクト `project`（必要に応じて `workers`, `sessions`）のみを記載し、`tickets` は空配列として省略するため、`tickets` や `jobs` の記載は一切不要となる。
  - [定義] **Ticket 側のファイル（チケット単位の分割ファイル）**：`protocol` と `tickets`（内部に `jobs[].runs[]`）のみを記載し、`project`, `workers`, `sessions` は省略する。各 Ticket は `task_id` で親 Task に紐づく。
  - [定義] **1ファイルにまとめる場合**：同じルートオブジェクト内に単一オブジェクト `project` と配列 `tickets`（および必要に応じて `workers`, `sessions`）を両方記載する。
- [定義] `protocol` の値は規格名と版を1項目で識別し、同じ文書内の子レコードへの転記は不要。
- [必須] 重複を避けるため、Project・Task・Ticket・Job・Run・Worker・Session（全7種）の `id` は自動採番とし、それぞれ `pj-`・`tsk-`・`tkt-`・`job-`・`run-`・`wkr-`・`ssn-` の接頭辞を用いた `<接頭辞>-<英数字6桁または8桁>-<インクリメント（ゼロ埋めなしの1始まり整数: 1, 2, 3...）>` 形式（正規表現：`^(pj|tsk|tkt|job|run|wkr|ssn)-[A-Za-z0-9]{6}([A-Za-z0-9]{2})?-[1-9][0-9]*$`）に統一する。
- [例] ID・名称・時刻・成果物参照はすべて架空の値。
- [定義] `objective` は Task・Ticket・Job すべてで「何を達成したいか」を表す。
- [定義] `criteria` は Task・Job の完了を確認する条件の一覧。
- [定義] Task・Ticket・Job の `status` は `todo`・`doing`・`waiting`・`paused`・`blocked`・`completed`・`canceled` の7値。
- [必須] 新規作成時の作業状態は、未指定でも `todo`。
- [必須] 更新で省略した項目は既存値を維持し、空にする場合は明示する。

## 1. Worker（`workers[]` の要素）

- [必須] `id: string`：Worker を一意に識別する不変の自動採番 ID（`wkr-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `name: string`：変更可能な表示名。
- [禁止] `owner_type` や `human`・`bot`・`agent`・`ai`・`service` 等の種別分類を必須項目として追加する。
- [定義] 認証情報や実行環境設定は外部環境側で管理する。

```json
{
  "protocol": "TTJ/1.1.0",
  "workers": [
    {
      "id": "wkr-7c8d9e0f-1",
      "name": "Alpha"
    }
  ]
}
```

## 2. Session（`sessions[]` の要素）

- [必須] `id: string`：Session を一意に識別する自動採番 ID（`ssn-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `worker_id: string`：この Session が属する `Worker.id`（`wkr-*`）。
- [任意] `started: string`：開始時刻（タイムゾーン付き ISO 8601）。
- [任意] `ended: string`：終了時刻（終了済みの場合）。
- [定義] 同じ Worker が複数 Session を持て、各 Session を個別に識別する。

```json
{
  "protocol": "TTJ/1.1.0",
  "sessions": [
    {
      "id": "ssn-2b3c4d5e-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "started": "2026-01-01T01:00:00Z"
    }
  ]
}
```

## 3. Project（ルート単一オブジェクト `project`）

- [必須] `id: string`：Project を一意に参照する自動採番 ID（`pj-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `name: string`：何の領域か分かる名前（ID は自動採番とし、人間が判別しやすい名称は `name` に記載する）。
- [任意] `knowledge: array<Knowledge>`：長期知識の配列（空配列時は省略可）。
- [任意] `tasks: array<Task>`：Project に属する Task の配列（空配列時は省略可）。
- [任意] リポジトリ等の追加情報は必要な場合だけ保持する。

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営"
  }
}
```

## 4. Task（`project.tasks[]` の要素）

- [必須] `id: string`：Task の自動採番 ID（`tsk-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `objective: string`：長寿命の成果目標。
- [必須] `criteria: array<string>`：Task 全体の完了を確認する条件を1件以上書く。
- [任意] `status: string`：作業状態（`todo`・`doing`・`waiting`・`paused`・`blocked`・`completed`・`canceled`。新規作成時の未指定は `todo`）。Run を持たない Task の計画内待機にも `waiting` を用いる。
- [任意] `deadline: string`：期限がある Task だけに完了期限（タイムゾーン付き ISO 8601）を書く。省略時は期限未設定として扱い、期限を過ぎてもそれだけで作業状態は変更しない。
- [任意] `resume: string`：永続正本（`depends`・`Run.wait`・管理履歴）から再開・解除条件が一意に分からない場合に1文で記載する（揮発性 `handoff` への記載を理由に省略しない）。
- [任意] `handoff: Handoff | null`：Task 所属の揮発性単一スロット（未設定時は省略可）。
- [定義] `tickets` はルート直下の `tickets[]` に分離されたため、`Task` オブジェクト内には `tickets`（および `jobs`）を持たない。また、`Task` は `Project.tasks[]` の内側に配置されるため、逆参照の `Task.project` フィールドも持たない。
- [禁止] `Task` 内に `tickets` や `jobs` を持たせる、または期限を Ticket・Job へ重複記入させる。
- [定義] 修正・再実行により Ticket が増えても、同一 Task（同じ `task_id`）に紐づけて管理する。

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "勉強会の案内を用意する",
        "criteria": [
          "日時・会場・申込方法が正しい案内文が確定している"
        ],
        "deadline": "2026-01-08T18:00:00+09:00"
      }
    ]
  }
}
```

## 5. Ticket（ルート `tickets[]` の要素）とファイル分割サンプル

- [必須] `id: string`：Ticket の自動採番 ID（`tkt-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `task_id: string`：所属する親 Task の ID（`tsk-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `worker_id: string`：担当する単一の `Worker.id`（`wkr-*`）。確定後の変更は禁止。
- [必須] `session_id: string`：担当する単一の `Session.id`（`ssn-*`）。確定後の変更は禁止。
- [必須] `objective: string`：達成したい目標。
- [任意] `exclusions: array<string>`：その Ticket でやらないこと・除外事項（空配列時は省略可）。LLM や担当者間の認識すり合わせ精度を上げるために記載する（`Job` には置かない）。
- [任意] `status: string`：作業状態（`todo`・`doing`・`waiting`・`paused`・`blocked`・`completed`・`canceled`。新規作成時の未指定は `todo`）。
- [任意] `depends: array<string>`：実行依存（先に `completed` になる必要がある前提 Ticket の ID `tkt-*` の配列、空配列時は省略可）。循環依存は禁止。
- [任意] `refs: array<Ref>`：情報共有用の参照オブジェクト配列（実行依存 `depends` とは明確に区別する。空配列時は省略可）。
- [任意] `artifacts: array<string>`：成果物・証拠の参照一覧（空配列時は省略可）。
- [必須] `cancel_reason: string`（条件付き必須）：Ticket を `status: "canceled"` にする場合は中止理由の記録を必須とする（非 `canceled` 時は省略可）。
- [任意] `resume: string`：永続正本（`depends`・`Run.wait`・管理履歴）から再開・障害解除条件が一意に分からない場合に1文で記載する（揮発性 `handoff` への記載を理由に省略しない）。
- [任意] `jobs: array<Job>`：`Job` オブジェクトの配列（ID 文字列配列ではなく `Job` 実体を実行順に格納し、内部に `runs: array<Run>` を内包する。確定後は固定、空配列時は省略可）。
- [禁止] 旧 `assignee`、複数担当、確定後の `worker_id`・`session_id` 変更、Ticket 独自の `criteria`、旧 `Ticket.handoffs`、`Task` 内への `tickets` ネスト、および `Ticket.jobs` を ID 文字列配列にする実体分離。
- [必須] Ticket と全 Job の定義は正式作成時に一括確定する。

### 5.1. ファイル分割する場合のサンプル（Task ファイルと Ticket ファイルの分離）

チケット単位で別ファイルに分けて複数 Worker / Session が排他操作できるようにする場合、省略規則により、**100% 同一のルート JSON スキーマ** のまま以下のように Task 側のファイルと Ticket 側のファイルに分割する。

- [例] **Task 側のファイル（`project.tasks[]` のみで `tickets` / `jobs` の記載なし）**：`protocol` と単一オブジェクト `project`（必要に応じて `workers`, `sessions`）のみを記載し、ルート `tickets` は空配列として省略する。

```json
{
  "protocol": "TTJ/1.1.0",
  "workers": [
    {
      "id": "wkr-7c8d9e0f-1",
      "name": "Alpha"
    }
  ],
  "sessions": [
    {
      "id": "ssn-2b3c4d5e-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "started": "2026-01-01T01:00:00Z"
    }
  ],
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "勉強会の案内を用意する",
        "criteria": [
          "日時・会場・申込方法が正しい案内文が確定している"
        ],
        "status": "todo",
        "deadline": "2026-01-08T18:00:00+09:00"
      }
    ]
  }
}
```

- [例] **Ticket 側のファイル（`tickets[].jobs[].runs[]` のみで `task_id` 付き）**：`protocol` とルート `tickets` のみを記載し、`project`, `workers`, `sessions` は省略する。Ticket は `task_id`（`"tsk-e5f6a7b8-1"`）で親 Task に紐づく（未実行 Job の `runs` は空配列のため省略）。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "案内文の初稿を作成し確認準備を整える",
      "exclusions": [
        "参加者への案内メールの本送信は行わない",
        "申込フォームの外部公開設定は変更しない"
      ],
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "必要事項を揃えた案内文を作る",
          "criteria": [
            "日時・会場・申込方法が記載された案内文ドラフトが保存されている"
          ]
        },
        {
          "id": "job-1a2b3c4d-2",
          "objective": "案内文のリンクと開催日時を点検する",
          "criteria": [
            "リンク切れがなく開催要領と日時が一致している"
          ]
        }
      ]
    }
  ]
}
```

### 5.2. 1ファイルにまとめる場合のサンプル（`project` と `tickets` を両方記載）

- [例] 1つのファイルにまとめる場合もまったく同じルートスキーマに従い、同じルートオブジェクト内に単一オブジェクト `project`（`project.tasks[]`）と配列 `tickets`（`tickets[].jobs[].runs[]`）を両方記載する。

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "勉強会の案内を用意する",
        "criteria": [
          "日時・会場・申込方法が正しい案内文が確定している"
        ]
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "案内文の初稿を作成し確認準備を整える",
      "exclusions": [
        "参加者への案内メールの本送信は行わない",
        "申込フォームの外部公開設定は変更しない"
      ],
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "必要事項を揃えた案内文を作る",
          "criteria": [
            "日時・会場・申込方法が記載された案内文ドラフトが保存されている"
          ]
        },
        {
          "id": "job-1a2b3c4d-2",
          "objective": "案内文のリンクと開催日時を点検する",
          "criteria": [
            "リンク切れがなく開催要領と日時が一致している"
          ]
        }
      ]
    }
  ]
}
```

## 6. Job と status / result 分離（`tickets[].jobs[]` の要素）

- [必須] `id: string`：Job の自動採番 ID（`job-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `objective: string`：達成したい目標。
- [必須] `criteria: array<string>`：Job の完了可否を確認可能な証拠で判断できる条件を1件以上書く。
- [任意] `instructions: string`：目標だけでは手順が伝わらない場合だけ補う（v1.0.0 の項目名を維持）。
- [任意] `status: string`：進行状態（`todo`・`doing`・`waiting`・`paused`・`blocked`・`completed`・`canceled`。未指定時は `todo`）。
- [任意] `result: string`：`status` と分離された確認結果・業務判定（例：`passed`・`approved`・`changes_requested`・`failed`・`interrupted`・`error` 等）。
- [任意] `artifacts: array<string>`：成果物・確認証拠の参照一覧（空配列時は省略可）。
- [任意] `resume: string`：永続正本（`Run.wait`・管理履歴）から再開・障害解除条件が一意に分からない場合に1文で記載する（揮発性 `handoff` への記載を理由に省略しない）。
- [任意] `runs: array<Run>`：実行試行（`Run`）オブジェクトの配列（未実行等の空配列時は `[]` または記載省略）。`Run` を `Job` の外に出さない。
- [定義] 親 Ticket と実行順序は `Ticket.jobs` 内の位置から分かるため、親 ID や順序番号を再記入しない。
- [禁止] Job ごとの個別担当、複雑な分岐・子 Job・待機制御詳細、独立 `Request` オブジェクト、種別分類を追加する。
- [例] 次は作成時の通常 Job（未実行のため `runs` は省略、Ticket ファイル形式で表記）。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "案内文の初稿を作成する",
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "必要事項を揃えた案内文を作る",
          "instructions": "前回の開催要領と最新の会場情報を照合して下書きを作成する。",
          "criteria": [
            "日時・会場・申込方法が記載された案内文ドラフトが保存されている"
          ]
        }
      ]
    }
  ]
}
```

- [例] 次は別 Worker（`wkr-7c8d9e0f-2`）が担当するレビュー Ticket 内のレビュー Job が、差し戻し判定で完了した例。`status`（完了）と `result`（差し戻し）を分離して表現する。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-2",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "案内文の初稿をレビューする",
      "status": "completed",
      "jobs": [
        {
          "id": "job-1a2b3c4d-3",
          "objective": "案内文の記載内容を確認しレビュー結果を返す",
          "criteria": [
            "元の開催情報との照合結果と判定が記録されている"
          ],
          "status": "completed",
          "result": "changes_requested",
          "artifacts": [
            "review-notes/invitation-rev1.md"
          ],
          "runs": [
            {
              "id": "run-5e6f7a8b-3",
              "started": "2026-01-01T02:00:00Z",
              "ended": "2026-01-01T02:15:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

- [例] 次はテスト実行 Job が実行自体を完了し、テスト結果として失敗を記録した例。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-6",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "案内ページの公開前検証を実施する",
      "status": "completed",
      "jobs": [
        {
          "id": "job-1a2b3c4d-4",
          "objective": "公開前検証を実行し結果を記録する",
          "criteria": [
            "全検証コマンドの実行ログと合否判定が保存されている"
          ],
          "status": "completed",
          "result": "failed",
          "artifacts": [
            "logs/release-checks-01.log"
          ],
          "runs": [
            {
              "id": "run-5e6f7a8b-4",
              "started": "2026-01-01T02:20:00Z",
              "ended": "2026-01-01T02:25:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

## 7. 終了判定・中止と新 Ticket による再実行

- [必須] Job は確認可能な証拠に基づき `criteria` をすべて満たしたら `completed`、中止を判断したら `canceled` にする。Job を単独で `canceled` にする場合は、実行中 Run があれば同時に終了（`result: "canceled"` 等を記録して閉鎖）し、停止要求を発行する。
- [必須] 後続 Job を開始するには、先行 Job がすべて `completed` または `canceled` であることに加え、[規格本文](specification.md) 第6節・第8節の開始条件をすべて満たす必要がある：前提 Ticket が `completed`、対象 Ticket および現在 Job が `todo` または `doing`（`paused`・`blocked`・`waiting`・終端でないこと）、親 Task（`Ticket.task_id` が指す Task）が `paused`・終端でないこと（親 Task が別 Ticket 起因で `waiting`・`blocked` であっても独立した実行可能 Ticket・Job は開始できる）、対象 Ticket の `worker_id`・`session_id` の一致、および先行 Job に対する停止確認または安全条件の確認。
- [必須] Ticket は全 Job が `completed` または `canceled` になれば `completed` にする。
- [必須] Ticket 自身がキャンセル済みなら `canceled` を維持し、中止理由（`cancel_reason`）を必須保持する。
- [定義] 次 Ticket が未作成でも、終了済み Ticket はアーカイブ・折りたたみできる。
- [例] 次は複合スクリプトの異常終了により Ticket を `canceled` にした例。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-4",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "ステージング環境へ案内ページを反映する",
      "status": "canceled",
      "cancel_reason": "ステージング反映スクリプトがタイムアウトし環境整合を確認できなかったため中止。復旧確認後に新Ticketで再実行する。",
      "artifacts": [
        "logs/deploy-staging-01.log"
      ],
      "jobs": [
        {
          "id": "job-1a2b3c4d-5",
          "objective": "ステージング用アセットをビルドする",
          "criteria": [
            "ビルド成果物が生成されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-5e6f7a8b-5",
              "started": "2026-01-01T00:50:00Z",
              "ended": "2026-01-01T00:59:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-1a2b3c4d-6",
          "objective": "ステージング環境へ反映スクリプトを実行する",
          "criteria": [
            "ステージング環境への反映と疎通確認が完了している"
          ],
          "status": "canceled",
          "result": "error",
          "runs": [
            {
              "id": "run-5e6f7a8b-2",
              "started": "2026-01-01T01:00:00Z",
              "ended": "2026-01-01T02:00:00Z",
              "result": "error"
            }
          ]
        }
      ]
    }
  ]
}
```

## 8. 実行依存（depends）と情報共有参照（refs）

- [任意] `depends: array<string>` に、先に `completed` になる必要がある前提 Ticket の ID（`tkt-*`）を指定する（空配列時は省略可）。
- [必須] `depends` に循環依存・自己依存・重複参照を含めない（表示状態が `waiting`・`paused`・`blocked` 等のいずれであっても、非終端 Ticket・Job が保持する未解決 `Run.wait` と混在する循環も禁止。解消済み・解除済みの `Run.wait` は履歴として残しつつ依存グラフの辺からは除外する）。
- [任意] `refs: array<Ref>` に、前 Ticket の成果物・結果・中止理由、派生元・修正対象・再実行元などを情報共有専用として記録する（空配列時は省略可）。
- [必須] `refs` の各要素は参照先 `ticket_id: string`（`tkt-*`）等を持ち、任意で `kind: string`（例：`derived_from`・`fixes`・`rerun_of`・`related`）と `note: string` を付けられる。
- [禁止] `refs` を実行依存や状態遷移のゲート条件として扱う。
- [例] 次は差し戻し結果を参照して新 Session で修正する新 Ticket の例。`depends` は前提 Ticket の完了待ち、`refs` は情報共有参照として分離している。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-3",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "レビュー指摘を反映して案内文を修正する",
      "status": "todo",
      "depends": [
        "tkt-c9d0e1f2-2"
      ],
      "refs": [
        {
          "ticket_id": "tkt-c9d0e1f2-1",
          "kind": "fixes",
          "note": "初稿ドラフト成果物を引き継ぐ"
        },
        {
          "ticket_id": "tkt-c9d0e1f2-2",
          "kind": "derived_from",
          "note": "changes_requested の指摘メモを参照"
        }
      ],
      "jobs": [
        {
          "id": "job-1a2b3c4d-7",
          "objective": "指摘された会場アクセス表記を修正する",
          "criteria": [
            "最新の会場案内図と所要時間が案内文に反映されている"
          ]
        },
        {
          "id": "job-1a2b3c4d-8",
          "objective": "修正後の案内文全体の整合性を確認する",
          "criteria": [
            "全指摘事項の解消が確認されている"
          ]
        }
      ]
    }
  ]
}
```

- [任意] `resume: string` は、Task・Ticket・Job において、永続正本（`depends`・`Run.wait`・管理履歴）だけでは再開・障害解除条件が一意に分からない場合に1文で記載する（揮発性 `handoff` のみに再開条件を残すことは禁止）。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-5",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "会場使用許可を反映して案内文を確定する",
      "status": "waiting",
      "resume": "会場管理者から最終使用許可の回答を受け取る"
    }
  ]
}
```

## 9. Run（`tickets[].jobs[].runs[]` の要素）

- [定義] `Run` は Job の1回の実行試行を記録する正本。各 `Job` オブジェクトの内部フィールド `runs`（未実行等の空配列時は `[]` または記載省略）として保持し、実行中であれば `result`（および `ended`）がまだ無く、終了確定時に `result`（および確認できれば `ended`）を記録する。
- [禁止] `Run` を `Job` の外に配置する、または `Run` 内に外出し用の冗長な逆参照を持たせる。
- [必須] `id: string`：実行試行を識別する自動採番 ID（`run-<英数字6桁または8桁>-<1, 2, 3...>` 形式）。
- [必須] `started: string`：実作業開始時刻（ISO 8601）。
- [任意] `ended: string`：終了時刻（ISO 8601。終了確定時に時刻を確認できる場合は記録する）。
- [必須] `result: string`（条件付き必須）：試行の結果（例：`succeeded`・`failed`・`interrupted`・`canceled`・`error` 等）。実行中は省略し、**Run の終了確定時は必須**とする。
- [任意] `wait: object`：`waiting` 中の待機・実行制御情報（`target`・`condition`・`since`・`resume_point`・`timeout`・待機解消/解除時の `resolved`）。Job 直下ではなく各 Job 内の `Run.wait` で保持する。
- [例] 次は複合スクリプト実行中の Job が外部操作の結果を `waiting` で待機している `tickets[].jobs[].runs[]` の例（実行中のため `result` と `ended` はまだ無い）。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-4",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "ステージング環境へ案内ページを反映する",
      "status": "waiting",
      "jobs": [
        {
          "id": "job-1a2b3c4d-6",
          "objective": "ステージング環境へマイグレーションを適用する",
          "criteria": [
            "承認枠内でマイグレーションが正常適用されている"
          ],
          "status": "waiting",
          "runs": [
            {
              "id": "run-5e6f7a8b-2",
              "started": "2026-01-01T01:00:00Z",
              "wait": {
                "target": "external-approval:staging-slot-7",
                "condition": "メンテナンス枠の開始承認を受け取る",
                "since": "2026-01-01T01:04:30Z",
                "resume_point": "step-3-apply-migration",
                "timeout": "2026-01-01T02:00:00Z"
              }
            }
          ]
        }
      ]
    }
  ]
}
```

- [例] 次は正常終了した `Run` を持つ `Job`（終了確定時に `result` と確認できた `ended` を記録し、この時点で `run-5e6f7a8b-1` は閉じる）。

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "案内文の初稿を作成する",
      "status": "completed",
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "必要事項を揃えた案内文を作る",
          "criteria": [
            "日時・会場・申込方法が記載された案内文ドラフトが保存されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-5e6f7a8b-1",
              "started": "2026-01-01T01:00:00Z",
              "ended": "2026-01-01T01:20:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

## 10. Handoff（`project.tasks[].handoff`：Task 所属の揮発性単一スロット）

- [定義] `handoff` は `Task` オブジェクト内（`project.tasks[].handoff`）に最大1つ持つ揮発性の引き継ぎ・状況スロット（`Handoff | null`、任意・未設定時は省略可）。`Task` の内側に配置されるため逆参照の `Handoff.task_id` は持たない。Task 完了時に消去し、恒久的な知見のみ `Project.knowledge` へ昇格させる。
- [任意] `current_ticket_id: string | null`：現在対象または直近の Ticket ID（`tkt-*`）。
- [任意] `worker_id: string | null`：対象 Ticket の `Worker.id`（`wkr-*`）。
- [任意] `session_id: string | null`：対象 Ticket を担当する `Session.id`（`ssn-*`）。
- [任意] `current_job_id: string | null`：現在 Job の ID（`job-*`）。
- [任意] `context: string`：現在の作業文脈・申し送り・次の操作（再開に不可欠な条件は正本にも必ず残す）。
- [任意] `stop_position: string`：停止位置の要約。
- [任意] `blockers: array<string>`：現在把握しているブロッカー一覧（空配列時は省略可）。
- [任意] `notes: array<string>`：補足情報（空配列時は省略可）。
- [任意] `updated: string`：最終更新時刻（ISO 8601）。
- [禁止] Handoff を正式履歴・成果物・再開条件の正本とする、閲覧権限を実行権と同一視する、Handoff にコマンド実行機能を持たせる。
- [禁止] Handoff が単一スロットであることを理由に、Task 内の無関係な Ticket を直列化する。

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "勉強会の案内を用意する",
        "criteria": [
          "日時・会場・申込方法が正しい案内文が確定している"
        ],
        "status": "doing",
        "handoff": {
          "current_ticket_id": "tkt-c9d0e1f2-3",
          "worker_id": "wkr-7c8d9e0f-1",
          "session_id": "ssn-2b3c4d5e-2",
          "current_job_id": "job-1a2b3c4d-7",
          "context": "レビュー指摘3件のうち日時表記と申込URLを修正済み。残り1件の会場アクセス表記を反映中。",
          "stop_position": "会場アクセス節の照合前",
          "notes": [
            "最新ドラフトは drafts/invitation-v2.md に保存済み。"
          ],
          "updated": "2026-01-01T03:10:00Z"
        }
      }
    ]
  }
}
```

## 11. 長期知識（`project.knowledge[]` の要素）

- [任意] Project の `knowledge`（`project.knowledge[]`、空配列時は省略可）に再利用する知見を残す。
- [必須] 記録する場合は `content: string` に残す内容、`basis: string` に根拠・判断理由・出典をまとめる。
- [必須] 元の Task・Ticket がなくても内容と判断経緯を理解できる。
- [必須] 知識は `id` で識別し、採用状態と変更履歴を保持する。
- [定義] 知識の `status` は `active`・`supersed` の2値（v1.0.0 の綴りを維持）。
- [必須] 新規記録は未指定でも `active` とし、通常は `active` だけを参照する。
- [定義] 将来の予定も、その予定を現在の知識として保持するため `active`。
- [必須] 採用中の旧知識を置換する場合は `supersedes` に同じ Project の旧知識の ID を書く。
- [必須] 置換時は旧記録の `supersed` と新記録の `active` を同時確定する。
- [例] 次は過去の記録、現在使う記録、現在の予定を表した例。

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "勉強会の運営",
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
}
```

## 12. 管理記録（監査イベント履歴）

- [必須] 現在状態と別に、変更前後・主体・時刻・理由・関連実行を外部または運用の監査履歴で追跡する。
- [定義] イベント ID（`id`）・一括操作名（`operation`）・対象 ID（`target`）・変更種別（`event`）・変更前後の値（`from` / `to`）・実行主体（`worker_id` / `session_id`）・時刻（`timestamp`）・理由（`reason`）・関連実行（`run`）を記録項目として扱う。
- [任意] 管理情報は更新手段側で補完し、同等の監査履歴形式を使う。
- [禁止] 補完できない事実を捏造する、または各階層へ手入力させる。
