---
name: ttjcore
description: TTJCoreでProject・Task・Ticket・Job・Run・Knowledgeを管理し、CLIやJSON-RPCでCRUD、Runの記録、statusの変更、eventの監視を行う。TTJに従う作業をLLMから進める場合に使う。
---

# TTJCore

利用契約は[protocol](references/protocol.md)を読む。接続・method・record・event・errorをこの文書から確認し、実機の`ttjcore methods`で対応methodとfieldを確認する。

Coreへ直接接続する。標準接続では`ttjcore ensure`で準備し、custom profileでは全コマンドに同じ`--home <data-dir>`を指定する。`project.list`から対象Projectを選び、`project.snapshot`でTask・Ticket・Job・Knowledgeとtimelineを取得する。

変更には実際のactor、reason、再送用の固有operationを指定する。IDは通常省略してCoreの発行結果を使う。更新・削除・statusの変更では最新のrevisionを使う。同じ操作の再送はoperationとrequestを維持し、conflict時は読み直す。

Ticketは1つのTaskに所属する。Ticketと全Jobを一括作成し、固定されたobjective・criteria・instructions・Jobの順序を後から書き換えない。summaryやiconはrecordとは別のdisplayに指定する。

実行前にdependsと先行Jobを確認する。run.start・run.finishで実際のRunを記録し、Jobをcompletedにする前にcriteriaを確認する。Runの終了だけでJobをcompletedと扱わない。statusやhandoffs、resumeは実態に合わせて記録する。Coreはagentのprocessを起動・停止・自動検出しない。

監視はCoreのevents.wait、またはrpcのevents.subscribeを使う。最後に受け取ったcursorから再接続する。UIから状態を取得しない。
