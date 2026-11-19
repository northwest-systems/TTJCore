---
name: ttjcore
description: TTJCoreでProject・Task・Ticket・Job・Run・knowledgeを管理し、CLIやJSON-RPCでCRUD、実行記録、状態変更、変更の監視を行う。TTJに従う作業をLLMから進める場合に使う。
---

# TTJCore

`ttjcore`を直接利用する。UIから状態を取得しない。導入やDB切替が必要な場合は[README](../../README.md)のクイックスタート・DBとプロファイルを読む。

1. 対象プロファイルで`ttjcore ensure`を実行する。カスタム接続では全コマンドに同じ`--home <data-dir>`を指定する。
2. `ttjcore methods`で利用可能なメソッド・必要項目を確認する。`project.list`から対象Projectを選び、`project.snapshot`でTask・Ticket・Job・履歴・knowledgeを読む。
3. 変更には実際の参加者を表すactorの配列、理由reason、再送用の固有operationを指定する。JSONは`call METHOD --file <params.json>`、または別引数の`--params JSON`で渡す。
4. 作成IDは通常省略し、`result.value`の生成IDを次の操作に使う。更新・削除・状態変更では最新のgetまたはsnapshotのrevisionを使う。同じ操作の再送はoperationと要求を完全に維持し、競合時は読み直す。

TaskはProjectに、Ticketは1つのTaskに所属する。Ticketと非空のJob定義を一括作成する。Jobの目標・条件・手順・順序とTicketの目標は作成後に固定される。カード用概要はトップレベルのdisplay.summary、Jobアイコンキーは作成時の各Job定義のdisplay.icon、更新時はjob.updateのトップレベルdisplay.iconで指定する。これらはTTJ記録本体に入れない。

実行前に依存Ticketと先行Jobを確認する。run.startには対象Jobの最新revisionを渡す。返ったRun IDとrevisionを保存し、実際の実行結果をrun.finishへ記録する。Jobを完了するには、実際に条件を確認してstatus.setへcriteria_met=trueと根拠resultを渡す。Run終了だけでJobを完了扱いにしない。待機・障害・保留は実態に合わせて記録し、解除時は必要なresolved・releaseを指定する。

Ticketは全Jobが終端になると集約される。Taskは配下Ticketの終端に加え、Task自身の条件確認が必要。削除は配下の作業が完了または中止した後に行う論理削除で、ID・履歴・依存参照は保持される。knowledgeの内容訂正は新規knowledge.createのsupersedesで旧記録へ結び付ける。

監視はevents.waitへ最後に受け取ったcursorを渡す。常時購読する場合は`ttjcore rpc`にJSON-RPC 2.0、protocol="TTJ/1.0.0"を付けたevents.subscribeを送り、ttj.changedを受け取る。切断時は最後のcursorから再接続する。Coreはエージェントのプロセスを起動・停止・自動検出しないため、実行側で状態と停止結果を確認して記録する。
