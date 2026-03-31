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
```bash
cd apps/backend
cargo run
```

```bash
cd apps/mobile
flutter pub get
flutter run -d chrome --dart-define=API_BASE_URL=http://localhost:8088
```

## Foundation alignment

This repo is the canonical implementation repo for MUSUBI Day 1.
The constitutional and design source of truth is `mt4110/musubi-foundation`.

Pinned foundation release URL:
https://github.com/mt4110/musubi-foundation/releases/tag/v0.1.0

Current implementation milestone:
M1 - Core Truth and Orchestration Baseline

See `docs/foundation_lock.md` for the pinned design corpus.

Implementation agents must treat `docs/foundation_lock.md` as the mandatory reading gateway before changing code.