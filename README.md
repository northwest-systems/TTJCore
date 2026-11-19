# TTJCore

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/app-icon.png">
  <img src="assets/app-icon-light.png" alt="TTJ" width="96" height="96">
</picture>

## 概要

[TTJ 1.0.0](https://github.com/northwest-systems/TTJ)に従い、Project → Task → Ticket → Job、Run、Projectのknowledgeを管理するRust製CLIです。UIは[Herdr TTJ Mod](https://github.com/northwest-systems/herdr-mods-TTJ)が担当します。バージョンは0.1.0です。

SQLiteを標準とし、SeaORMを介して任意のPostgreSQLへ切り替えられます。変更・履歴・再送結果を同じtransactionで保存します。CoreはTCPポートを開かず、Windowsではnamed pipe、Linux・macOSではUnix domain socketを使用します。

Taskは複数のTicketを持ち、Ticketは1つのTaskに所属します。Ticketと順序付きJobは一括作成し、作成後の計画変更を拒否します。削除は履歴・依存参照を残す論理削除です。

## クイックスタート

ビルド環境にはRust stableのCargoとOSのC compiler・linkerが必要です。CargoはCoreと依存crateをビルドし、次のコマンドで実行ファイルをCargoのbinディレクトリへインストールします。SQLite driverのビルドにもC compilerを使います。

```sh
git clone https://github.com/northwest-systems/TTJCore.git
cd TTJCore
git checkout v0.1.0
cargo install --path . --locked
ttjcore ensure
ttjcore call project.list
```

`ttjcore`をPATHから実行できる状態にしてください。`ensure`は標準DBを初期化し、GUIなしでCoreを常駐起動します。すでに起動中なら同じCoreに接続します。Herdr Modの標準接続はこの処理を自動で行うため、日常操作でinit・serveを実行する必要はありません。

作成IDはCoreが自動発行します。停止は`ttjcore stop`です。Herdrを閉じても、ほかのclientと共有するCoreは停止しません。

## フォルダ構造

```text
assets/        共通アイコン（ライト・ダーク・黒・白の透過PNG）
src/
  main.rs       CLI
  startup.rs    標準DBの準備と常駐Coreの起動
  ipc.rs        IPCとstdio bridge
  rpc.rs        JSON-RPCの検証
  engine.rs     TTJの規則・CRUD・statusの変更
  store.rs      ORM・transaction・変更通知
  entity.rs     SeaORMのtable定義
tests/          SQLite・PostgreSQL・IPCのテスト
skills/ttjcore/
  SKILL.md      LLM向け利用スキル
  references/protocol.md CLI・JSON-RPC・eventの仕様
.github/workflows/ OS別のビルド・テスト
compose.postgres-test.yaml PostgreSQL互換性テスト用DB
Cargo.toml / Cargo.lock   依存の宣言と固定
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

DB設定はensure・init・serveが使います。call・rpc・Herdrは同じprofileのCoreに接続し、DBへ直接接続しません。

## API・LLMからの利用

CLI・JSON-RPC・eventの仕様は[protocol](skills/ttjcore/references/protocol.md)、LLM向けの入口は[skills/ttjcore/SKILL.md](skills/ttjcore/SKILL.md)です。

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
