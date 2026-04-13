# pi-musubi-core

Pi Network の `Sign in` と `10 Pi deposit` web PoC から出発した、
MUSUBI Day 1 の canonical implementation repository です。

## 構成
- `apps/mobile`: Flutter Web フロントエンド
- `apps/backend`: Rust / Axum バックエンド

## 導線
1. Pi でサインイン
2. ダミーの相手一覧を見る
3. 詳細画面で `デポジットして本気のアプローチ（10 Pi）`

## 起動
まず backend のローカル基盤を立ち上げます。

```bash
cd apps/backend
cp .env.example .env
docker-compose up -d postgres redis
make db-bootstrap
make db-migrate
make db-status
```

そのうえで backend をホスト側で起動します。

```bash
cd apps/backend
make dev
```

必要なら backend コンテナごと起動することもできます。

```bash
cd apps/backend
docker-compose up --build backend
```

```bash
cd apps/mobile
flutter pub get
flutter run -d chrome --dart-define=API_BASE_URL=http://localhost:8088
```

ローカル接続先の既定値は以下です。

- `APP_ENV=local`
- `DATABASE_URL=postgres://musubi:musubi_local_dev@127.0.0.1:55432/musubi_dev`
- `MUSUBI_TEST_DATABASE_URL=postgres://musubi:musubi_local_dev@127.0.0.1:55432/musubi_test`
- `REDIS_URL=redis://127.0.0.1:56379/0`
- `PROVIDER_MODE=sandbox`
- `PROVIDER_BASE_URL=https://sandbox.minepi.com/v2`

`REQUIRE_LATEST_SCHEMA=true` の場合、backend は pending / failed / checksum drift に加えて、DB 側にだけ存在する applied migration がある状態では起動しません。

## Foundation alignment

This repo is the canonical implementation repo for MUSUBI Day 1.
The constitutional and design source of truth is `mt4110/musubi-foundation`.

Pinned foundation release URL:
https://github.com/mt4110/musubi-foundation/releases/tag/v0.1.0

Current implementation milestone:
M1 - Core Truth and Orchestration Baseline

See `docs/foundation_lock.md` for the pinned design corpus.

Implementation agents must treat `docs/foundation_lock.md` as the mandatory reading gateway before changing code.

## Git運用メモ

M1 issue work は `main` から直接切るのではなく、
integration branch を最新化してから issue branch を切る前提です。

現在の integration branch:
- `feat/happy_route`

推奨手順:

```bash
git fetch origin --prune
git checkout main
git merge --ff-only origin/main
git checkout feat/happy_route
git merge --no-ff origin/main
git checkout -b <issue-branch>
```

required docs が issue branch 側で見つからない場合は、
先に integration branch の鮮度を疑ってください。
missing docs を推測で補うのではなく、branch を揃えるか人間に確認するのが正解です。

## Rust / Cargo 実行場所

Rust workspace の root は repo 直下ではなく `apps/backend` です。
`cargo check` や `cargo test` は `apps/backend` で実行してください。
