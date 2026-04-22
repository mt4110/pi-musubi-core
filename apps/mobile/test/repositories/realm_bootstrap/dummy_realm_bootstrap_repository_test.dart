import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/core/errors/app_exception.dart';
import 'package:musubi_mobile/features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'package:musubi_mobile/repositories/realm_bootstrap/dummy_realm_bootstrap_repository.dart';

void main() {
  test(
    'dummy realm bootstrap repository replays same idempotency key and payload',
    () async {
      final repository = DummyRealmBootstrapRepository();
      const draft = CreateRealmRequestDraft(
        displayName: 'Tokyo slow coffee',
        slugCandidate: 'tokyo-slow-coffee',
        purposeText: 'Calm local meetings.',
        venueContextText: 'Tokyo cafe',
        expectedMemberShapeText: 'Small and local',
        bootstrapRationaleText: 'Start with bounded growth.',
        requestIdempotencyKey: 'realm-request-action-1',
      );

      final created = await repository.createRealmRequest(draft);
      final replayed = await repository.createRealmRequest(draft);

      expect(created.realmRequestId, replayed.realmRequestId);
      expect(created.displayName, replayed.displayName);
    },
  );

  test(
    'dummy realm bootstrap repository rejects idempotency payload drift',
    () async {
      final repository = DummyRealmBootstrapRepository();
      const draft = CreateRealmRequestDraft(
        displayName: 'Tokyo slow coffee',
        slugCandidate: 'tokyo-slow-coffee',
        purposeText: 'Calm local meetings.',
        venueContextText: 'Tokyo cafe',
        expectedMemberShapeText: 'Small and local',
        bootstrapRationaleText: 'Start with bounded growth.',
        requestIdempotencyKey: 'realm-request-action-1',
      );
      const drift = CreateRealmRequestDraft(
        displayName: 'Tokyo loud coffee',
        slugCandidate: 'tokyo-slow-coffee',
        purposeText: 'Calm local meetings.',
        venueContextText: 'Tokyo cafe',
        expectedMemberShapeText: 'Small and local',
        bootstrapRationaleText: 'Start with bounded growth.',
        requestIdempotencyKey: 'realm-request-action-1',
      );

      await repository.createRealmRequest(draft);

      await expectLater(
        repository.createRealmRequest(drift),
        throwsA(isA<BusinessException>()),
      );
    },
  );

  test(
    'dummy realm bootstrap repository rejects blank idempotency key',
    () async {
      final repository = DummyRealmBootstrapRepository();

      await expectLater(
        repository.createRealmRequest(
          const CreateRealmRequestDraft(
            displayName: 'Tokyo slow coffee',
            slugCandidate: 'tokyo-slow-coffee',
            purposeText: 'Calm local meetings.',
            venueContextText: 'Tokyo cafe',
            expectedMemberShapeText: 'Small and local',
            bootstrapRationaleText: 'Start with bounded growth.',
            requestIdempotencyKey: '   ',
          ),
        ),
        throwsA(isA<BusinessException>()),
      );
    },
  );
}
