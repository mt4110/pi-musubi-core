import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/app/widgets/musubi_pressable.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'package:musubi_mobile/features/realm_bootstrap/presentation/realm_bootstrap_screen.dart';
import 'package:musubi_mobile/repositories/auth/auth_repository.dart';
import 'package:musubi_mobile/repositories/auth/auth_session_controller.dart';
import 'package:musubi_mobile/repositories/auth/pi_auth_session.dart';
import 'package:musubi_mobile/repositories/realm_bootstrap/realm_bootstrap_repository.dart';
import 'package:musubi_mobile/repositories/repository_providers.dart';

void main() {
  testWidgets('realm request UI submits required participant fields', (
    tester,
  ) async {
    await _useTallSurface(tester);
    final repository = _FakeRealmBootstrapRepository();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          authRepositoryProvider.overrideWith((ref) => _FakeAuthRepository()),
          realmBootstrapRepositoryProvider.overrideWith((ref) => repository),
        ],
        child: const _WarmAuthSession(
          child: MaterialApp(home: RealmBootstrapScreen()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField).at(0), 'Tokyo slow coffee');
    await tester.enterText(find.byType(TextField).at(1), 'tokyo-slow-coffee');
    await tester.enterText(
      find.byType(TextField).at(2),
      'Calm local meetings.',
    );
    await tester.enterText(find.byType(TextField).at(3), 'Tokyo cafe');
    await tester.enterText(find.byType(TextField).at(4), 'Small and local');
    await tester.enterText(
      find.byType(TextField).at(5),
      'Start with bounded growth.',
    );
    final submitButton = find.byType(MusubiPrimaryButton);
    expect(
      tester.widget<MusubiPrimaryButton>(submitButton).onPressed,
      isNotNull,
    );
    await tester.tap(submitButton);
    await tester.pumpAndSettle();

    expect(repository.createdDraft?.displayName, 'Tokyo slow coffee');
    expect(find.text('申請を受け付けました'), findsOneWidget);
    expect(find.text('申請済み'), findsOneWidget);
    expect(find.textContaining('operator id'), findsNothing);
    expect(find.textContaining('source fact'), findsNothing);

    final firstRequestKey = repository.createdDraft!.requestIdempotencyKey;
    expect(
      tester.widget<MusubiPrimaryButton>(submitButton).onPressed,
      isNotNull,
    );
    await tester.tap(submitButton);
    await tester.pumpAndSettle();

    expect(repository.createdDrafts, hasLength(2));
    expect(
      repository.createdDrafts.last.requestIdempotencyKey,
      isNot(firstRequestKey),
    );
  });

  testWidgets('realm request UI regenerates key after edited failed intent', (
    tester,
  ) async {
    await _useTallSurface(tester);
    final repository = _FakeRealmBootstrapRepository()..failNextCreate = true;
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          authRepositoryProvider.overrideWith((ref) => _FakeAuthRepository()),
          realmBootstrapRepositoryProvider.overrideWith((ref) => repository),
        ],
        child: const _WarmAuthSession(
          child: MaterialApp(home: RealmBootstrapScreen()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField).at(0), 'Tokyo slow coffee');
    await tester.enterText(find.byType(TextField).at(1), 'tokyo-slow-coffee');
    await tester.enterText(
      find.byType(TextField).at(2),
      'Calm local meetings.',
    );
    await tester.enterText(find.byType(TextField).at(3), 'Tokyo cafe');
    await tester.enterText(find.byType(TextField).at(4), 'Small and local');
    await tester.enterText(
      find.byType(TextField).at(5),
      'Start with bounded growth.',
    );

    final submitButton = find.byType(MusubiPrimaryButton);
    await tester.tap(submitButton);
    await tester.pumpAndSettle();
    final failedRequestKey = repository.createdDraft!.requestIdempotencyKey;

    await tester.enterText(
      find.byType(TextField).at(2),
      'Calm local meetings with a smaller first circle.',
    );
    await tester.tap(submitButton);
    await tester.pumpAndSettle();

    expect(repository.createdDrafts, hasLength(2));
    expect(
      repository.createdDrafts.last.requestIdempotencyKey,
      isNot(failedRequestKey),
    );
  });

  testWidgets('realm summary UI renders redacted bootstrap and operator panels',
      (
    tester,
  ) async {
    await _useTallSurface(tester);
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          authRepositoryProvider.overrideWith((ref) => _FakeAuthRepository()),
          realmBootstrapRepositoryProvider.overrideWith(
            (ref) => _FakeRealmBootstrapRepository(),
          ),
        ],
        child: const _WarmAuthSession(
          child: MaterialApp(home: RealmBootstrapScreen()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final summaryButton = find.widgetWithText(MusubiGhostButton, '状態を確認');
    expect(
      tester.widget<MusubiGhostButton>(summaryButton).onPressed,
      isNotNull,
    );
    await tester.tap(summaryButton);
    await tester.pumpAndSettle();
    await tester.drag(find.byType(ListView), const Offset(0, -700));
    await tester.pumpAndSettle();

    expect(find.text('Tokyo slow coffee'), findsOneWidget);
    expect(find.text('限定受付'), findsOneWidget);
    expect(find.text('Admission request'), findsOneWidget);
    expect(find.text('確認キュー'), findsOneWidget);
    expect(find.text('Operator / Steward review'), findsOneWidget);
    expect(find.textContaining('証跡の所在'), findsOneWidget);
    expect(find.textContaining('private://'), findsNothing);
    expect(find.textContaining('ランキング'), findsNothing);
    expect(find.textContaining('DM unlock'), findsNothing);
  });
}

Future<void> _useTallSurface(WidgetTester tester) async {
  await tester.binding.setSurfaceSize(const Size(1000, 1400));
  addTearDown(() => tester.binding.setSurfaceSize(null));
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

class _FakeRealmBootstrapRepository implements RealmBootstrapRepository {
  CreateRealmRequestDraft? createdDraft;
  final createdDrafts = <CreateRealmRequestDraft>[];
  bool failNextCreate = false;

  @override
  Future<RealmRequestView> createRealmRequest(
    CreateRealmRequestDraft draft,
  ) async {
    createdDraft = draft;
    createdDrafts.add(draft);
    if (failNextCreate) {
      failNextCreate = false;
      throw Exception('realm request failed');
    }
    return RealmRequestView(
      realmRequestId: 'request-1',
      displayName: draft.displayName,
      slugCandidate: draft.slugCandidate,
      purposeText: draft.purposeText,
      venueContextSummary: draft.venueContextText,
      expectedMemberShapeSummary: draft.expectedMemberShapeText,
      bootstrapRationaleText: draft.bootstrapRationaleText,
      requestState: 'requested',
      reviewReasonCode: 'pending_operator_review',
      createdRealmId: null,
      proposedSponsorAccountId: null,
      proposedStewardAccountId: null,
    );
  }

  @override
  Future<RealmRequestView> fetchRealmRequest(String realmRequestId) {
    throw UnimplementedError();
  }

  @override
  Future<RealmBootstrapSummaryBundle> fetchBootstrapSummary(
    String realmId,
  ) async {
    return const RealmBootstrapSummaryBundle(
      realmRequest: null,
      bootstrapView: RealmBootstrapView(
        realmId: 'realm-tokyo-day1',
        slug: 'tokyo-slow-coffee',
        displayName: 'Tokyo slow coffee',
        realmStatus: 'limited_bootstrap',
        admissionPosture: 'limited',
        corridorStatus: 'active',
        publicReasonCode: 'limited_bootstrap_active',
        sponsorDisplayState: 'sponsor_and_steward',
      ),
      admissionView: RealmAdmissionView(
        realmId: 'realm-tokyo-day1',
        accountId: 'account-1',
        admissionStatus: 'pending',
        admissionKind: 'review_required',
        publicReasonCode: 'review_required',
      ),
    );
  }
}
