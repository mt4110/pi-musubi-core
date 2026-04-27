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
