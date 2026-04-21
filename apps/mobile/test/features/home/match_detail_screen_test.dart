import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/features/home/presentation/match_detail_screen.dart';
import 'package:musubi_mobile/features/promise/models/promise_models.dart';
import 'package:musubi_mobile/features/promise/presentation/promise_status_screen.dart';
import 'package:musubi_mobile/repositories/auth/auth_repository.dart';
import 'package:musubi_mobile/repositories/auth/auth_session_controller.dart';
import 'package:musubi_mobile/repositories/auth/pi_auth_session.dart';
import 'package:musubi_mobile/repositories/promise/promise_repository.dart';
import 'package:musubi_mobile/repositories/repository_providers.dart';

void main() {
  testWidgets('creating a promise keeps a back path to the detail screen',
      (tester) async {
    final router = GoRouter(
      initialLocation: '/detail/mai',
      routes: [
        GoRoute(
          path: '/detail/:profileId',
          builder: (context, state) => MatchDetailScreen(
            profileId: state.pathParameters['profileId'] ?? '',
          ),
        ),
        GoRoute(
          path: '/promises/:promiseIntentId',
          builder: (context, state) {
            final navigationState = state.extra;
            return PromiseStatusScreen(
              promiseIntentId: state.pathParameters['promiseIntentId'] ?? '',
              settlementCaseId: state.uri.queryParameters['settlementCaseId'],
              creationConfirmed: navigationState is Map<Object?, Object?> &&
                  navigationState['created'] == true,
              replayedIntent: navigationState is Map<Object?, Object?> &&
                  navigationState['replayed'] == true,
            );
          },
        ),
      ],
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          authRepositoryProvider.overrideWith((ref) => _FakeAuthRepository()),
          promiseRepositoryProvider.overrideWith(
            (ref) => _FakePromiseRepository(),
          ),
        ],
        child: _WarmAuthSession(
          child: MaterialApp.router(routerConfig: router),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Promise を作成して進む（10 Pi）'));
    await tester.pumpAndSettle();

    expect(find.text('約束を作成しました'), findsOneWidget);
    expect(router.canPop(), isTrue);
  });

  testWidgets('created query parameters alone do not keep stale links pending',
      (tester) async {
    final router = GoRouter(
      initialLocation:
          '/promises/promise-missing?created=true&replayed=true&settlementCaseId=settlement-1',
      routes: [
        GoRoute(
          path: '/promises/:promiseIntentId',
          builder: (context, state) {
            final navigationState = state.extra;
            return PromiseStatusScreen(
              promiseIntentId: state.pathParameters['promiseIntentId'] ?? '',
              settlementCaseId: state.uri.queryParameters['settlementCaseId'],
              creationConfirmed: navigationState is Map<Object?, Object?> &&
                  navigationState['created'] == true,
              replayedIntent: navigationState is Map<Object?, Object?> &&
                  navigationState['replayed'] == true,
            );
          },
        ),
      ],
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          promiseRepositoryProvider.overrideWith(
            (ref) => const _MissingProjectionPromiseRepository(),
          ),
        ],
        child: MaterialApp.router(routerConfig: router),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('約束を表示できませんでした'), findsOneWidget);
    expect(find.text('約束の表示を確認しています'), findsNothing);
  });
}

class _WarmAuthSession extends ConsumerWidget {
  const _WarmAuthSession({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(authSessionControllerProvider);
    return child;
  }
}

class _FakeAuthRepository implements AuthRepository {
  static const _session = PiAuthSession(
    userId: 'test-user',
    piUid: 'test-pi-user',
    displayName: '@test_user',
  );

  @override
  Future<PiAuthSession?> getCurrentSession() async => _session;

  @override
  Future<PiAuthSession> signInWithPi() async => _session;

  @override
  Future<void> signOut() async {}

  @override
  Future<PiAuthSession?> trySilentSignIn() async => _session;
}

class _FakePromiseRepository implements PromiseRepository {
  @override
  Future<CreatePromiseIntentResponse> createPromiseIntent(
    CreatePromiseIntentRequest request,
  ) async {
    return const CreatePromiseIntentResponse(
      promiseIntentId: 'promise-1',
      settlementCaseId: 'settlement-1',
      caseStatus: 'pending_funding',
      replayedIntent: false,
    );
  }

  @override
  Future<PromiseStatusBundle> fetchPromiseStatus(
    String promiseIntentId, {
    String? settlementCaseId,
  }) async {
    return PromiseStatusBundle(
      promiseIntentId: promiseIntentId,
      initialSettlementCaseId: settlementCaseId,
      promise: const PromiseProjectionView(
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
      settlement: const ExpandedSettlementView(
        settlementCaseId: 'settlement-1',
        promiseIntentId: 'promise-1',
        realmId: 'realm-tokyo-day1',
        currentSettlementStatus: 'pending_funding',
        totalFundedMinorUnits: 0,
        currencyCode: 'PI',
        proofStatus: 'unavailable',
        proofSignalCount: 0,
      ),
    );
  }
}

class _MissingProjectionPromiseRepository implements PromiseRepository {
  const _MissingProjectionPromiseRepository();

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
    return PromiseStatusBundle(
      promiseIntentId: promiseIntentId,
      initialSettlementCaseId: settlementCaseId,
      promise: null,
      settlement: null,
    );
  }
}
