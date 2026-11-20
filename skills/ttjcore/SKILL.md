---
name: ttjcore
description: TTJCoreでProject・Task・Ticket・Job・Run・Knowledgeを管理し、CLIやJSON-RPCでCRUD、Runの記録、statusの変更、eventの監視を行う。TTJに従う作業をLLMから進める場合に使う。
---

# TTJCore

計画・依存・実行・完了判定・申し送りの規則は、同梱の[TTJ 1.0.0規格本文](references/ttj-1.0.0/specification.md)を読む。記入項目と例は[JSON参照形式](references/ttj-1.0.0/json-format.md)で確認する。これらは[TTJ v1.0.0](https://github.com/northwest-systems/TTJ/tree/v1.0.0)の固定版を同梱しており、ネット接続や別のTTJ skillの設置なしで参照できる。[ライセンス](references/ttj-1.0.0/LICENSE)も同梱する。

Coreの利用契約は[protocol](references/protocol.md)を読む。接続・method・record・event・errorをこの文書から確認し、実機の`ttjcore methods`で対応methodとfieldを確認する。

Coreへ直接接続する。標準接続では`ttjcore ensure`で準備し、custom profileでは全コマンドに同じ`--home <data-dir>`を指定する。`project.list`から対象Projectを選び、`project.snapshot`でTask・Ticket・Job・Knowledgeとtimelineを取得する。

変更には実際のactor、reason、再送用の固有operationを指定する。IDは通常省略してCoreの発行結果を使う。更新・削除・statusの変更では最新のrevisionを使う。同じ操作の再送はoperationとrequestを維持し、conflict時は読み直す。

Ticketは1つのTaskに所属する。Ticketと全Jobを一括作成し、固定されたobjective・criteria・instructions・Jobの順序を後から書き換えない。summaryやiconはrecordとは別のdisplayに指定する。

実行前にdependsと先行Jobを確認する。run.start・run.finishで実際のRunを記録し、Jobをcompletedにする前にcriteriaを確認する。Runの終了だけでJobをcompletedと扱わない。statusやhandoffs、resumeは実態に合わせて記録する。Coreはagentのprocessを起動・停止・自動検出しない。

Taskの閲覧用の完了と削除を分ける。task.purgeはTask自身と配下がcompletedまたはcanceledで有効なRunがなく、他TaskのTicketから参照されていない場合だけ使う。対象Taskと全配下を物理削除し、詳しい履歴と再送データを要約・失効記録へ整理する。必要な詳しい内容は日々ProjectのKnowledgeへ保存し、Task削除に連動してKnowledgeを失わせない。理由の入力は不要で、actor・id・revisionと再送用operationを指定する。storageがpendingなら、削除は完了したが容量回収は未完了であることを伝える。

監視はCoreのevents.wait、またはrpcのevents.subscribeを使う。最後に受け取ったcursorから再接続する。UIから状態を取得しない。
