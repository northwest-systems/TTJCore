# TTJCore

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/app-icon.png">
  <img src="assets/app-icon-light.png" alt="TTJ" width="96" height="96">
</picture>

## 概要

[TTJ 1.1.0](https://github.com/northwest-systems/TTJ)に従い、Project・Task・Ticket・Job・Run・Worker・SessionとProjectのKnowledgeを管理するRust製CLIです。UIは自由に実装でき、CLI・JSON-RPCでCoreに接続します。バージョンは0.2.0です。

SQLiteを標準とし、SeaORMを介して任意のPostgreSQLへ切り替えられます。変更・履歴・再送結果を同じtransactionで保存します。CoreはTCPポートを開かず、Windowsではnamed pipe、Linux・macOSではUnix domain socketを使用します。

Taskは単一のproject内に置き、Ticketはルートtickets配列からtask_idでTaskを参照します。WorkerとSessionでTicketの固定担当を識別します。Ticketと順序付きJobは一括作成し、作成後の計画変更を拒否します。終了したTaskは`task.purge`で配下を一括物理削除し、要約履歴とProjectのKnowledgeを残せます。既存の`delete`操作は履歴・依存参照を残す論理削除です。

## クイックスタート

ビルド環境にはRust stableのCargoとOSのC compiler・linkerが必要です。CargoはCoreと依存crateをビルドし、次のコマンドで実行ファイルをCargoのbinディレクトリへインストールします。SQLite driverのビルドにもC compilerを使います。

```sh
git clone https://github.com/northwest-systems/TTJCore.git
cd TTJCore
cargo install --path . --locked
ttjcore ensure
ttjcore call project.list
```

`ttjcore`をPATHから実行できる状態にしてください。`ensure`は標準DBを初期化し、GUIなしでCoreを常駐起動します。すでに起動中なら同じCoreに接続します。

作成IDはCoreが自動発行します。停止は`ttjcore stop`です。クライアントを閉じてもCoreは動作を続けます。

## コマンド操作の例

サンプルProjectの作成・更新から作業完了までの操作例です。

- [PowerShell 7](docs/usage-powershell.md)
- [Bash](docs/usage-bash.md)（JSON処理にjqを使用）
- [進捗API・監視CLI・Orca通知](docs/progress-notifications.md)

## フォルダ構造

Linuxの`tree`形式で、配布対象のフォルダと末端ファイルを示します。`.git`とローカルの生成物は含みません。

```text
TTJCore/                        # Rust製CLI・Core
├── .github/                    # GitHubの設定
│   └── workflows/              # GitHub Actions
│       └── ci.yml              # ビルド・テストのCI
├── assets/                     # 共通アイコン
│   ├── app-icon-black.png      # 単色・黒の透過PNG
│   ├── app-icon-light.png      # ライト用の透過PNG
│   ├── app-icon-white.png      # 単色・白の透過PNG
│   └── app-icon.png            # ダーク用の透過PNG
├── docs/                       # 操作例
│   ├── usage-powershell.md     # PowerShell 7の操作例
│   ├── usage-bash.md           # Bashの操作例
│   └── progress-notifications.md # 進捗API・監視CLI・Orca通知
├── skills/                     # LLM向け利用スキル
│   └── ttjcore/                # Coreの利用スキル
│       ├── references/         # 仕様の参照資料
│       │   ├── ttj-1.1.0/      # オフラインのTTJ規格・skill
│       │   │   ├── SKILL.md
│       │   │   ├── LICENSE
│       │   │   └── references/ # specification・json-format・use-cases
│       │   └── protocol.md     # CLI・JSON-RPC・eventの仕様
│       └── SKILL.md            # LLM向けの操作手順
├── src/                        # Rustソース
│   ├── document.rs             # TTJ 1.1.0の型・検証・ファイル分割
│   ├── engine.rs               # TTJの規則・CRUD・status変更
│   ├── entity.rs               # SeaORMのtable定義
│   ├── ipc.rs                  # ローカルIPC・stdio bridge
│   ├── lib.rs                  # Coreのmodule公開
│   ├── main.rs                 # CLIの入口
│   ├── progress.rs             # 進捗の監視・OSC 7501出力
│   ├── rpc.rs                  # JSON-RPCの検証
│   ├── startup.rs              # 標準DBの準備・常駐Coreの起動
│   └── store.rs                # ORM・transaction・変更通知
├── tests/                      # 自動テスト
│   ├── contract.rs             # SQLite・PostgreSQLの動作契約
│   ├── document.rs             # 1.1.0の入出力・連番・Session・待機循環
│   ├── ipc.rs                  # IPC経由の呼び出し
│   ├── purge.rs                # Task物理削除・要約履歴・容量回収
│   └── startup.rs              # Coreの自動起動・停止
├── .gitignore                  # Gitの除外設定
├── Cargo.lock                  # 依存crateのバージョン固定
├── Cargo.toml                  # package情報・依存crate
├── compose.postgres-test.yaml  # PostgreSQL互換性テスト用DB
├── LICENSE                     # Apache-2.0ライセンス
└── README.md                   # 概要・クイックスタート・フォルダ構造
```

Cargoが取得する直接依存は次のとおりです。推移依存のバージョンはCargo.lockに固定しています。

| crate | 用途 |
|---|---|
| sea-orm | SQLite・PostgreSQL共通のDB操作 |
| tokio | DB・IPC・変更通知の非同期実行 |
| interprocess | named pipe・Unix domain socket |
| serde・serde_json | JSON入出力 |
| time | timezone付き日時の検証・生成 |
| uuid | record ID・operation ID・接続用tokenの生成 |
| base64 | OSC 7501の日本語ラベル・メッセージの符号化 |

## DBとprofile

標準DBはCore profile内の`core.sqlite`です。profileはWindowsでは`LOCALAPPDATA/.ttjcore`、Linux・macOSでは`$HOME/.ttjcore`です。`TTJCORE_HOME`、または各CLIコマンドの前の`--home <data-dir>`で変更できます。Unixでは他ユーザーが書き込めない、socketのpath長制約に収まる短いディレクトリを使います。profileには接続用tokenも保存されるため、Gitへ保存したり共有したりしないでください。

```sh
ttjcore --home /path/to/profile ensure
ttjcore --home /path/to/profile call project.list
ttjcore --home /path/to/profile stop
```

任意のSQLite・PostgreSQLを使う場合は、Coreを起動する環境で`TTJCORE_DATABASE_URL`を設定します。SQLiteファイルの親ディレクトリ、PostgreSQLのDB・接続ユーザーは事前に用意してください。

PowerShell 7:

```powershell
$env:TTJCORE_DATABASE_URL = 'postgresql://<role>:<password>@<host>:<port>/<database>'
ttjcore ensure
```

Bash:

```sh
export TTJCORE_DATABASE_URL='postgresql://<role>:<password>@<host>:<port>/<database>'
ttjcore ensure
```

passwordはURL encodeし、認証・TLS設定に応じた接続optionを指定します。接続情報をGitへ保存しないでください。すでに起動中のCoreは設定変更で切り替わりません。同じprofileのCoreをstopしてからensureしてください。initはschema作成、serveはforeground起動として引き続き使用でき、serveはCtrl+Cでも停止できます。

管理操作には、起動時に`TTJCORE_MANAGEMENT_ACTORS`へ許可主体のJSON配列（例：`["operator"]`）を設定し、requestに`mode:"management"`と対応actorを明示します。既定では管理主体を許可しません。実行由来の更新は固定Worker/Sessionと現行Runに結び付けます。詳細は[protocol](skills/ttjcore/references/protocol.md)を参照してください。

DB設定はensure・init・serveが使います。call・rpcと各UIは同じprofileのCoreに接続し、DBへ直接接続しません。

## API・LLMからの利用

CLI・JSON-RPC・eventの仕様は[protocol](skills/ttjcore/references/protocol.md)、LLM向けの入口は[skills/ttjcore/SKILL.md](skills/ttjcore/SKILL.md)です。

Skillには[TTJ 1.1.0規格本文](skills/ttjcore/references/ttj-1.1.0/references/specification.md)・[JSON参照形式](skills/ttjcore/references/ttj-1.1.0/references/json-format.md)とライセンスを固定版として同梱しています。`skills/ttjcore`を参照資料ごとコピーすれば、ネット接続なしでTTJの規則とCoreの操作を確認できます。

## テスト

通常はSQLiteだけで実行できます。

```sh
cargo test --locked
```

PostgreSQLは任意の接続先への互換性テスト用です。付属のCompose設定はテスト専用で、memory上の一時DBを起動します。PostgreSQL 18の公式imageをDockerが取得します。DBポートは既定でhostのloopback 15432にだけ公開します。

PowerShell 7:

```powershell
$env:TTJCORE_POSTGRES_PASSWORD = [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(24)).ToLowerInvariant()
docker compose -f compose.postgres-test.yaml -p ttjcore-test up -d --wait
$env:TTJCORE_TEST_POSTGRES_URL = "postgresql://ttjcore_test:$($env:TTJCORE_POSTGRES_PASSWORD)@127.0.0.1:15432/ttjcore_test"
cargo test --locked postgres_contract -- --ignored
docker compose -f compose.postgres-test.yaml -p ttjcore-test down
```

別のPostgreSQLでテストする場合も`TTJCORE_TEST_POSTGRES_URL`で指定できます。テストはrecordを作成・更新・論理削除するため、専用のテストDBを使ってください。`TTJCORE_DATABASE_URL`とは別の設定なので、通常利用のSQLiteは切り替わりません。

## ライセンス

[Apache-2.0](LICENSE)。
