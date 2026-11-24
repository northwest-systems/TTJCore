# TTJ 1.1.0 代表ユースケースとサンプルパターン（UC-01〜UC-10）

本文書は、[規格本文](specification.md) および [JSON参照形式](json-format.md) を実運用へ適用する際に迷いやすい代表的な **10場面（UC-01〜UC-10）** について、**「どのような場面か（シナリオ）」「規格上のポイント（どの状態・フィールドを使うか）」「サンプル JSON パターン」** をまとめた実践リファレンスである。

すべての JSON サンプルは TTJ 1.1.0 の配列・単一オブジェクト原則と単一ルートスキーマ（0〜複数があり得る項目のみ配列とし、1つだけに定まっている項目は単一オブジェクトとする原則に基づき、ルートキーは `protocol: "TTJ/1.1.0"` と `workers`・`sessions`・`project`（単一オブジェクト）・`tickets` の5つのみ、Project / Task 階層は `project.tasks[]` および `project.knowledge[]`・`project.tasks[].handoff`、Ticket 以下の階層はルート直下の `tickets[].jobs[].runs[]` で各 `Ticket` に親 `task_id` 必須）と7種の自動採番 ID 形式（`pj-`・`tsk-`・`tkt-`・`job-`・`run-`・`wkr-`・`ssn-` に続く `<接頭辞>-<英数字6桁または8桁>-<ゼロ埋めなしインクリメント>`）に完全準拠している。空配列 `[]` および未記載の単一オブジェクト `project` は記載自体を省略できる規則により、1ファイルに `project` と `tickets` を両方記載するパターンと、Task 側のファイル（`project.tasks[]` のみで `tickets` / `jobs` の記載なし）と Ticket 側のファイル（`tickets[].jobs[].runs[]` のみで `task_id` 付き）に分割して排他操作するパターンの双方を100%同一スキーマで表現できる。

---

## 目次

1. [UC-01: 基本の実行フロー](#uc-01-基本の実行フロー)
2. [UC-02: 複数 Worker による並列実行とファイル分割パターン](#uc-02-複数-worker-による並列実行とファイル分割パターン)
3. [UC-03: Ticket 間の依存関係（`depends_on` / `depends`）とブロック解除](#uc-03-ticket-間の依存関係depends_on--dependsとブロック解除)
4. [UC-04: 外部待機（`waiting`）と一時停止（`paused`）の使い分け](#uc-04-外部待機waitingと一時停止pausedの使い分け)
5. [UC-05: Job の失敗（`failed`）と同一 Ticket・同一 Session 内での再試行](#uc-05-job-の失敗failedと同一-ticket同一-session-内での再試行)
6. [UC-06: Worker / Session 再開不可時の旧 Ticket 中止と新規 Ticket への引き継ぎ](#uc-06-worker--session-再開不可時の旧-ticket-中止と新規-ticket-への引き継ぎ)
7. [UC-07: Task 所属の揮発性 Handoff による引き継ぎ](#uc-07-task-所属の揮発性-handoff-による引き継ぎ)
8. [UC-08: 単独 Job の中止（`canceled`）と Task 全体の中止](#uc-08-単独-job-の中止canceledと-task-全体の中止)
9. [UC-09: レビュー差し戻し・追加 Job の発生](#uc-09-レビュー差し戻し追加-job-の発生)
10. [UC-10: Task 完了後の整理（Ticket 肥大化対策）](#uc-10-task-完了後の整理ticket-肥大化対策)

---

## UC-01: 基本の実行フロー

### どのような場面か（シナリオ）
`Project` 配下に長寿命の成果目標である `Task`（`project.tasks[]`）を作成し、その Task に紐づく単一 Worker（`wkr-7c8d9e0f-1`）・単一 Session（`ssn-2b3c4d5e-1`）担当の `Ticket`（ルート `tickets[]`、`task_id: "tsk-e5f6a7b8-1"`）と最小実行単位の `Job` 列（`tickets[].jobs[]`）を正式作成する。その後、`ssn-2b3c4d5e-1` が `Run` を開始し、確認可能な証拠に基づいて作業完了（`status: "completed"` / `Run.result: "succeeded"`）まで一方向に進める最も基本的なシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **`project.tasks[]` とルート `tickets[].jobs[]` の分離と初期状態**：`Task` は `project.tasks[]` に配置し（`Task` 内に `tickets` は持たない）、`Ticket` はルート直下の `tickets[]` に配置して `task_id`（`tsk-*`）で親 Task を必須参照する。初期 `status` はすべて `"todo"`（未実行 Job の `runs` は空配列のため省略可）。Ticket は `worker_id`（`wkr-*`）と `session_id`（`ssn-*`）を持ち、任意フィールド `exclusions` で「その Ticket でやらないこと（対象外・除外事項）」を明記して認識すり合わせ精度を高める（`exclusions` は `Ticket` にのみ配置し、`Job` には置かない）。確定後は担当セッション（`worker_id`・`session_id`）と `jobs` 配列（順序固定）・作業定義を変更しない。
- **Run 開始**：対象 Ticket の `worker_id`・`session_id` と一致する Session が現在 Job の内部フィールド `runs`（`tickets[].jobs[].runs[]`）に初回 `Run`（`run-*`）を追加し、実作業開始と同時に Job・Ticket・`todo` の親 Task を一括で `status: "doing"` に遷移させ、`Run.started` を記録する（実行中のため `result` と `ended` はまだ無い）。実行時は `Ticket.exclusions` に記載された対象外作業を行わない。
- **完了（completed / succeeded）と `status` / `result` 分離**：TTJ 1.1.0 では「作業工程の完了」を Task・Ticket・Job の `status: "completed"` で表し、Job の業務判定を `Job.result`（例：`"passed"`）、実行試行の成功を `Run.result: "succeeded"`（および `ended` 記録）として明確に分離する。

### サンプル JSON パターン

#### パターン 1-A：正式作成直後（`todo`）

```json
{
  "protocol": "TTJ/1.1.0",
  "workers": [
    {
      "id": "wkr-7c8d9e0f-1",
      "name": "Worker 01"
    }
  ],
  "sessions": [
    {
      "id": "ssn-2b3c4d5e-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "started": "2026-02-01T09:00:00Z"
    }
  ],
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "認証トークン検証エンドポイントを実装して公開可能にする",
        "criteria": [
          "エンドポイント実装と単体テストが完了し全件合格している"
        ],
        "status": "todo",
        "deadline": "2026-02-05T18:00:00+09:00"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "トークン検証ハンドラを実装し単体テストを通す",
      "exclusions": [
        "リフレッシュトークン発行エンドポイントの改修は行わない",
        "本番環境の設定ファイルは変更しない"
      ],
      "status": "todo",
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "トークン検証ハンドラを実装する",
          "criteria": [
            "src/auth/verify.ts が作成されビルドが通る"
          ],
          "status": "todo"
        },
        {
          "id": "job-1a2b3c4d-2",
          "objective": "トークン検証ハンドラの単体テストを実行する",
          "criteria": [
            "単体テストが全件合格しテスト結果ログが保存されている"
          ],
          "status": "todo"
        }
      ]
    }
  ]
}
```

#### パターン 1-B：最初の Job の Run 実行中（`doing`）

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "認証トークン検証エンドポイントを実装して公開可能にする",
        "criteria": [
          "エンドポイント実装と単体テストが完了し全件合格している"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "トークン検証ハンドラを実装し単体テストを通す",
      "status": "doing",
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "トークン検証ハンドラを実装する",
          "criteria": [
            "src/auth/verify.ts が作成されビルドが通る"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-5e6f7a8b-1",
              "started": "2026-02-01T09:05:00Z"
            }
          ]
        },
        {
          "id": "job-1a2b3c4d-2",
          "objective": "トークン検証ハンドラの単体テストを実行する",
          "criteria": [
            "単体テストが全件合格しテスト結果ログが保存されている"
          ],
          "status": "todo"
        }
      ]
    }
  ]
}
```

#### パターン 1-C：全 Job・Ticket・Task の完了確定（`completed` / `succeeded`）

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-1",
        "objective": "認証トークン検証エンドポイントを実装して公開可能にする",
        "criteria": [
          "エンドポイント実装と単体テストが完了し全件合格している"
        ],
        "status": "completed"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-c9d0e1f2-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "トークン検証ハンドラを実装し単体テストを通す",
      "exclusions": [
        "リフレッシュトークン発行エンドポイントの改修は行わない",
        "本番環境の設定ファイルは変更しない"
      ],
      "status": "completed",
      "artifacts": [
        "src/auth/verify.ts",
        "logs/unit-test-auth-01.log"
      ],
      "jobs": [
        {
          "id": "job-1a2b3c4d-1",
          "objective": "トークン検証ハンドラを実装する",
          "criteria": [
            "src/auth/verify.ts が作成されビルドが通る"
          ],
          "status": "completed",
          "result": "passed",
          "artifacts": [
            "src/auth/verify.ts"
          ],
          "runs": [
            {
              "id": "run-5e6f7a8b-1",
              "started": "2026-02-01T09:05:00Z",
              "ended": "2026-02-01T09:20:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-1a2b3c4d-2",
          "objective": "トークン検証ハンドラの単体テストを実行する",
          "criteria": [
            "単体テストが全件合格しテスト結果ログが保存されている"
          ],
          "status": "completed",
          "result": "passed",
          "artifacts": [
            "logs/unit-test-auth-01.log"
          ],
          "runs": [
            {
              "id": "run-5e6f7a8b-2",
              "started": "2026-02-01T09:21:00Z",
              "ended": "2026-02-01T09:30:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-02: 複数 Worker による並列実行とファイル分割パターン

### どのような場面か（シナリオ）
同一の `Task`（`tsk-e5f6a7b8-2`）に対して、互いに実行依存を持たない独立した複数の `Ticket`（バックエンド実装 `tkt-2a3b4c5d-1` とフロントエンド実装 `tkt-2a3b4c5d-2`、ともに `task_id: "tsk-e5f6a7b8-2"`）を作成し、`wkr-7c8d9e0f-1`（`ssn-2b3c4d5e-1`）と `wkr-7c8d9e0f-2`（`ssn-9a8b7c6d-1`）がそれぞれの Ticket で並行して `Run` を同時実行するシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **Ticket ごとの担当セッション固定と分割**：1つの Ticket に複数 Worker や複数 Session を設定することは禁止されているため、複数 Worker（`wkr-7c8d9e0f-1`, `wkr-7c8d9e0f-2`）が分担する場合は Worker / Session ごとに別 Ticket を作成し、それぞれ不変の `task_id`・`worker_id`・`session_id` を設定する。各 Ticket の `exclusions` に相手側スコープ（やらないこと）を明記することで、並列実行時の変更衝突や重複作業を防ぐ。
- **チケット単位のファイル分割による排他操作（単一スキーマ・空配列省略）**：ルートで `project.tasks[]` と `tickets[].jobs[].runs[]` が分離されており空配列は記載を省略できるため、**100% 同一のルート JSON スキーマ** のまま以下のようにファイルを分割して各 Worker が自分の Ticket ファイルだけを排他更新できる。
  1. **Task 側のファイル**：`protocol` と単一オブジェクト `project`（必要に応じて `workers`, `sessions`）のみを記載し、`tickets` は省略（`Task` 内にも `tickets` や `jobs` の記載は一切なし）。
  2. **各 Worker の Ticket ファイル**：`protocol` と `tickets`（自分が担当する Ticket とその `jobs[].runs[]`）のみを記載し、`project`, `workers`, `sessions` は省略。`task_id` で親 Task に紐づく。
- **並列実行の非阻害**：一方の Ticket が `waiting` や `blocked` になったり、Task 内の単一スロット `handoff` が一方の Ticket 文脈を表示していたりしても、無関係な独立 Ticket の開始・並行実行を遮ったり直列化したりしてはならない（[規格本文](specification.md) §6・§8.2・§9.1・禁止事項 #16）。

### サンプル JSON パターン

#### パターン 2-A：チケット単位でファイルを分割して並列・排他操作する場合

1. **Task 側の管理ファイル（`project.tasks[]` のみで `tickets` / `jobs` の記載なし）**

```json
{
  "protocol": "TTJ/1.1.0",
  "workers": [
    {
      "id": "wkr-7c8d9e0f-1",
      "name": "Worker 01"
    },
    {
      "id": "wkr-7c8d9e0f-2",
      "name": "Worker 02"
    }
  ],
  "sessions": [
    {
      "id": "ssn-2b3c4d5e-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "started": "2026-02-01T10:00:00Z"
    },
    {
      "id": "ssn-9a8b7c6d-1",
      "worker_id": "wkr-7c8d9e0f-2",
      "started": "2026-02-01T10:00:00Z"
    }
  ],
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-2",
        "objective": "検索APIと検索UIを実装する",
        "criteria": [
          "検索APIエンドポイントと検索画面コンポーネントの双方がテストを通過している"
        ],
        "status": "doing"
      }
    ]
  }
}
```

2. **Worker 01 担当の Ticket ファイル（`tickets[].jobs[].runs[]` のみで `task_id` 付き）**

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-2a3b4c5d-1",
      "task_id": "tsk-e5f6a7b8-2",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "検索APIエンドポイントを実装する",
      "exclusions": [
        "フロントエンドの検索UIコンポーネント（src/ui/）は変更しない"
      ],
      "status": "doing",
      "jobs": [
        {
          "id": "job-2b3c4d5e-1",
          "objective": "検索APIエンドポイントを実装し単体テストを通す",
          "criteria": [
            "検索APIの単体テストが全件合格している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-2b3c4d5e-1",
              "started": "2026-02-01T10:01:00Z"
            }
          ]
        }
      ]
    }
  ]
}
```

3. **Worker 02 担当の Ticket ファイル（`tickets[].jobs[].runs[]` のみで `task_id` 付き）**

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-2a3b4c5d-2",
      "task_id": "tsk-e5f6a7b8-2",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "検索UIコンポーネントを実装する",
      "exclusions": [
        "バックエンドの検索API実装（src/api/）およびDBスキーマは変更しない"
      ],
      "status": "doing",
      "jobs": [
        {
          "id": "job-2b3c4d5e-2",
          "objective": "検索UIコンポーネントを実装し単体テストを通す",
          "criteria": [
            "検索UIコンポーネントのテストが全件合格している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-2b3c4d5e-2",
              "started": "2026-02-01T10:01:30Z"
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 2-B：1つのファイルにまとめて記載する場合

```json
{
  "protocol": "TTJ/1.1.0",
  "workers": [
    {
      "id": "wkr-7c8d9e0f-1",
      "name": "Worker 01"
    },
    {
      "id": "wkr-7c8d9e0f-2",
      "name": "Worker 02"
    }
  ],
  "sessions": [
    {
      "id": "ssn-2b3c4d5e-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "started": "2026-02-01T10:00:00Z"
    },
    {
      "id": "ssn-9a8b7c6d-1",
      "worker_id": "wkr-7c8d9e0f-2",
      "started": "2026-02-01T10:00:00Z"
    }
  ],
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-2",
        "objective": "検索APIと検索UIを実装する",
        "criteria": [
          "検索APIエンドポイントと検索画面コンポーネントの双方がテストを通過している"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-2a3b4c5d-1",
      "task_id": "tsk-e5f6a7b8-2",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "検索APIエンドポイントを実装する",
      "exclusions": [
        "フロントエンドの検索UIコンポーネント（src/ui/）は変更しない"
      ],
      "status": "doing",
      "jobs": [
        {
          "id": "job-2b3c4d5e-1",
          "objective": "検索APIエンドポイントを実装し単体テストを通す",
          "criteria": [
            "検索APIの単体テストが全件合格している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-2b3c4d5e-1",
              "started": "2026-02-01T10:01:00Z"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-2a3b4c5d-2",
      "task_id": "tsk-e5f6a7b8-2",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "検索UIコンポーネントを実装する",
      "exclusions": [
        "バックエンドの検索API実装（src/api/）およびDBスキーマは変更しない"
      ],
      "status": "doing",
      "jobs": [
        {
          "id": "job-2b3c4d5e-2",
          "objective": "検索UIコンポーネントを実装し単体テストを通す",
          "criteria": [
            "検索UIコンポーネントのテストが全件合格している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-2b3c4d5e-2",
              "started": "2026-02-01T10:01:30Z"
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-03: Ticket 間の依存関係（`depends_on` / `depends`）とブロック解除

### どのような場面か（シナリオ）
後続 Ticket が先行 Ticket の完了を前提としている場面。先行 Ticket が未完了（あるいは先行 Ticket が中止となり継続不能な `blocked` 状態）のため後続 Ticket が開始を阻まれている状態から、先行 Ticket（または代替 Ticket）の `completed` 確定によってブロックが解除され、後続 Ticket が実行可能な `"todo"` へ遷移するシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **実行依存フィールド名 `depends`**：実行依存は `Ticket.depends`（前提 Ticket ID `tkt-*` の配列）として記述する。情報共有専用の `refs` とは明確に区別し、循環依存は厳禁とする。
- **先行 Ticket 未完了時の状態（計画内待機 `waiting` と計画外障害 `blocked`）**：
  1. **通常の先行 Ticket 完了待ち**：先行 Ticket が進行中（`todo` / `doing` 等）の間、後続 Ticket は計画内の前提待機として `status: "waiting"` とし、先行 Ticket が `status: "completed"` になった時点で依存評価操作により `waiting → todo` へ解除される。
  2. **先行 Ticket の中止や成果物欠落による `blocked` とその解除**：先行 Ticket（例：`tkt-3c4d5e6f-1`）が `status: "canceled"` になった場合など、現行計画のままでは継続不能になったとき、後続 Ticket（`tkt-3c4d5e6f-3`）は `status: "blocked"` に遷移し `resume` に解除条件を永続記録する。その後、代替の先行 Ticket（`tkt-3c4d5e6f-2`）が完了し、未着手の後続 Ticket の `depends` 更新と課題解消が確認された時点で `blocked → todo` へ遷移してブロック解除される（[規格本文](specification.md) §5.2・§7）。

### サンプル JSON パターン

#### パターン 3-A：先行 Ticket 中止により後続 Ticket が `blocked` で待機中

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-3",
        "objective": "監査ログ用テーブルを追加しマイグレーションを適用する",
        "criteria": [
          "確定スキーマによるマイグレーションが正常適用されている"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-3c4d5e6f-1",
      "task_id": "tsk-e5f6a7b8-3",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "新テーブルのスキーマ定義を作成する",
      "status": "canceled",
      "cancel_reason": "旧仕様のカラム設計に不整合が判明したため中止。tkt-3c4d5e6f-2 で再作成する。",
      "jobs": [
        {
          "id": "job-3c4d5e6f-1",
          "objective": "初期スキーマ定義SQLを作成する",
          "criteria": [
            "db/schema-v1.sql が保存されている"
          ],
          "status": "canceled"
        }
      ]
    },
    {
      "id": "tkt-3c4d5e6f-2",
      "task_id": "tsk-e5f6a7b8-3",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "修正仕様に基づきスキーマ定義を確定する",
      "status": "doing",
      "refs": [
        {
          "ticket_id": "tkt-3c4d5e6f-1",
          "kind": "rerun_of",
          "note": "カラム型修正後の再作成"
        }
      ],
      "jobs": [
        {
          "id": "job-3c4d5e6f-2",
          "objective": "改訂版スキーマ定義SQLを作成し検証する",
          "criteria": [
            "db/schema-v2.sql が作成され構文検証を通過している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-3c4d5e6f-1",
              "started": "2026-02-01T11:00:00Z"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-3c4d5e6f-3",
      "task_id": "tsk-e5f6a7b8-3",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "確定スキーマを用いてマイグレーションを実行する",
      "status": "blocked",
      "depends": [
        "tkt-3c4d5e6f-1"
      ],
      "resume": "代替の先行Ticket tkt-3c4d5e6f-2 が completed になり depends を更新してスキーマ成果物の存在を確認する",
      "jobs": [
        {
          "id": "job-3c4d5e6f-3",
          "objective": "マイグレーションを適用しテーブル構造を確認する",
          "criteria": [
            "マイグレーションがエラーなく適用されている"
          ],
          "status": "todo"
        }
      ]
    }
  ]
}
```

#### パターン 3-B：先行 Ticket の完了（`completed`）に伴い後続 Ticket が `"todo"` へブロック解除された状態

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-3",
        "objective": "監査ログ用テーブルを追加しマイグレーションを適用する",
        "criteria": [
          "確定スキーマによるマイグレーションが正常適用されている"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-3c4d5e6f-2",
      "task_id": "tsk-e5f6a7b8-3",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "修正仕様に基づきスキーマ定義を確定する",
      "status": "completed",
      "artifacts": [
        "db/schema-v2.sql"
      ],
      "jobs": [
        {
          "id": "job-3c4d5e6f-2",
          "objective": "改訂版スキーマ定義SQLを作成し検証する",
          "criteria": [
            "db/schema-v2.sql が作成され構文検証を通過している"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-3c4d5e6f-1",
              "started": "2026-02-01T11:00:00Z",
              "ended": "2026-02-01T11:15:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-3c4d5e6f-3",
      "task_id": "tsk-e5f6a7b8-3",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "確定スキーマを用いてマイグレーションを実行する",
      "status": "todo",
      "depends": [
        "tkt-3c4d5e6f-2"
      ],
      "refs": [
        {
          "ticket_id": "tkt-3c4d5e6f-1",
          "kind": "related",
          "note": "旧前提 tkt-3c4d5e6f-1 の中止に伴い前提を tkt-3c4d5e6f-2 に更新済み"
        }
      ],
      "jobs": [
        {
          "id": "job-3c4d5e6f-3",
          "objective": "マイグレーションを適用しテーブル構造を確認する",
          "criteria": [
            "マイグレーションがエラーなく適用されている"
          ],
          "status": "todo"
        }
      ]
    }
  ]
}
```

---

## UC-04: 外部待機（`waiting`）と一時停止（`paused`）の使い分け

### どのような場面か（シナリオ）
Job 実行中に外部システムの応答や承認を待つ計画内待機（`waiting`）と、オーナー（または運用責任者）の明示指示によって作業を一時保留する（`paused`）シナリオ。さらに、`waiting` 中にオーナー指示で `paused` へ移行し、その後 `paused` を解除した際にどのように状態を再評価するかを示す。

### 規格上のポイント（どの状態・フィールドを使うか）
- **`waiting` と `Run.wait`**：計画に含まれる外部応答・承認・イベント待ちは `status: "waiting"` とする。Job 実行に伴う待機制御情報（`target`, `condition`, `since`, `resume_point`, `timeout`, 解消時 `resolved`）は Job 直下ではなく各 Job 内の **`Run.wait`** に保持する（Run を持たない Task 単位の計画内待機や、`Run.wait` だけでは再開条件が分からない場合は `resume` に記載する）。待機対象や Worker 種別で状態遷移を分岐させてはならない。
- **`paused`（オーナー指示等による明示的な保留）**：運用判断やオーナー指示で一時停止する場合は `status: "paused"` とし、明示解除されるまで一切の実行再開を禁止する。
- **`paused` 解除時の再評価順序**：`waiting`（または `blocked`）の途中で `paused` にした場合も、未解消の `Run.wait` や課題はそのまま保持する。オーナー指示で `paused` を明示解除した際は、**保留前の状態を無条件に復元せず**、残存する `blocked`・`waiting`・`depends` を先に再評価する。外部応答がまだ未到着なら `paused → waiting` に戻り、待機条件がすべて解消済み（`Run.wait.resolved` 記録済み）なら `paused → doing`（未着手なら `todo`）へ復帰する。

### サンプル JSON パターン

#### パターン 4-A：外部応答待ちの `waiting`（`Run.wait` による待機制御）

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-4",
        "objective": "v1.1.0 を本番環境へリリースする",
        "criteria": [
          "本番デプロイと疎通確認が完了している"
        ],
        "status": "waiting"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-4d5e6f7a-1",
      "task_id": "tsk-e5f6a7b8-4",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "本番環境へのデプロイを実施する",
      "status": "waiting",
      "jobs": [
        {
          "id": "job-4d5e6f7a-1",
          "objective": "本番反映スクリプトを実行しヘルスチェックを確認する",
          "criteria": [
            "承認ゲート通過後に本番反映が完了しヘルスチェック200 OKが記録されている"
          ],
          "status": "waiting",
          "runs": [
            {
              "id": "run-4d5e6f7a-1",
              "started": "2026-02-01T12:00:00Z",
              "wait": {
                "target": "change-gate:chg-2026-0201",
                "condition": "変更管理システムで本番反映ウィンドウの承認通知を受領する",
                "since": "2026-02-01T12:03:00Z",
                "resume_point": "step-2-rollout-service",
                "timeout": "2026-02-01T15:00:00Z"
              }
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 4-B：オーナー指示による一時停止（`paused`）と未解消 `Run.wait` の保持

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-4",
        "objective": "v1.1.0 を本番環境へリリースする",
        "criteria": [
          "本番デプロイと疎通確認が完了している"
        ],
        "status": "paused",
        "resume": "オーナーからリリース凍結解除の指示を受け、かつ変更管理ゲート chg-2026-0201 の承認を確認する"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-4d5e6f7a-1",
      "task_id": "tsk-e5f6a7b8-4",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "本番環境へのデプロイを実施する",
      "status": "paused",
      "resume": "オーナーの保留解除指示後、未解決の承認ゲート待機を再評価する",
      "jobs": [
        {
          "id": "job-4d5e6f7a-1",
          "objective": "本番反映スクリプトを実行しヘルスチェックを確認する",
          "criteria": [
            "承認ゲート通過後に本番反映が完了しヘルスチェック200 OKが記録されている"
          ],
          "status": "paused",
          "resume": "保留解除および change-gate:chg-2026-0201 の承認成立を確認する",
          "runs": [
            {
              "id": "run-4d5e6f7a-1",
              "started": "2026-02-01T12:00:00Z",
              "ended": "2026-02-01T12:10:00Z",
              "result": "interrupted",
              "wait": {
                "target": "change-gate:chg-2026-0201",
                "condition": "変更管理システムで本番反映ウィンドウの承認通知を受領する",
                "since": "2026-02-01T12:03:00Z",
                "resume_point": "step-2-rollout-service",
                "timeout": "2026-02-01T15:00:00Z"
              }
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-05: Job の失敗（`failed`）と同一 Ticket・同一 Session 内での再試行

### どのような場面か（シナリオ）
Job の 1 回目の実行試行（`run-5a6b7c8d-1`）が一時的な通信エラーやタイムアウトで失敗（`Run.result: "failed"`）したものの、同じ作業計画（同一 Ticket・同一 Session・同一 Job）のまま回復可能であるため、失敗した旧 Run を閉じて新しい Run（`run-5a6b7c8d-2`）を追加・実行するシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **回復可能な中断と Job `status: "doing"` の維持**：Run が 1 回失敗・終了しただけで即座に Job や Ticket を `completed`・`canceled`・`blocked` にしてはならない（[規格本文](specification.md) §8.2）。同一計画・同一 Session で再試行できる間は Job・Ticket ともに `status: "doing"` を維持する。
- **旧 Run の閉鎖と新 Run の追加**：旧 Run（`run-5a6b7c8d-1`）の終了確定時に `result: "failed"`（および `ended`）を記録して閉じ、前の実行が停止（または残存副作用がない安全条件）していることを確認した上で、同一 Ticket（`tkt-5e6f7a8b-1`）・同一 Session（`ssn-2b3c4d5e-1`）で当該 Job（`job-5e6f7a8b-1`）の `runs` 配列に新しい `Run`（`run-5a6b7c8d-2`）を追加して再試行する。

### サンプル JSON パターン

#### パターン 5-A：1回目の Run（`run-5a6b7c8d-1`）が `failed` で閉鎖された直後

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-5e6f7a8b-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "ステージング環境で結合検証を実行する",
      "status": "doing",
      "jobs": [
        {
          "id": "job-5e6f7a8b-1",
          "objective": "結合テストスイートを実行し全件成功を確認する",
          "criteria": [
            "結合テスト全12ケースが成功し結果レポートが保存されている"
          ],
          "status": "doing",
          "resume": "一時的な接続タイムアウト解消を確認して同一Jobを新Runで再試行する",
          "runs": [
            {
              "id": "run-5a6b7c8d-1",
              "started": "2026-02-01T13:00:00Z",
              "ended": "2026-02-01T13:04:00Z",
              "result": "failed"
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 5-B：同一 Ticket・同一 Session 内で新 Run（`run-5a6b7c8d-2`）を追加して再試行中

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-5e6f7a8b-1",
      "task_id": "tsk-e5f6a7b8-1",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "ステージング環境で結合検証を実行する",
      "status": "doing",
      "jobs": [
        {
          "id": "job-5e6f7a8b-1",
          "objective": "結合テストスイートを実行し全件成功を確認する",
          "criteria": [
            "結合テスト全12ケースが成功し結果レポートが保存されている"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-5a6b7c8d-1",
              "started": "2026-02-01T13:00:00Z",
              "ended": "2026-02-01T13:04:00Z",
              "result": "failed"
            },
            {
              "id": "run-5a6b7c8d-2",
              "started": "2026-02-01T13:06:00Z"
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-06: Worker / Session 再開不可時の旧 Ticket 中止と新規 Ticket への引き継ぎ

### どのような場面か（シナリオ）
作業中の Worker / Session が異常停止し再開不可となったため、旧 Ticket（および未完了 Job / Run）を中止・閉鎖し、同一 Task（同じ `task_id`）に紐づく新規 Ticket（`refs` で旧 Ticket を参照）を起票して新しい Session で稼働させるシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **1つの Ticket の担当セッション不変の原則**：1つの Ticket の持ち主（`worker_id`・`session_id`）は途中で変わらない。担当 Session が再開不可になった場合は、同じ Ticket の持ち主を付け替えるのではなく、旧 Ticket を閉じて新規 Ticket として作り直し、新しい Session で稼働させる。
- **引き継ぎ前の停止・安全確認**：前の実行が稼働中または停止未確認の間は、後続 Ticket による引き継ぎ実行を開始してはならない（禁止事項 #13, #14）。前の実行が停止（または残存副作用がない安全条件）していることを確認できた場合のみ引き継ぎが許可される。
- **旧 Ticket の閉鎖と新規 Ticket の起票（別 Worker でも同一 Worker の新 Session でも同一方式）**：
  - 前の実行が停止（または残存副作用がない安全条件）していることを確認した上で、旧 Run を終了（`result: "canceled"` または `"interrupted"` 等、確認できれば `ended` を記録）し、旧 Ticket `tkt-6f7a8b9c-1`（`task_id: "tsk-e5f6a7b8-4"`, `worker_id: "wkr-7c8d9e0f-1"`, `session_id: "ssn-2b3c4d5e-1"`）を `status: "canceled"`（`cancel_reason` 必須、未完了 Job も `canceled`）として閉鎖する。
  - ルート `tickets[]`（または新規 Ticket ファイル）に同じ `task_id: "tsk-e5f6a7b8-4"` を持つ新規 Ticket `tkt-6f7a8b9c-2`（`refs` に `tkt-6f7a8b9c-1` を指定）を起票し、新しい担当 Session（別 Worker `wkr-7c8d9e0f-2` の `ssn-9a8b7c6d-1`、または同一 Worker `wkr-7c8d9e0f-1` の新 Session `ssn-2b3c4d5e-2`）を設定して新しい `Run` を開始する。

### サンプル JSON パターン

#### パターン 6-A：別 Worker（`wkr-7c8d9e0f-2` / `ssn-9a8b7c6d-1`）が新規 Ticket（`tkt-6f7a8b9c-2`）で引き継ぐ場合

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-4",
        "objective": "v1.1.0 を本番環境へリリースする",
        "criteria": [
          "ステージング疎通確認と本番リリースが完了している"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-6f7a8b9c-1",
      "task_id": "tsk-e5f6a7b8-4",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "ステージング環境へパッケージを配布し疎通確認を行う",
      "status": "canceled",
      "cancel_reason": "ssn-2b3c4d5e-1 の停止および残存副作用がない安全条件を確認した上で旧Ticketを中止し、新規Ticket tkt-6f7a8b9c-2 (wkr-7c8d9e0f-2 / ssn-9a8b7c6d-1) へ引き継ぐ。",
      "artifacts": [
        "packages/app-v110.tar.gz"
      ],
      "jobs": [
        {
          "id": "job-6f7a8b9c-1",
          "objective": "ステージングへパッケージをアップロードする",
          "criteria": [
            "packages/app-v110.tar.gz のチェックサム一致が確認されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-6f7a8b9c-1",
              "started": "2026-02-01T13:50:00Z",
              "ended": "2026-02-01T13:59:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-6f7a8b9c-2",
          "objective": "ステージングの疎通テストを実行する",
          "criteria": [
            "疎通テスト全項目が成功している"
          ],
          "status": "canceled",
          "result": "interrupted",
          "runs": [
            {
              "id": "run-6f7a8b9c-2",
              "started": "2026-02-01T14:00:00Z",
              "ended": "2026-02-01T14:15:00Z",
              "result": "interrupted"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-6f7a8b9c-2",
      "task_id": "tsk-e5f6a7b8-4",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "配布済みパッケージを用いてステージング疎通確認を完了する",
      "status": "doing",
      "refs": [
        {
          "ticket_id": "tkt-6f7a8b9c-1",
          "kind": "derived_from",
          "note": "job-6f7a8b9c-1 の成果物 packages/app-v110.tar.gz を引き継いで残工程を実行"
        }
      ],
      "jobs": [
        {
          "id": "job-6f7a8b9c-3",
          "objective": "ステージングの疎通テストを実行する",
          "criteria": [
            "疎通テスト全項目が成功している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-6f7a8b9c-3",
              "started": "2026-02-01T14:21:00Z"
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 6-B：同一 Worker（`wkr-7c8d9e0f-1`）が新 Session（`ssn-2b3c4d5e-2`）で新規 Ticket（`tkt-6f7a8b9c-2`）を起票して引き継ぐ場合（分割 Ticket ファイル形式）

```json
{
  "protocol": "TTJ/1.1.0",
  "tickets": [
    {
      "id": "tkt-6f7a8b9c-2",
      "task_id": "tsk-e5f6a7b8-4",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "配布済みパッケージを用いてステージング疎通確認を完了する",
      "status": "doing",
      "refs": [
        {
          "ticket_id": "tkt-6f7a8b9c-1",
          "kind": "derived_from",
          "note": "旧Session ssn-2b3c4d5e-1 の停止確認後に旧Ticket tkt-6f7a8b9c-1 を閉じ、新Session ssn-2b3c4d5e-2 で残工程を引き継ぐ"
        }
      ],
      "jobs": [
        {
          "id": "job-6f7a8b9c-3",
          "objective": "ステージングの疎通テストを実行する",
          "criteria": [
            "疎通テスト全項目が成功している"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-6f7a8b9c-3",
              "started": "2026-02-01T14:21:00Z"
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-07: Task 所属の揮発性 Handoff による引き継ぎ

### どのような場面か（シナリオ）
作業途中の文脈・停止位置・注意点を `Task` 内側の揮発性単一スロット `handoff`（`project.tasks[].handoff`）に記録して後続 Session や他メンバーが状況を把握できるようにし、Task 完了時には揮発性 Handoff を破棄・消去して、今後再利用すべき長期知見だけを `Project.knowledge`（`project.knowledge[]`）へ昇格させるシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **Task 内側の揮発性単一スロット**：v1.1.0 では旧 `Ticket.handoffs` を廃止し、`Task` オブジェクトの内部フィールド `handoff` として最大1つの揮発性 `Handoff` スロットを持つ（`Task` の内側に配置されるため `Handoff.task_id` は持たない）。
- **閲覧権限と実行権限の分離**：Handoff は Task 参照権限を持つ複数 Session から閲覧できるが、Handoff を閲覧できることは Ticket の実行権限を意味しない（禁止事項 #11）。
- **正本保存の原則と Task 完了時の揮発・消去**：再開に必須の条件（`resume`, `Run.wait`, `cancel_reason`, `artifacts`）は必ず永続正本側に保存し、Handoff を正式履歴の正本にしてはならない（禁止事項 #15）。Task 完了時に `handoff` スロットを消去しても正式履歴は一切失われず、再利用する恒久知見のみを選別して `Project.knowledge`（`status: "active"`）へ記録する。

### サンプル JSON パターン

#### パターン 7-A：作業進行中の Task 揮発性 `handoff` スロットと永続正本

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-5",
        "objective": "認証・検索APIのP99レイテンシを50ms以下に改善する",
        "criteria": [
          "ベンチマーク計測でP99レイテンシが50ms以下であることが確認されている"
        ],
        "status": "doing",
        "handoff": {
          "current_ticket_id": "tkt-7a8b9c0d-1",
          "worker_id": "wkr-7c8d9e0f-1",
          "session_id": "ssn-2b3c4d5e-1",
          "current_job_id": "job-7a8b9c0d-2",
          "context": "TTLを300秒に変更した cache-ttl-v2.yaml を反映済み。現在ベンチマーク計測を実行中。",
          "stop_position": "job-7a8b9c0d-2 のウォームアップ完了後・本計測ステップ",
          "notes": [
            "TTL 600秒ではメモリ使用率が閾値を超えたため300秒を採用した。"
          ],
          "updated": "2026-02-01T15:10:00Z"
        }
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-7a8b9c0d-1",
      "task_id": "tsk-e5f6a7b8-5",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "キャッシュTTL設定を調整してP99レイテンシを改善する",
      "status": "doing",
      "artifacts": [
        "configs/cache-ttl-v2.yaml"
      ],
      "jobs": [
        {
          "id": "job-7a8b9c0d-1",
          "objective": "キャッシュTTL設定ファイルを更新する",
          "criteria": [
            "configs/cache-ttl-v2.yaml が保存されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-7a8b9c0d-1",
              "started": "2026-02-01T14:50:00Z",
              "ended": "2026-02-01T15:00:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-7a8b9c0d-2",
          "objective": "ベンチマーク計測を実行しP99レイテンシを確認する",
          "criteria": [
            "ベンチマークレポートでP99 50ms以下が確認されている"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-7a8b9c0d-2",
              "started": "2026-02-01T15:02:00Z"
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 7-B：Task 完了後（揮発性 Handoff 消去後）の永続正本と `Project.knowledge` への知見昇格

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "knowledge": [
      {
        "id": "cache-ttl-guideline",
        "status": "active",
        "content": "APIレスポンスキャッシュの標準TTLは300秒とし、メモリ使用率80%未満を維持する",
        "basis": "tsk-e5f6a7b8-5 のベンチマーク検証において、TTL 600秒ではメモリ逼迫が発生し、300秒でP99 38msとメモリ安全域を両立できたため。"
      }
    ],
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-5",
        "objective": "認証・検索APIのP99レイテンシを50ms以下に改善する",
        "criteria": [
          "ベンチマーク計測でP99レイテンシが50ms以下であることが確認されている"
        ],
        "status": "completed"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-7a8b9c0d-1",
      "task_id": "tsk-e5f6a7b8-5",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "キャッシュTTL設定を調整してP99レイテンシを改善する",
      "status": "completed",
      "artifacts": [
        "configs/cache-ttl-v2.yaml",
        "reports/benchmark-20260201.json"
      ],
      "jobs": [
        {
          "id": "job-7a8b9c0d-1",
          "objective": "キャッシュTTL設定ファイルを更新する",
          "criteria": [
            "configs/cache-ttl-v2.yaml が保存されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-7a8b9c0d-1",
              "started": "2026-02-01T14:50:00Z",
              "ended": "2026-02-01T15:00:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-7a8b9c0d-2",
          "objective": "ベンチマーク計測を実行しP99レイテンシを確認する",
          "criteria": [
            "ベンチマークレポートでP99 50ms以下が確認されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-7a8b9c0d-2",
              "started": "2026-02-01T15:02:00Z",
              "ended": "2026-02-01T15:20:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

---

## UC-08: 単独 Job の中止（`canceled`）と Task 全体の中止

### どのような場面か（シナリオ）
Ticket 内の特定の不要になった Job だけを `canceled` にして実行中の Run を閉鎖しつつ同一 Ticket・同一 Session で後続 Job を続行するケースと、方針変更等により Task 全体（および配下の未終端 Ticket・Job）を一括で `canceled` にするケースの対比シナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **単独 Job の中止（後続 Job を続ける場合）**：
  - 対象 Job を `status: "canceled"` に確定すると同時に、実行中の Run があれば終了（`Run.result: "canceled"` 等を記録して閉鎖）し、停止要求を発行する。
  - 後続 Job の新 Run を開始する前に、前の Job の実行が停止（または残存副作用がない安全条件）していることを確認する。全 Job が `completed` または `canceled` になれば Ticket は `status: "completed"` に集約される（ただし中止した Job の目標を達成したとはみなさない）。
- **Task 全体（または Ticket 全体）の中止**：
  - 親 Task を `status: "canceled"` にすると、`task_id` でその Task に属する**未終端の Ticket・Job もすべて同時に `status: "canceled"`** に確定し、実行中の Run も閉鎖する（既に `completed` になっていた終端済みの子孫は変更せず保持する）。
  - 中止された各 Ticket には **`cancel_reason` の記録が必須**。Ticket 自身の中止は全 Job 終端による `completed` 集約より常に優先される。

### サンプル JSON パターン

#### パターン 8-A：単独 Job（`job-8b9c0d1e-2`）のみを中止し、同一 Ticket・同一 Session で後続 Job（`job-8b9c0d1e-3`）を完了させたパターン

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-6",
        "objective": "プレビュー環境を構築して動作確認を完了する",
        "criteria": [
          "プレビュー環境のヘルスチェックが成功している"
        ],
        "status": "completed"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-8b9c0d1e-1",
      "task_id": "tsk-e5f6a7b8-6",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "プレビュー環境を構築し疎通を確認する",
      "status": "completed",
      "jobs": [
        {
          "id": "job-8b9c0d1e-1",
          "objective": "プレビュー用コンテナを起動する",
          "criteria": [
            "コンテナがRunning状態である"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-8b9c0d1e-1",
              "started": "2026-02-01T16:00:00Z",
              "ended": "2026-02-01T16:09:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-8b9c0d1e-2",
          "objective": "旧形式のデモ用シードデータを投入する",
          "criteria": [
            "シードデータが投入されている"
          ],
          "status": "canceled",
          "result": "canceled",
          "runs": [
            {
              "id": "run-8b9c0d1e-2",
              "started": "2026-02-01T16:10:00Z",
              "ended": "2026-02-01T16:11:00Z",
              "result": "canceled"
            }
          ]
        },
        {
          "id": "job-8b9c0d1e-3",
          "objective": "プレビュー環境のヘルスチェックを実行する",
          "criteria": [
            "/healthz が 200 OK を返す"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-8b9c0d1e-3",
              "started": "2026-02-01T16:12:00Z",
              "ended": "2026-02-01T16:15:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```

#### パターン 8-B：Task 全体を中止（`canceled`）し、未終端の子孫中止を同時確定したパターン

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-7",
        "objective": "旧CSVエクスポート機能を追加する",
        "criteria": [
          "CSV出力APIとバッチ処理が稼働している"
        ],
        "status": "canceled"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-8b9c0d1e-2",
      "task_id": "tsk-e5f6a7b8-7",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "CSVフォーマット仕様を策定する",
      "status": "completed",
      "artifacts": [
        "docs/csv-spec.md"
      ],
      "jobs": [
        {
          "id": "job-8b9c0d1e-4",
          "objective": "CSVフォーマット仕様書を作成する",
          "criteria": [
            "docs/csv-spec.md が作成されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-8b9c0d1e-4",
              "started": "2026-02-01T15:00:00Z",
              "ended": "2026-02-01T15:30:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-8b9c0d1e-3",
      "task_id": "tsk-e5f6a7b8-7",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "CSV出力バッチを実装する",
      "status": "canceled",
      "cancel_reason": "製品ロードマップ変更により旧CSVエクスポート機能の開発自体が中止（tsk-e5f6a7b8-7 canceled）となったため。",
      "jobs": [
        {
          "id": "job-8b9c0d1e-5",
          "objective": "CSV出力バッチを実装する",
          "criteria": [
            "バッチ処理コードがビルドを通過する"
          ],
          "status": "canceled",
          "result": "canceled",
          "runs": [
            {
              "id": "run-8b9c0d1e-5",
              "started": "2026-02-01T16:00:00Z",
              "ended": "2026-02-01T16:20:00Z",
              "result": "canceled"
            }
          ]
        },
        {
          "id": "job-8b9c0d1e-6",
          "objective": "CSV出力バッチを検証する",
          "criteria": [
            "検証テストが全件成功する"
          ],
          "status": "canceled"
        }
      ]
    }
  ]
}
```

---

## UC-09: レビュー差し戻し・追加 Job の発生

### どのような場面か（シナリオ）
作成 Ticket（`tkt-9c0d1e2f-1`）の完了後に別 Worker（`wkr-7c8d9e0f-2`）がレビュー Ticket（`tkt-9c0d1e2f-2`）を実行した結果、修正要求（`changes_requested`）となり、指摘反映と再検証のための追加 Job と新 Run が必要になったシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **レビュー差し戻し時の `status` と `result` の分離**：レビュー Job が差し戻し結論を出した場合、レビューという作業自体は完了しているため `Job.status: "completed"`, `Job.result: "changes_requested"` として確定し、レビュー Ticket も `status: "completed"` に集約する（独立した `Request` オブジェクトは導入しない）。
- **確定後 Ticket への Job 追加禁止・終端 Ticket 再開禁止**：一度正式作成した Ticket の `jobs` 列に後から Job を追加したり、完了済みの作成 Ticket やレビュー Ticket を `doing` / `todo` に再開・巻き戻したりすることは**禁止**（禁止事項 #8, #9）。
- **同一 Task（同じ `task_id`）への新 Ticket 起票による追加 Job・新 Run 対応**：差し戻し修正や追加で必要になった Job 列は、**同一 Task（`task_id: "tsk-e5f6a7b8-8"`）に紐づく新しい Ticket（`tkt-9c0d1e2f-3`）としてルート `tickets[]` に起票**する（Task を新しく増殖させてはならない）。新 Ticket の `depends` にレビュー Ticket を指定し、`refs`（`kind: "fixes"` / `"derived_from"`）で初稿 Ticket の成果物とレビュー Ticket の指摘メモを参照し、`exclusions` で修正対象外の範囲を明記した上で、新 Session（`ssn-2b3c4d5e-2`）で新 Run を実行する。

### サンプル JSON パターン

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-8",
        "objective": "APIセキュリティ運用規約を策定し承認を得る",
        "criteria": [
          "規約文書が作成され、レビュアーの承認（approved）を得ている"
        ],
        "status": "doing"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-9c0d1e2f-1",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "セキュリティ運用規約の初稿を作成する",
      "status": "completed",
      "artifacts": [
        "docs/security-policy-v1.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-1",
          "objective": "セキュリティ運用規約の初稿を執筆する",
          "criteria": [
            "docs/security-policy-v1.md が作成されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-9c0d1e2f-1",
              "started": "2026-02-01T15:30:00Z",
              "ended": "2026-02-01T16:15:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-9c0d1e2f-2",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "セキュリティ運用規約の初稿をレビューする",
      "status": "completed",
      "depends": [
        "tkt-9c0d1e2f-1"
      ],
      "refs": [
        {
          "ticket_id": "tkt-9c0d1e2f-1",
          "kind": "related",
          "note": "初稿 docs/security-policy-v1.md をレビュー"
        }
      ],
      "artifacts": [
        "reviews/security-policy-review-01.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-2",
          "objective": "初稿の記載漏れと基準適合性を検証し判定を記録する",
          "criteria": [
            "レビュー結果と判定（approved または changes_requested）が記録されている"
          ],
          "status": "completed",
          "result": "changes_requested",
          "artifacts": [
            "reviews/security-policy-review-01.md"
          ],
          "runs": [
            {
              "id": "run-9c0d1e2f-2",
              "started": "2026-02-01T16:30:00Z",
              "ended": "2026-02-01T16:45:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-9c0d1e2f-3",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "レビュー指摘に基づき鍵ローテーション手順を追記・検証する",
      "exclusions": [
        "承認済みのアクセス制御節（第1〜3章）の要件変更は行わない",
        "本番ゲートウェイの実運用鍵のローテーション実行は行わない"
      ],
      "status": "doing",
      "depends": [
        "tkt-9c0d1e2f-2"
      ],
      "refs": [
        {
          "ticket_id": "tkt-9c0d1e2f-1",
          "kind": "fixes",
          "note": "初稿 docs/security-policy-v1.md をベースに改訂"
        },
        {
          "ticket_id": "tkt-9c0d1e2f-2",
          "kind": "derived_from",
          "note": "changes_requested の指摘事項（鍵ローテーション手順の不足）を反映"
        }
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-3",
          "objective": "緊急時の鍵ローテーション手順節を追記する",
          "criteria": [
            "docs/security-policy-v2.md に鍵ローテーション手順が明記されている"
          ],
          "status": "doing",
          "runs": [
            {
              "id": "run-9c0d1e2f-3",
              "started": "2026-02-01T17:00:00Z"
            }
          ]
        },
        {
          "id": "job-9c0d1e2f-4",
          "objective": "全チェックリスト項目との整合を確認する",
          "criteria": [
            "チェックリスト全項目との照合が完了している"
          ],
          "status": "todo"
        }
      ]
    }
  ]
}
```

---

## UC-10: Task 完了後の整理（Ticket 肥大化対策）

### どのような場面か（シナリオ）
作成・レビュー差し戻し・修正・再レビューを経て複数の Ticket が作られた長寿命 `Task` が目標を達成して完了する際、終了済み旧 Ticket のアーカイブ・折りたたみ、揮発性 Handoff の消去、および Task 完了判定を行ってクリーンな終了状態にするシナリオ。

### 規格上のポイント（どの状態・フィールドを使うか）
- **終了済み旧 Ticket の即時アーカイブ・折りたたみ**：各 Ticket は終了（`completed` / `canceled`）時点で成果（`artifacts`）・結果（`result`）・中止理由（`cancel_reason`）を永続化する。次 Ticket が未作成でも終了済み Ticket は即座にアーカイブ・折りたたみ可能とする（[規格本文](specification.md) §4・§6）。チケット単位でファイルを分割している場合は、終了済み Ticket ファイルをアーカイブ移動し、Task 側のファイルには `project.tasks[]` と昇格した `project.knowledge[]` のみをクリーンに保持できる。
- **Ticket 肥大化対策と Task 完了判定**：
  - `refs`（`fixes`, `derived_from`, `rerun_of`, `related`）によって「初稿 → 差し戻しレビュー → 修正 → 承認レビュー」の系列を構造化し、終了済み旧 Ticket を折りたたんで管理負荷を抑える。
  - **完了 Ticket 数だけで Task の達成率や完了を決めてはならない**（[規格本文](specification.md) §2・§6）。`task_id` で紐づく全 Ticket が終端状態であり、差し戻しや中止で未達だった必須範囲が後続 Ticket で解決され、`Task.criteria` をすべて満たしていることを確認して `Task.status: "completed"` とする。
- **揮発性 Handoff の消去と長期知識の分離**：Task 完了時には全 Run が終了確定済みとなり、Task 内の揮発性 `handoff` スロットは長期記憶として残さず消去（省略）する。将来再利用する恒久知見のみを `Project.knowledge`（`status: "active"`、旧知識の置換時は旧記録を `"supersed"` にして `supersedes` で参照）に昇格させて保持する。

### サンプル JSON パターン

```json
{
  "protocol": "TTJ/1.1.0",
  "project": {
    "id": "pj-a1b2c3d4-1",
    "name": "API基盤開発",
    "knowledge": [
      {
        "id": "security-policy-v0-rule",
        "status": "supersed",
        "content": "署名鍵のローテーションは日次バッチで一括反映する",
        "basis": "初期設計時の暫定運用ルールとして採用していた。"
      },
      {
        "id": "security-policy-immediate-key-rotation",
        "status": "active",
        "content": "緊急時の署名鍵ローテーションは即時配信リストへ反映し5分以内に全ゲートウェイへ伝播させる",
        "basis": "tsk-e5f6a7b8-8 のレビューおよび改訂で合意された新基準。",
        "supersedes": "security-policy-v0-rule"
      }
    ],
    "tasks": [
      {
        "id": "tsk-e5f6a7b8-8",
        "objective": "APIセキュリティ運用規約を策定し承認を得る",
        "criteria": [
          "規約文書が作成され、レビュアーの承認（approved）を得ている"
        ],
        "status": "completed"
      }
    ]
  },
  "tickets": [
    {
      "id": "tkt-9c0d1e2f-1",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-1",
      "objective": "セキュリティ運用規約の初稿を作成する",
      "status": "completed",
      "artifacts": [
        "docs/security-policy-v1.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-1",
          "objective": "セキュリティ運用規約の初稿を執筆する",
          "criteria": [
            "docs/security-policy-v1.md が作成されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-9c0d1e2f-1",
              "started": "2026-02-01T15:30:00Z",
              "ended": "2026-02-01T16:15:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-9c0d1e2f-2",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-1",
      "objective": "セキュリティ運用規約の初稿をレビューする",
      "status": "completed",
      "depends": [
        "tkt-9c0d1e2f-1"
      ],
      "refs": [
        {
          "ticket_id": "tkt-9c0d1e2f-1",
          "kind": "related",
          "note": "初稿レビュー（changes_requested で終了・アーカイブ済み）"
        }
      ],
      "artifacts": [
        "reviews/security-policy-review-01.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-2",
          "objective": "初稿の記載漏れと基準適合性を検証し判定を記録する",
          "criteria": [
            "レビュー結果と判定（approved または changes_requested）が記録されている"
          ],
          "status": "completed",
          "result": "changes_requested",
          "runs": [
            {
              "id": "run-9c0d1e2f-2",
              "started": "2026-02-01T16:30:00Z",
              "ended": "2026-02-01T16:45:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-9c0d1e2f-3",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-1",
      "session_id": "ssn-2b3c4d5e-2",
      "objective": "レビュー指摘に基づき鍵ローテーション手順を追記・検証する",
      "status": "completed",
      "depends": [
        "tkt-9c0d1e2f-2"
      ],
      "refs": [
        {
          "ticket_id": "tkt-9c0d1e2f-1",
          "kind": "fixes",
          "note": "初稿を改訂し v2 を作成"
        },
        {
          "ticket_id": "tkt-9c0d1e2f-2",
          "kind": "derived_from",
          "note": "差し戻し指摘をすべて解消"
        }
      ],
      "artifacts": [
        "docs/security-policy-v2.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-3",
          "objective": "緊急時の鍵ローテーション手順節を追記する",
          "criteria": [
            "docs/security-policy-v2.md に鍵ローテーション手順が明記されている"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-9c0d1e2f-3",
              "started": "2026-02-01T17:00:00Z",
              "ended": "2026-02-01T17:15:00Z",
              "result": "succeeded"
            }
          ]
        },
        {
          "id": "job-9c0d1e2f-4",
          "objective": "全チェックリスト項目との整合を確認する",
          "criteria": [
            "チェックリスト全項目との照合が完了している"
          ],
          "status": "completed",
          "result": "passed",
          "runs": [
            {
              "id": "run-9c0d1e2f-4",
              "started": "2026-02-01T17:16:00Z",
              "ended": "2026-02-01T17:25:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    },
    {
      "id": "tkt-9c0d1e2f-4",
      "task_id": "tsk-e5f6a7b8-8",
      "worker_id": "wkr-7c8d9e0f-2",
      "session_id": "ssn-9a8b7c6d-2",
      "objective": "改訂版セキュリティ運用規約の最終承認レビューを行う",
      "status": "completed",
      "depends": [
        "tkt-9c0d1e2f-3"
      ],
      "refs": [
        {
          "ticket_id": "tkt-9c0d1e2f-3",
          "kind": "related",
          "note": "改訂版 docs/security-policy-v2.md を最終確認し approved"
        }
      ],
      "artifacts": [
        "reviews/security-policy-review-02.md"
      ],
      "jobs": [
        {
          "id": "job-9c0d1e2f-5",
          "objective": "改訂版規約を最終確認し承認判定を記録する",
          "criteria": [
            "指摘事項解消が確認され approved 判定が記録されている"
          ],
          "status": "completed",
          "result": "approved",
          "artifacts": [
            "reviews/security-policy-review-02.md"
          ],
          "runs": [
            {
              "id": "run-9c0d1e2f-5",
              "started": "2026-02-01T17:30:00Z",
              "ended": "2026-02-01T17:40:00Z",
              "result": "succeeded"
            }
          ]
        }
      ]
    }
  ]
}
```
