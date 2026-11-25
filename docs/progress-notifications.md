# 進捗APIとOrcaへの通知

Runの担当Sessionは進捗率と短いメッセージを報告できます。ブラウザUIと端末はCoreの同じ情報を参照します。進捗は運用情報であり、業務状態・達成結果・再開条件とは分けて扱います。

## 進捗を報告する

`run.progress`は現在Jobの有効なRunだけが使えます。`id`・最新の`revision`と、固定された`[worker_id, session_id]`を`actor`に指定します。管理モード、終了済みSession、終了・失効したRun、別担当からの更新は拒否します。

```json
{
  "actor": ["wkr-abcdef-1", "ssn-abcdef-1"],
  "reason": "検証の進行を報告する",
  "operation": "verify-progress-37",
  "id": "run-abcdef-1",
  "revision": 2,
  "percent": 37,
  "message": "テスト結果を確認中"
}
```

| パラメータ | 扱い |
|---|---|
| percent | 0〜100の整数。nullで消去 |
| message | 空でない1行、UTF-8で2048 bytes以内。制御文字は禁止。nullで消去 |
| operation | 通常の更新と同じ再送用ID。省略時はCoreが発行 |

percent・messageの少なくとも一方を指定します。省略した方は保持します。同じ内容の報告はrevisionや変更eventを増やしません。変更と`run.progress_changed`の履歴は同じDB transactionで保存し、失敗時はどちらも変更しません。

応答は通常の更新と同じ`result.value`で、Run wrapperの`progress`に報告値を返します。`run.get`・`run.list`でも参照できます。標準TTJ recordや`constraints`には混在させません。

```json
{"progress":{"percent":37,"message":"テスト結果を確認中"}}
```

100%の報告やRunの終了だけでJobは完了しません。達成条件を確認して通常の完了操作を行います。新しいRunに旧Runの進捗を引き継ぎません。保留中にも停止作業などの観測結果は報告できますが、実行の再開を許可するものではありません。

進捗はDBへ保存しますが、TTJ 1.1.0の標準JSONには追加せずdocument.export/importの対象外です。正式な成果・待機条件・再開根拠は既存のresult・Run.wait・resumeなどへ記録します。

## 現在の進捗を一括取得する

`ticket.snapshot`に`{"id":"<ticket-id>"}`を渡すと、次の情報を同じ読み取りtransactionから返します。

| キー | 内容 |
|---|---|
| project_id | 所属Project ID |
| task / ticket | 親Taskと対象Ticketのwrapper |
| job | 最初の非終端Jobのwrapper。全Jobが終端ならnull |
| run | 現在Jobの最新Runのwrapper。未試行ならnull |
| cursor | 同じtransactionで確認したeventの位置 |

取得後は、そのproject_idとcursorでevents.waitまたはevents.subscribeを使い、変更後に再取得します。初回表示と監視開始の間の更新を取りこぼさず、異なる時点のJob・Runを合成することも避けられます。

## CLIで監視する

```sh
# 1行ずつJSONのsnapshotを受け取る
ttjcore --home /path/to/profile watch <ticket-id>

# OrcaなどOSC 7501対応端末に状態・進捗率・メッセージを表示する
ttjcore --home /path/to/profile watch <ticket-id> --osc7501
```

watchは読み取り専用です。Coreの起動、作業の開始・停止、状態変更は行いません。Ticketがcompleted/canceledになると最終表示後に終了します。Ctrl+Cは監視だけを停止します。再接続時は再実行し、最新snapshotから再開します。

JSON出力には制御シーケンスを混ぜません。`--osc7501`はstdoutが端末の場合だけ使えます。パイプやファイルにはJSON形式を使います。OSCを出すのは常駐Coreではなく、表示先端末へ接続したwatchです。

## OSC 7501への対応付け

専用の監視端末に1つのTicketを表示します。appは`ttjcore`、titleは現在Job（なければTicket）のobjective、msgは状態と報告メッセージです。報告がなければobjectiveを使います。

| TTJの状態・条件 | OSC state |
|---|---|
| Ticket completed | done |
| Ticket canceled、親Task/対象Ticket/現在Jobのpaused、停止要求付きRun | idle |
| 対象Ticket/現在Jobのblocked | idle。msgにblockedを明記 |
| 対象Ticket/現在Jobのwaiting | working。msgにwaitingを明記 |
| doingで有効なRunがある | working |
| todo、Run終了後の確認待ちなど | idle |
| Coreへの接続・監視に失敗した | error。業務状態は変更しない |

外部応答待ちをユーザーへの承認要求・質問と推測せず、OSCのblocked/kindは自動生成しません。percentはworkingかつ有効なRunに報告値がある場合だけ表示します。Taskの達成率をTicket件数から生成しません。

形式は[Program Status Protocol revision 0.4](https://www.superlogical.com/rex/docs/build/program-status)に従います。title/msgはUTF-8から標準Base64へ変換し、制御文字を空白に置換して、文字境界を保ったままそれぞれ192/2048 bytes以内に収めます。毎回全キーを出し、古い進捗率を残しません。中断時はidleを出します。

符号化にはCargoが取得するbase64 crateを使います。Orcaは[OSC 7501と既存hookを併用](https://github.com/stablyai/orca/pull/30261)するため、agentとは別の監視端末で利用できます。OSCは実行認可やOrcaへのコマンド送信を行いません。
