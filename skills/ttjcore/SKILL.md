---
name: ttjcore
description: TTJCoreでProject・Task・Ticket・Job・Run・Worker・Session・Knowledgeを管理し、CLIやJSON-RPCでCRUD、Runの記録、statusの変更、eventの監視を行う。毎セッション開始・引き継ぎ時のKnowledge必読や、該当Ticket処理時のHandoff参照を含め、TTJに従う作業をLLMから進める場合に使う。
---

# TTJCore

計画・依存・実行・完了判定・申し送りの規則は、同梱の[TTJ 1.1.0規格本文](references/ttj-1.1.0/references/specification.md)を読む。記入項目と例は[JSON参照形式](references/ttj-1.1.0/references/json-format.md)で確認する。これらは[TTJ v1.1.0](https://github.com/northwest-systems/TTJ/tree/1907cbcea7de364f6ca882556c05801a8c4eacdc)の固定版を同梱しており、ネット接続や別のTTJ skillの設置なしで参照できる。[ライセンス](references/ttj-1.1.0/LICENSE)も同梱する。

Coreの利用契約は[protocol](references/protocol.md)を読む。接続・method・record・event・errorをこの文書から確認し、実機の`ttjcore methods`で対応methodとfieldを確認する。

## セッション開始・引き継ぎ時の必須ルール（Knowledge 必読 / Handoff 対象限定）

- **Knowledge は毎セッション開始・引き継ぎ時に自発的に読みに行く（必須）**：
  新規セッションを開始したとき、および別セッションや別エージェントから作業を引き継いだときは、**ユーザーから指示されなくても、計画・実装・回答に入る前に LLM が自発的かつ最初に**対象 Project の Knowledge を取得して読む。前セッションの要約や会話履歴だけに頼らず、Core 上の最新の `active` Knowledge（内容 `content` と根拠 `basis`）を必ず確認する。通常は `status: "active"` のみを使い、振り返り・再発障害・過去判断の調査時だけ `supersed` も参照する。
- **Handoff は該当 Ticket を処理するときだけ読む（対象限定）**：
  `Task.handoff` は、**自分が着手・継続・引き継ぎ処理する該当 Ticket の作業文脈を確認するときだけ**参照する。毎セッション開始時に Project 内の全 Task・全 Ticket の Handoff を読みに行く必要はない（コンテキストの浪費を防ぎ、担当 Ticket の文脈だけに絞る）。

### Knowledge と Handoff の使い分け

| 項目 | Knowledge（`Project.knowledge`） | Handoff（`Task.handoff`） |
|---|---|---|
| 読むタイミング | **毎セッション開始時・セッション引き継ぎ時に自発的かつ必須** | **該当 Ticket を着手・継続・引き継ぎ処理するときのみ** |
| 対象範囲 | Project 全体の継続的な知見・暗黙ルール・設計判断と根拠 | 特定 Ticket 処理中の作業文脈・停止位置・ブロッカー |
| 性質・寿命 | 永続（`active` / `supersed`）。Task 完了・削除後も保持 | 揮発性単一スロット。Task 完了時に自動消去 |
| 毎セッション全件確認 | **必須**（対象 Project の `active` Knowledge を必ず読む） | **不要**（処理対象の該当 Ticket だけ読む） |
| 主な参照コマンド | `knowledge.list` / `knowledge.get` | `ticket.snapshot` / `task.get` |

### セッション開始・引き継ぎの標準フロー

Core へ直接接続する。標準接続では `ttjcore ensure` で準備し、custom profile では全コマンドに同じ `--home <data-dir>` を指定する。

1. **Core 接続と対象 Project の特定**：
   ```sh
   ttjcore ensure
   ttjcore call project.list
   ```
2. **現行 Knowledge の自発的な読み込み（毎セッション必須・最優先）**：
   対象 `<project-id>` の Knowledge を取得し、`record.status` が `"active"` の全件について `content` と `basis` を読む。特定の Knowledge を個別に確認・再取得する場合は `knowledge.get` を使う。
   ```sh
   ttjcore call knowledge.list --params '{"parent":"<project-id>"}'
   ttjcore call knowledge.get --params '{"id":"<knowledge-id>"}'
   ```
3. **該当 Ticket を処理する場合のみ、Ticket 状態と親 Task の Handoff を取得**：
   処理対象の `<ticket-id>` が決まっているときは `ticket.snapshot` で対象 Ticket・現在 Job・最新 Run と親 Task（`task.record.handoff`）を一括取得する。親 Task の Handoff だけを確認する場合は `task.get` を使う。
   ```sh
   ttjcore call ticket.snapshot --params '{"id":"<ticket-id>"}'
   ttjcore call task.get --params '{"id":"<task-id>"}'
   ```
   - `Task.handoff` は Task 内の揮発性単一スロットのため、`current_ticket_id` や Ticket の `refs` を確認して処理対象 Ticket の文脈かを判別する。
   - Handoff は補足キャッシュであり、再開条件・前提依存・中止理由・成果物の正本は永続レコード（`depends`・`Run.wait`・`Run.result`・`Ticket.cancel_reason`・`resume`・`artifacts`）で確認する。
   - Handoff を閲覧できることは Ticket の実行権限を意味しない（確定済みの `worker_id`・`session_id` が一致する担当 Session のみが実行できる）。
4. **Project 全体の俯瞰が必要な場合のみ `project.snapshot` を使う**：
   Task 一覧・Ticket 構造・依存・timeline をまとめて把握したいときだけ `ttjcore call project.snapshot --params '{"id":"<project-id>"}'` を使う（`project.snapshot` の `knowledge` には `active` Knowledge が含まれる）。

## 更新・実行・運用ルール

変更には実際のactor、reason、再送用の固有operationを指定する。実行由来の変更では現行runを必ず指定する。管理・範囲変更・文書importではmode=managementと外部設定で許可された管理主体を指定し、実行元runと混在させない。終了後の承認はjob.accept_evidenceで検証内容を記録し、waitの解決とJobの遷移をまとめて確定する。IDは通常省略してCoreの発行結果を使う。更新・削除・statusの変更では最新のrevisionを使う。同じ操作の再送はoperationとrequestを維持し、conflict時は読み直す。

Ticketはtask_idで1つのTaskに所属し、確定後のworker_id・session_idは変更しない。実行時のactorには固定のworker_id・session_idをこの順で指定する。Taskの作成ではProject IDをrecord内ではなくparams.parentに渡す。Ticketと全Jobを一括作成し、固定されたobjective・criteria・instructions・Jobの順序を後から書き換えない。summaryやiconはrecordとは別のdisplayに指定する。

実行前にdependsと先行Jobを確認する。run.start・run.finishで実際のRunを記録し、Jobをcompletedにする前にcriteriaを確認する。Runの終了だけでJobをcompletedと扱わない。statusやTask.handoff、resumeは実態に合わせて記録する。Coreはagentのprocessを起動・停止・自動検出しない。

Taskの完了と削除を分ける。task.purgeはTask自身と配下がcompletedまたはcanceledで有効なRunがなく、他TaskのTicketから参照されていない場合だけ使う。対象Taskと全配下を物理削除し、詳しい履歴と再送データを要約・失効記録へ整理する。必要な詳しい内容は日々ProjectのKnowledgeへ保存し、Task削除に連動してKnowledgeを失わせない。理由の入力は不要で、actor・id・revisionと再送用operationを指定する。storageがpendingなら、削除は完了したが容量回収は未完了であることを伝える。

監視はCoreのevents.wait、またはrpcのevents.subscribeを使う。最後に受け取ったcursorから再接続する。UIから状態を取得しない。
