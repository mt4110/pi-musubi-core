# pi-musubi-core

Pi Network の `Sign in` と `10 Pi deposit` だけに絞った web PoC です。

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
```

そのうえで backend をホスト側で起動します。

```bash
cd apps/backend
cargo run
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

- `DATABASE_URL=postgres://musubi:musubi_local_dev@127.0.0.1:55432/musubi_dev`
- `MUSUBI_TEST_DATABASE_URL=postgres://musubi:musubi_local_dev@127.0.0.1:55432/musubi_test`
- `REDIS_URL=redis://127.0.0.1:56379/0`

## Foundation alignment

This repo is the canonical implementation repo for MUSUBI Day 1.
The constitutional and design source of truth is `mt4110/musubi-foundation`.

Pinned foundation release URL:
https://github.com/mt4110/musubi-foundation/releases/tag/v0.1.0

Current implementation milestone:
M1 - Core Truth and Orchestration Baseline

See `docs/foundation_lock.md` for the pinned design corpus.

Implementation agents must treat `docs/foundation_lock.md` as the mandatory reading gateway before changing code.
