---
name: ttj
description: TTJ v1.1.0 に基づき、Worker と Session を分離した Task・Ticket・Job・Run の計画と実行管理、実行依存と情報共有参照、Task 所属の揮発性 Handoff、Project の長期知識を管理・点検する。TTJ の適用や準拠確認を求められた場合に使う。
metadata:
  version: "1.1.0"
---

# TTJ

- [必須] **適用**：[規格本文](references/specification.md) を正本とし、[JSON参照形式](references/json-format.md) で記入項目を、[代表ユースケースとサンプルパターン](references/use-cases.md) で場面別の状態遷移と JSON パターンを確認する。
  - [禁止] Worker 種別（人間・Bot・AI 等）で業務ロジックや状態遷移を分岐させる、または `owner_type` を必須にする。
  - [禁止] 特定の Agent・Bot・オーケストレーターへの依存を TTJ の規則へ持ち込む。
  - [必須] ラベルはその行だけに適用し、配下の項目へ継承しない。
  - [必須] JSON を読み書きする場合はルートの `protocol` で規格名と版を確認し、出力には `"TTJ/1.1.0"` を付ける。
  - [必須] JSON は「0〜複数があり得る項目（`workers`, `sessions`, `knowledge`, `tasks`, `tickets`, `jobs`, `runs` 等）のみ配列（空配列 `[]`・1要素 `[ {...} ]`・複数要素 `[ {...}, {...} ]` のいずれも許容、空配列時は記載省略可）とし、1つだけに定まっている項目（ルートの `project`、`Task.handoff`、`Run.wait`）は配列にせず単一オブジェクト（記載しない場合は省略可）とする」原則に従う。ルートを `protocol`・`workers`・`sessions`・`project`・`tickets` の5キーのみとし、Project / Task 階層は単一オブジェクト `project` 配下の `project.tasks[]`（および `project.knowledge[]`・`project.tasks[].handoff`。`Task` 内に `tickets` は持たない）、Ticket 以下の作業階層はルート直下の `tickets[].jobs[].runs[]`（各 `Ticket` に親 Task を示す `task_id` 必須）に統一する（逆参照の `Task.project`・`Handoff.task_id` や、`Ticket.jobs` の ID 配列／実体分離は廃止）。これにより同一スキーマのまま、Task 側のファイル（`protocol` と `project` 等のみ、`tickets` 省略）と Ticket 側のファイル（`protocol` と `tickets` のみ、`project` 等省略）に分割してチケット単位で排他操作することも、1ファイルに同居させることもできる。
  - [禁止] 同じ文書の子レコードへ版情報を転記する。

## 計画

- [必須] **目標と主体確認**：Project・Task・Worker・Session を確認する。
  - [必須] Project の `id`（接頭辞 `pj-`）・`name`、Worker の `id`（接頭辞 `wkr-`）・`name`、Session の `id`（接頭辞 `ssn-`）・`worker_id` で対象と作業主体・実行単位を識別できるようにする。
  - [必須] 重複を避けるため、Project・Task・Ticket・Job・Run・Worker・Session（全7種）の ID は自動採番とし、それぞれ `pj-`・`tsk-`・`tkt-`・`job-`・`run-`・`wkr-`・`ssn-` の接頭辞を用いた `<接頭辞>-<英数字6桁または8桁>-<インクリメント（ゼロ埋めなしの1始まり整数: 1, 2, 3...）>` 形式（正規表現：`^(pj|tsk|tkt|job|run|wkr|ssn)-[A-Za-z0-9]{6}([A-Za-z0-9]{2})?-[1-9][0-9]*$`、例：`pj-a1b2c3d4-1`・`tsk-e5f6a7b8-1`・`tkt-c9d0e1f2-1`・`job-1a2b3c4d-1`・`run-5e6f7a8b-1`・`wkr-7c8d9e0f-1`・`ssn-2b3c4d5e-1`）に統一する。
  - [必須] 既存の作業と採用中（`active`）の知識を先に読み、結果を左右する不足だけを質問する。
  - [禁止] Project をリポジトリに限定する。
- [必須] **記入**：Task・Ticket・Job すべての `objective` に「達成したいこと」を書く。
  - [必須] Task・Job の `criteria` に確認可能な条件を1件以上書く。
  - [任意] 期限が指定されている場合だけ、Task の `deadline` にタイムゾーン付き ISO 8601 日時を記録する。
  - [禁止] 未指定の期限を作る、または期限を Ticket・Job へ転記する。
  - [必須] Ticket（`tkt-*`）はルート直下の `tickets[]` の要素として配置し、所属する親 Task の `task_id`（`tsk-*`）、担当する単一の `worker_id`（`wkr-*`）と `session_id`（`ssn-*`）を指定し、`jobs` に `Job` オブジェクト（`job-*`）の実体を実行順に格納する（`Task` の内側に `tickets` は持たない）。
  - [任意] その Ticket でやらないこと（対象外・除外事項）がある場合は Ticket の `exclusions`（文字列の配列、空配列時は省略可）に明記し、LLM や担当者間の認識すり合わせ精度を上げる（`Job` には置かない）。
  - [禁止] Ticket に複数担当を設定する、Job ごとに個別担当を設定する、確定後に Ticket の `worker_id` や `session_id` を変更する、旧 `assignee` や `Ticket.handoffs` を使う、Ticket 独自の `criteria` を設ける。
  - [任意] 手順が不足する場合だけ Job の `instructions` を補う。
  - [禁止] Job に複雑なワークフロー構造・子 Job・待機制御構造・独立 `Request` オブジェクト・種別分類を追加する。
  - [禁止] 同じ目標・条件・結果を別の欄へ転記する。
- [必須] **正式作成**：Ticket と全 Job を一括確定する。
  - [必須] Job 列で Ticket の目標を達成できる計画か確認する。
  - [任意] 正式作成前の Draft や Ticket テンプレートを編集する。
  - [必須] 初期 `status` を `todo` にし、確定後は担当セッション（`worker_id`・`session_id`）と Job 列および作業定義を変更しない。
  - [必須] Ticket 増加への対策（テンプレート・標準 Job 列・後続 Ticket 生成支援・成果物/中止理由参照・系列表示と終了折りたたみ・Task 進捗集約と現在 Handoff 表示）を運用・実装に備える（具体的な形式や UI は選べる）。
  - [禁止] 修正・再実行・再レビューによる Ticket 増加を理由に新しい Task を増殖させる。
- [任意] **実行依存と情報共有参照**：`depends` と `refs` を明確に区別して記録する。
  - [任意] 先に `completed` である必要がある前提 Ticket だけを `depends` に指定する。
  - [必須] `depends` と非終端 Ticket・Job が保持するすべての未解決 `Run.wait` を合わせた完了依存グラフに循環がないことを確認する（解消済み・解除済み待機は履歴に保持しつつ辺から外す）。
  - [任意] 前 Ticket の成果物・結果・中止理由・派生元・修正対象・再実行元は情報共有専用の `refs` に記録する。
  - [禁止] `refs` を開始条件や完了依存として扱う。
- [必須] **保存**：ルートスキーマ（`project.tasks[]` とルート `tickets[].jobs[].runs[]`）の整合性を維持する（単一ファイル構成・チケット単位の分割ファイル構成のいずれも同一スキーマに従う）。
  - [禁止] 専用ツール・データベース・追加ファイルを一律に導入する。
  - [必須] 履歴・原子性・排他・外部副作用防止を保証できない場合は未対応範囲を示す。

## 実行と終了

- [必須] **開始前確認**：依存・順序・状態・担当セッション・安全条件を確認する。
  - [必須] 前提 Ticket が `completed`、先行 Job が `completed` または `canceled`。
  - [必須] 対象 Ticket および現在 Job が `todo` または `doing`、親 Task が保留（`paused`）・終端ではない（親 Task が別 Ticket 起因で `waiting`・`blocked` でも独立した実行可能 Ticket・Job は開始できる）。
  - [必須] 実行する Session が対象 Ticket の `worker_id`（`wkr-*`）および `session_id`（`ssn-*`）と一致し、同一 Ticket で他に稼働中の有効な `Run` が存在しないことを確認する。
  - [禁止] 同じ Worker であることを理由に、対象 Ticket の `session_id` と異なる別 Session による無断実行を許可する。
  - [禁止] `Ticket.exclusions` に記載された対象外・除外事項の作業を行う。
  - [必須] 同一 Ticket・同一 Session 内の通常再試行および（Job 単独中止後の）後続 Job 開始では、前の Run を閉鎖した上で停止確認または安全条件を確認してから新しい `Run` を開始する（残存副作用の疑いがあれば停止確認・復旧まで開始禁止）。
- [必須] **Run 記録**：各 `Job` オブジェクトの内部フィールド `runs: [ ... ]`（`tickets[].jobs[].runs[]`：未実行等の空配列時は `[]` または記載省略）に、Job の1回の実行試行の正本として `Run`（`id`: `run-*`, `started`, 終了時の `ended` / `result`, `waiting` 時の `wait`）を記録する。
  - [禁止] `Run` を `Job` の外に配置する、または親参照を重複記載する。
  - [禁止] 同じ Ticket の別 Job を同時実行する、複数の Run を同時に稼働させる、または同じ試行の再送を二重登録する。
  - [必須] Run では実作業開始時刻 `started` を必須記録する。実行中は `result`（および `ended`）を持たず、終了確定時に `result`（および確認できれば `ended`）を必須記録し、終了時点で当該 `Run.id` を閉じる。
  - [必須] 待機・実行制御情報（`wait`：`target`・`condition`・`since`・`resume_point`・`timeout`・`resolved`）は Job 直下ではなく各 Job 内の `Run.wait` で保持する。
  - [禁止] 依頼送信や `Run` の終了だけで Job を完了する。
  - [禁止] 閉じた旧 `Run.id` や対象外 Session からの業務状態更新を適用する。
- [必須] **管理・評価・証拠取り込み操作**：単一 Owner 原則のもと、未着手 Ticket への依存評価（前提完了→`todo`、前提中止→`blocked`）、保留・解除・中止・Run を持たない Task の計画内 `waiting` 入退/完了・障害解消復帰、および Run 終了後の `waiting` / `doing` Job への検証済み外部証拠取り込みは、それぞれの認可・終端保護・監査記録を満たして実行する。
- [必須] **Job の終了と status / result 分離**：確認可能な証拠に基づき完了または中止を確定する。
  - [必須] `criteria` をすべて満たせば `status` を `completed` とし、業務判定（`approved`・`changes_requested`・`passed`・`failed` 等）を `result` に分離して記録する。
  - [必須] レビュー・調査・承認・操作依頼および依頼先からの追加依頼は通常の Ticket / Job で表現する。
  - [任意] 必要な Job だけ理由を残して `canceled` にし、実行中の Run があれば同時に終了（`result: "canceled"` 等を記録して閉鎖）し、前の実行が停止（または残存副作用がない安全条件）していることを確認した上で後続 Job を続ける。
  - [必須] 差し戻しや失敗に応じた修正・再実行は完了済み Job を巻き戻さず新しい Ticket で対応する。
- [必須] **Ticket の終了**：全 Job が終端（`completed`・`canceled`）なら `completed` に集約する。
  - [必須] Ticket 自身がキャンセル済みなら `canceled` を維持し、`cancel_reason` を必須記録する。
  - [必須] Ticket 終了時に正式な成果・結果・状態を永続化し、次 Ticket が未作成でも終了済み Ticket をアーカイブできるようにする。
  - [禁止] 終端 Ticket（`completed`・`canceled`）を再開する、または Ticket 独自の完了条件を追加する。
- [必須] **Task の完了**：全 Ticket の終了に加え、Task の `criteria` と必須成果を確認する。
  - [禁止] Ticket の完了数や中止 Job を含む処理終了だけで Task の達成率・目標達成を判断する。
- [必須] **明示的な中止**：Task・Ticket の未終端の子孫（Job・Run）も同時に閉鎖・`canceled` にする。
  - [必須] 自身の中止を全 Job 終端による Ticket 完了より優先し、遅延通知による復活を禁止する。

## 停止・再試行・再開不可時の新規 Ticket 引き継ぎ

- [必須] **状態判断**：Task・Ticket・Job における計画内の外部結果待ちは `waiting`、明示保留は `paused`、計画外の継続不能課題は `blocked` とする。
  - [必須] 待機対象の実装種類や Worker 種別で `waiting` の状態遷移を分岐させない。
  - [必須] `waiting` 解除後は現在の未完了 Job のみから一方向に続行する（複合スクリプト内の待機も同じ）。
  - [必須] 保留中（`paused`）は実行再開を禁止し、未解消の待機・課題を保持して明示解除時に再評価する（保留前状態の無条件復元禁止）。
  - [必須] 再開・障害解除・再試行に必要な条件や証拠は必ず永続正本（`depends`・`Run.wait`・`Run.result`・`Ticket.cancel_reason`・`resume`・管理履歴）に保存し、揮発性 Handoff のみに委ねない。
  - [必須] 中断・引き継ぎ前および重要な失敗時は正本と Handoff を更新し、再開時は正本・実状態と照合して、条件が変わっていない失敗を根拠なく繰り返さない。
- [必須] **同一 Ticket・同一 Session 内の Job 再試行**：一時的なエラー等で `Run` が `result: "failed"` となったが同一 Session で回復可能な場合は、Ticket・Job の `status: "doing"` を維持したまま失敗した旧 Run を終了（`result: "failed"` 等を記録して閉鎖）し、前の実行が停止（または残存副作用がない安全条件）していることを確認した上で、同一 Ticket・同一 Session で当該 Job の `runs` 配列に新しい `Run`（`run-*`）を追加・実行する。
- [必須] **Session 再開不可時の新規 Ticket 起票**：担当 Session が再開不可となった場合は、前の実行が停止（または残存副作用がない安全条件）していることを確認してから新規 Ticket へ引き継ぐ。
  - [禁止] 1つの Ticket の担当セッション（`worker_id`・`session_id`）を途中で書き換えて別 Session や別 Worker に付け替える。
  - [禁止] 前の実行が稼働中または停止未確認の状態で旧 Ticket を引き継ぐ。
  - [必須] Worker / Session が無応答・再開不可となった場合は、前の実行が停止（または残存副作用がない安全条件）していることを確認した上で、旧 Run を終了（`result: "canceled"` 等）し、旧 Ticket（および未完了 Job）を `status: "canceled"`（`cancel_reason` 必須）として閉鎖し、同一 Task 配下に新規 Ticket（`tkt-*`、`refs` で旧 Ticket を参照）を起票して新しい Session（`ssn-*`）で稼働させる。
  - [必須] 障害注入等の副作用がある作業では、Ticket 中止と環境復旧を分離し、安全確認完了まで後続の危険な実行を禁止する。
- [必須] **課題対応**：ブロックされた課題の修復、別 Worker への依頼、差し戻し修正はいずれも通常の新 Ticket にし、`refs` で元 Ticket を参照する。
  - [必須] 元 Ticket が継続可能なら、正本に記録した解除条件に基づき実際の課題解消を確認してから再開する（別 Ticket 完了だけで自動解除しない）。

## Handoff と長期知識

- [必須] **揮発性 Handoff**：`handoff` は `Task` オブジェクト内（`project.tasks[].handoff`）の揮発性単一スロットとして扱い（逆参照の `task_id` は持たない）、作業途中の文脈・停止位置・ブロッカーを共有する。Task 完了時には揮発性 Handoff を消去し、恒久的な知見のみを `Project.knowledge` へ昇格させる。
  - [必須] Task 参照権限を持つ複数 Session による同時閲覧を許可し、閲覧権限と実行権限を厳格に分離する。
  - [禁止] Handoff を正式履歴・成果物・再開条件の正本にする、Handoff にコマンド実行機能を持たせる、または Handoff によって無関係な Ticket を直列化する。
  - [必須] Handoff が消失・破棄されても Task・Ticket・Job・中止理由・Artifact・正本 `resume`・Run・管理履歴が失われず、正本から再開判断と再生成ができる構成を保つ。
- [必須] **長期知識**：Project の `knowledge`（`project.knowledge[]`）に再利用する内容（`content`）と根拠（`basis`）を残す。
  - [必須] 状態は `active`・`supersed` の2値（v1.0.0 の綴りを維持）とし、新規記録は未指定でも `active` にする。
  - [必須] 予定も現在の知識として `active` に記録し、通常は `active` だけを参照し、振り返り・再発障害確認では `supersed` も読む。
  - [必須] 使わなくなった知識は理由を残して `supersed` にし、訂正は `supersedes` 付きの新規記録とする。
- [必須] **点検**：7種の自動採番 ID 形式（`<接頭辞>-<英数字6桁または8桁>-<ゼロ埋めなしインクリメント>`：`pj-`・`tsk-`・`tkt-`・`job-`・`run-`・`wkr-`・`ssn-`）、配列・単一オブジェクトの原則に基づく `project.tasks[]`（`project` は単一オブジェクト）とルート `tickets[].jobs[].runs[]`（`Ticket.task_id` 必須）の分離スキーマと省略規則およびファイル分割対応（`Task` 内の `tickets` 保持・逆参照 `Task.project`・`Handoff.task_id` および `Ticket.jobs` の ID 配列分離の廃止）、単一 Owner 原則、Ticket の `worker_id`・`session_id` 固定、`status`/`result` 分離、未解決待機辺を含む依存循環禁止、再開不可・差し戻し時の新規 Ticket 起票、揮発性 Handoff の Task 完了時消去と `Project.knowledge` への昇格、禁止事項、Worker 非依存性を確認する。
  - [必須] 計画・実行済みの事実・管理情報・未確認事項を分けて報告する。
