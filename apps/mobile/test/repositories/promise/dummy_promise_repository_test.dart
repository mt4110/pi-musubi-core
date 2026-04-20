import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/core/errors/app_exception.dart';
import 'package:musubi_mobile/features/promise/models/promise_models.dart';
import 'package:musubi_mobile/repositories/promise/dummy_promise_repository.dart';

void main() {
  test('dummy promise repository replays same idempotency key and payload',
      () async {
    final repository = DummyPromiseRepository();
    const request = CreatePromiseIntentRequest(
      internalIdempotencyKey: 'promise-action-1',
      realmId: 'realm-1',
      counterpartyAccountId: 'counterparty-1',
      depositAmountMinorUnits: 10000,
      currencyCode: 'PI',
    );

    final created = await repository.createPromiseIntent(request);
    final replayed = await repository.createPromiseIntent(request);

    expect(created.promiseIntentId, replayed.promiseIntentId);
    expect(created.replayedIntent, isFalse);
    expect(replayed.replayedIntent, isTrue);
  });

  test('dummy promise repository rejects idempotency payload drift', () async {
    final repository = DummyPromiseRepository();
    const request = CreatePromiseIntentRequest(
      internalIdempotencyKey: 'promise-action-1',
      realmId: 'realm-1',
      counterpartyAccountId: 'counterparty-1',
      depositAmountMinorUnits: 10000,
      currencyCode: 'PI',
    );
    const drift = CreatePromiseIntentRequest(
      internalIdempotencyKey: 'promise-action-1',
      realmId: 'realm-1',
      counterpartyAccountId: 'counterparty-2',
      depositAmountMinorUnits: 10000,
      currencyCode: 'PI',
    );

    await repository.createPromiseIntent(request);

    expect(
      repository.createPromiseIntent(drift),
      throwsA(isA<BusinessException>()),
    );
  });
}
