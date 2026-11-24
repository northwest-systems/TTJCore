# Bashでの操作例

TTJ 1.1.0対応の`ttjcore`と、JSON処理用の`jq`をPATHに置き、新しいBashで順に実行します。専用の一時profileでProjectを作成し、名前の変更から作業完了まで確認します。

```bash
# サンプル専用のSQLiteと管理主体を使ってCoreを起動する
set -euo pipefail
sample_home=$(mktemp -d /tmp/ttj-sample.XXXXXXXX)
unset TTJCORE_DATABASE_URL
export TTJCORE_MANAGEMENT_ACTORS='["sample-owner"]'
ttjcore --home "$sample_home" ensure

# JSON応答の取り出し・管理操作の共通パラメータ・参照を補助関数にまとめる
ttj() {
  ttjcore --home "$sample_home" call "$1" --params "$2" \
    | jq -ce '.result | if has("value") then .value else . end'
}
ttj_manage() {
  ttj "$1" "$(jq -c '. + {mode:"management",actor:["sample-owner"],reason:"サンプル操作"}' <<< "$2")"
}
ttj_get() { ttj "$1.get" "$(jq -cn --arg id "$2" '{id:$id}')"; }

# 作業担当のWorkerと、その実行単位となるSessionを登録する
worker=$(ttj_manage worker.create '{"record":{"name":"サンプル担当者"}}')
session=$(ttj_manage session.create "$(jq -cn --argjson w "$worker" '{record:{worker_id:$w.record.id}}')")

# Project・目標のTask・担当SessionとJobを固定したTicketを作成する
project=$(ttj_manage project.create '{"record":{"name":"CLIサンプル"}}')
project_id=$(jq -er '.record.id' <<< "$project")
task=$(ttj_manage task.create "$(jq -cn --arg id "$project_id" \
  '{parent:$id,record:{objective:"Project名を変更して確認する",criteria:["変更後の名前を取得できる"]}}')")
ticket=$(ttj_manage ticket.create "$(jq -cn --argjson t "$task" --argjson w "$worker" --argjson s "$session" \
  '{record:{task_id:$t.record.id,worker_id:$w.record.id,session_id:$s.record.id,objective:"Project名の更新と確認",
    jobs:[{objective:"Project名の確認",criteria:["名前がCLIサンプル（確認済み）である"]}]}}')")
job_id=$(jq -er '.record.jobs[0].id' <<< "$ticket")

# 現在JobのRunを、固定されたWorker/Sessionとして開始する
job=$(ttj_get job "$job_id")
execution=$(jq -cn --argjson w "$worker" --argjson s "$session" \
  '{actor:[$w.record.id,$s.record.id],reason:"Project名の確認"}')
run=$(ttj run.start "$(jq -cn --argjson common "$execution" --argjson j "$job" \
  '$common + {job:$j.record.id,revision:$j.revision}')" | jq -ce '.run')

# 最新revisionでProject名を更新し、取得結果を検証する
project=$(ttj_get project "$project_id")
ttj_manage project.update "$(jq -cn --argjson p "$project" \
  '{id:$p.record.id,revision:$p.revision,patch:{name:"CLIサンプル（確認済み）"}}')" >/dev/null
project=$(ttj_get project "$project_id")
jq -e '.record.name == "CLIサンプル（確認済み）"' <<< "$project"
jq '.record | {id,name}' <<< "$project"

# Runを終了する。この操作だけではJobは完了しない
run=$(ttj_get run "$(jq -er '.record.id' <<< "$run")")
ttj run.finish "$(jq -cn --argjson common "$execution" --argjson r "$run" \
  '$common + {id:$r.record.id,revision:$r.revision,result:"succeeded"}')" >/dev/null

# 管理主体がJob→Taskの順に完了を確定する。TicketはJob完了時に自動で完了する
for row in "$job" "$task"; do
  row=$(ttj_get "$(jq -r '.kind' <<< "$row")" "$(jq -er '.record.id' <<< "$row")")
  ttj_manage status.set "$(jq -cn --argjson r "$row" \
    '{id:$r.record.id,revision:$r.revision,status:"completed",criteria_met:true,
      result:"project.getで名前がCLIサンプル（確認済み）であることを確認した"}')" >/dev/null
done

# Task・Ticket・Jobがすべてcompletedであることを表示し、サンプルのCoreを停止する
snapshot=$(ttj project.snapshot "$(jq -cn --arg id "$project_id" '{id:$id}')")
jq -r '.tasks[0] | [.task.record,.tickets[0].ticket.record,.tickets[0].jobs[0].record][]
  | [.id,.status] | @tsv' <<< "$snapshot"
ttjcore --home "$sample_home" stop
```

既存のprofileは変更せず、記録は`$sample_home`内のSQLiteに残ります。`ttj`は参照結果の`result`と更新結果の`result.value`を取り出します。各methodの詳細は[protocol](../skills/ttjcore/references/protocol.md)を参照してください。
