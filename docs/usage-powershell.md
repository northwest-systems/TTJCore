# PowerShell 7での操作例

TTJ 1.1.0対応の`ttjcore`をPATHに置き、新しいPowerShell 7で順に実行します。専用の一時profileでProjectを作成し、名前の変更から作業完了まで確認します。

```powershell
# サンプル専用のSQLiteと管理主体を使ってCoreを起動する
$sampleHome = Join-Path ([IO.Path]::GetTempPath()) ("ttj-sample-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
Remove-Item Env:TTJCORE_DATABASE_URL -ErrorAction SilentlyContinue
$env:TTJCORE_MANAGEMENT_ACTORS = '["sample-owner"]'
ttjcore --home $sampleHome ensure

# JSON変換と応答の取り出しだけを補助関数にまとめる
function Ttj($method, $params) {
    $json = $params | ConvertTo-Json -Depth 6 -Compress
    $wire = ttjcore --home $sampleHome call $method --params $json
    if ($LASTEXITCODE -ne 0) { throw ($wire -join "`n") }
    $result = ($wire | ConvertFrom-Json).result
    if ($result.PSObject.Properties["value"]) { $result.value } else { $result }
}
$management = @{ mode = "management"; actor = @("sample-owner"); reason = "サンプル操作" }

# 作業担当のWorkerと、その実行単位となるSessionを登録する
$worker = Ttj worker.create ($management + @{ record = @{ name = "サンプル担当者" } })
$session = Ttj session.create ($management + @{ record = @{ worker_id = $worker.record.id } })

# Project・目標のTask・担当SessionとJobを固定したTicketを作成する
$project = Ttj project.create ($management + @{ record = @{ name = "CLIサンプル" } })
$task = Ttj task.create ($management + @{
    parent = $project.record.id
    record = @{ objective = "Project名を変更して確認する"; criteria = @("変更後の名前を取得できる") }
})
$ticket = Ttj ticket.create ($management + @{ record = @{
    task_id = $task.record.id; worker_id = $worker.record.id; session_id = $session.record.id
    objective = "Project名の更新と確認"
    jobs = @(@{ objective = "Project名の確認"; criteria = @("名前がCLIサンプル（確認済み）である") })
} })
$jobId = $ticket.record.jobs[0].id

# 現在JobのRunを、固定されたWorker/Sessionとして開始する
$job = Ttj job.get @{ id = $jobId }
$execution = @{ actor = @($worker.record.id, $session.record.id); reason = "Project名の確認" }
$run = (Ttj run.start ($execution + @{ job = $jobId; revision = $job.revision })).run

# 最新revisionでProject名を更新し、取得結果を検証する
$project = Ttj project.get @{ id = $project.record.id }
$project = Ttj project.update ($management + @{
    id = $project.record.id; revision = $project.revision; patch = @{ name = "CLIサンプル（確認済み）" }
})
$project = Ttj project.get @{ id = $project.record.id }
if ($project.record.name -ne "CLIサンプル（確認済み）") { throw "Project名が一致しません" }
$project.record | Select-Object id, name | Format-Table -AutoSize

# Runを終了する。この操作だけではJobは完了しない
$run = Ttj run.get @{ id = $run.record.id }
Ttj run.finish ($execution + @{ id = $run.record.id; revision = $run.revision; result = "succeeded" }) | Out-Null

# 管理主体がJob→Taskの順に完了を確定する。TicketはJob完了時に自動で完了する
foreach ($row in @($job, $task)) {
    $row = Ttj "$($row.kind).get" @{ id = $row.record.id }
    Ttj status.set ($management + @{
        id = $row.record.id; revision = $row.revision; status = "completed"; criteria_met = $true
        result = "project.getで名前がCLIサンプル（確認済み）であることを確認した"
    }) | Out-Null
}

# Task・Ticket・Jobがすべてcompletedであることを表示し、サンプルのCoreを停止する
$snapshot = Ttj project.snapshot @{ id = $project.record.id }
@(
    $snapshot.tasks[0].task.record
    $snapshot.tasks[0].tickets[0].ticket.record
    $snapshot.tasks[0].tickets[0].jobs[0].record
) | Select-Object id, status | Format-Table -AutoSize
ttjcore --home $sampleHome stop
```

既存のprofileは変更せず、記録は`$sampleHome`内のSQLiteに残ります。補助関数は参照結果の`result`と更新結果の`result.value`を取り出します。各methodの詳細は[protocol](../skills/ttjcore/references/protocol.md)を参照してください。
