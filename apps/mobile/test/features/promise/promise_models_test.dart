import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/features/promise/models/promise_models.dart';

void main() {
  test('promise status labels stay calm and bounded', () {
    expect(promiseStatusLabel('proposed'), '提案中');
    expect(settlementStatusLabel('pending_funding'), 'デポジット準備中');
    expect(proofStatusLabel('unavailable'), '証明機能は準備中');
    expect(proofStatusLabel('quarantined'), '確認中');
  });

  test('participant next action does not claim local completion truth', () {
    const bundle = PromiseStatusBundle(
      promiseIntentId: 'promise-1',
      initialSettlementCaseId: 'settlement-1',
      promise: PromiseProjectionView(
        promiseIntentId: 'promise-1',
        realmId: 'realm-1',
        initiatorAccountId: 'initiator-1',
        counterpartyAccountId: 'counterparty-1',
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
        realmId: 'realm-1',
        currentSettlementStatus: 'pending_funding',
        totalFundedMinorUnits: 0,
        currencyCode: 'PI',
        proofStatus: 'unavailable',
        proofSignalCount: 0,
      ),
    );

    final copy = participantNextActionCopy(bundle);
    expect(copy, contains('完了はまだこの画面だけでは確定しません'));
    expect(copy, isNot(contains('DM')));
    expect(copy, isNot(contains('ランキング')));
  });

  test('missing projection copy stays neutral while the screen waits', () {
    const bundle = PromiseStatusBundle(
      promiseIntentId: 'promise-1',
      initialSettlementCaseId: 'settlement-1',
      promise: null,
      settlement: null,
    );

    final copy = participantNextActionCopy(bundle);
    expect(copy, '表示の準備を確認しています。反映まで少し時間がかかることがあります。');
    expect(copy, isNot(contains('作成は受け付けました')));
  });

  test('projection parsing ignores internal fields not modeled by the UI', () {
    final view = PromiseProjectionView.fromJson({
      'promise_intent_id': 'promise-1',
      'realm_id': 'realm-1',
      'initiator_account_id': 'initiator-1',
      'counterparty_account_id': 'counterparty-1',
      'current_intent_status': 'proposed',
      'deposit_amount_minor_units': 10000,
      'currency_code': 'PI',
      'deposit_scale': 3,
      'latest_settlement_case_id': 'settlement-1',
      'latest_settlement_status': 'pending_funding',
      'operator_note_internal': 'must not be rendered',
      'raw_evidence_locator': 'private://evidence',
      'source_fact_id': 'source-fact-1',
    });

    expect(view.promiseIntentId, 'promise-1');
    expect(view.latestSettlementStatus, 'pending_funding');
  });
}
