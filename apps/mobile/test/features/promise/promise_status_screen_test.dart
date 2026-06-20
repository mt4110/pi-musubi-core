import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/features/promise/models/promise_models.dart';
import 'package:musubi_mobile/features/promise/presentation/promise_status_screen.dart';
import 'package:musubi_mobile/repositories/promise/promise_repository.dart';
import 'package:musubi_mobile/repositories/repository_providers.dart';

void main() {
  testWidgets(
      'missing projections without creation context show unavailable state',
      (tester) async {
    await tester.pumpWidget(
      _buildApp(
        repository: const _FakePromiseRepository(
          bundle: PromiseStatusBundle(
            promiseIntentId: 'promise-missing',
            initialSettlementCaseId: null,
            promise: null,
            settlement: null,
          ),
        ),
        promiseIntentId: 'promise-missing',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('約束を表示できませんでした'), findsOneWidget);
    expect(
      find.text('URL が古いか、表示対象が見つからない可能性があります。'),
      findsOneWidget,
    );
    expect(find.text('約束を作成しました'), findsNothing);
  });

  testWidgets(
      'creation context without projections stays pending instead of success',
      (tester) async {
    await tester.pumpWidget(
      _buildApp(
        repository: const _FakePromiseRepository(
          bundle: PromiseStatusBundle(
            promiseIntentId: 'promise-1',
            initialSettlementCaseId: 'settlement-1',
            promise: null,
            settlement: null,
          ),
        ),
        promiseIntentId: 'promise-1',
        settlementCaseId: 'settlement-1',
        creationConfirmed: true,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('約束の表示を確認しています'), findsOneWidget);
    expect(find.text('約束を作成しました'), findsNothing);
    expect(find.text('表示準備中'), findsNWidgets(2));
  });

  testWidgets(
      'settlement projection without promise projection stays pending instead of success',
      (tester) async {
    await tester.pumpWidget(
      _buildApp(
        repository: const _FakePromiseRepository(
          bundle: PromiseStatusBundle(
            promiseIntentId: 'promise-1',
            initialSettlementCaseId: 'settlement-1',
            promise: null,
            settlement: ExpandedSettlementView(
              settlementCaseId: 'settlement-1',
              promiseIntentId: 'promise-1',
              realmId: 'realm-tokyo-day1',
              currentSettlementStatus: 'pending_funding',
              totalFundedMinorUnits: 0,
              currencyCode: 'PI',
              proofStatus: 'unavailable',
              proofSignalCount: 0,
            ),
          ),
        ),
        promiseIntentId: 'promise-1',
        settlementCaseId: 'settlement-1',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('約束の表示を確認しています'), findsOneWidget);
    expect(find.text('約束を作成しました'), findsNothing);
    expect(find.text('約束を表示できませんでした'), findsNothing);
  });

  testWidgets(
      'available completion display stays inside bounded participant copy',
      (tester) async {
    await tester.pumpWidget(
      _buildApp(
        repository: const _FakePromiseRepository(
          bundle: PromiseStatusBundle(
            promiseIntentId: 'promise-1',
            initialSettlementCaseId: 'settlement-1',
            promise: PromiseProjectionView(
              promiseIntentId: 'promise-1',
              realmId: 'realm-tokyo-day1',
              initiatorAccountId: 'account-a',
              counterpartyAccountId: 'account-b',
              currentIntentStatus: 'proposed',
              depositAmountMinorUnits: 10000,
              currencyCode: 'PI',
              depositScale: 3,
              latestSettlementCaseId: 'settlement-1',
              latestSettlementStatus: 'pending_funding',
            ),
            settlement: ExpandedSettlementView(
              settlementCaseId: 'settlement-1',
              promiseIntentId: 'promise-1',
              realmId: 'realm-tokyo-day1',
              currentSettlementStatus: 'pending_funding',
              totalFundedMinorUnits: 0,
              currencyCode: 'PI',
              proofStatus: 'unavailable',
              proofSignalCount: 0,
            ),
            participantSafeDisplayAvailability:
                ParticipantSafeDisplayAvailability(
              displayAvailability: 'available',
              completedReferenceAvailable: true,
            ),
          ),
        ),
        promiseIntentId: 'promise-1',
        settlementCaseId: 'settlement-1',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('完了について'), findsOneWidget);
    expect(
      find.text(
        '完了の確認材料を参加者向けに扱える準備ができています。この画面だけで預かり金、関係の扱い、相手へのアクセスは変わりません。',
      ),
      findsOneWidget,
    );
    for (final forbiddenLabel in <Pattern>[
      'バッジ',
      'スコア',
      'カウント',
      '件数',
      'ランキング',
      '非難',
      '公開',
      '完了済み参照',
      'プロフィール',
      'ステータス',
      '信頼',
      '関係の深さ',
      '決済ラベル',
      '精算ラベル',
      '連絡先',
      'ルーム',
      'おすすめ',
      '発見',
      'プロバイダー',
      'アウトボックス',
      'インボックス',
      'ワーカー',
      '分析',
      '可観測性',
      '外部副作用',
      RegExp('public', caseSensitive: false),
      RegExp('badge', caseSensitive: false),
      RegExp('score', caseSensitive: false),
      RegExp(r'\bpublic count\b', caseSensitive: false),
      RegExp(r'\bcompletion count\b', caseSensitive: false),
      RegExp(r'\bcompleted[- ]reference\b', caseSensitive: false),
      RegExp('status', caseSensitive: false),
      RegExp('trust', caseSensitive: false),
      RegExp('depth', caseSensitive: false),
      RegExp('settlement', caseSensitive: false),
      RegExp('accusation', caseSensitive: false),
      RegExp('public profile', caseSensitive: false),
      RegExp('public proof', caseSensitive: false),
      RegExp('discovery', caseSensitive: false),
      RegExp('recommendation', caseSensitive: false),
      RegExp('contact', caseSensitive: false),
      RegExp('room', caseSensitive: false),
      RegExp('provider', caseSensitive: false),
      RegExp('outbox', caseSensitive: false),
      RegExp('inbox', caseSensitive: false),
      RegExp('worker', caseSensitive: false),
      RegExp('analytics', caseSensitive: false),
      RegExp('observability', caseSensitive: false),
      RegExp(r'external[ -]side[ -]effect', caseSensitive: false),
    ]) {
      expect(find.textContaining(forbiddenLabel), findsNothing);
    }
  });
}

Widget _buildApp({
  required PromiseRepository repository,
  required String promiseIntentId,
  String? settlementCaseId,
  bool creationConfirmed = false,
}) {
  return ProviderScope(
    overrides: [
      promiseRepositoryProvider.overrideWith((ref) => repository),
    ],
    child: MaterialApp(
      home: PromiseStatusScreen(
        promiseIntentId: promiseIntentId,
        settlementCaseId: settlementCaseId,
        creationConfirmed: creationConfirmed,
      ),
    ),
  );
}

class _FakePromiseRepository implements PromiseRepository {
  const _FakePromiseRepository({required this.bundle});

  final PromiseStatusBundle bundle;

  @override
  Future<CreatePromiseIntentResponse> createPromiseIntent(
    CreatePromiseIntentRequest request,
  ) {
    throw UnimplementedError();
  }

  @override
  Future<PromiseStatusBundle> fetchPromiseStatus(
    String promiseIntentId, {
    String? settlementCaseId,
  }) async {
    return bundle;
  }
}
