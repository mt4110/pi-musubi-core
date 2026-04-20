import '../../core/errors/app_exception.dart';
import '../../features/promise/models/promise_models.dart';
import 'promise_repository.dart';

class DummyPromiseRepository implements PromiseRepository {
  final Map<String, _DummyPromiseRecord> _recordsByKey = {};
  final Map<String, _DummyPromiseRecord> _recordsByIntentId = {};

  @override
  Future<CreatePromiseIntentResponse> createPromiseIntent(
    CreatePromiseIntentRequest request,
  ) async {
    await Future.delayed(const Duration(milliseconds: 450));
    final fingerprint = _fingerprint(request);
    final existing = _recordsByKey[request.internalIdempotencyKey];
    if (existing != null) {
      if (existing.fingerprint != fingerprint) {
        throw const BusinessException(
          message: '同じ操作キーで別の約束は作れません。もう一度画面を開き直してください。',
        );
      }
      return existing.response.copyWith(replayedIntent: true);
    }

    final sequence = _recordsByKey.length + 1;
    final response = CreatePromiseIntentResponse(
      promiseIntentId: 'demo-promise-$sequence',
      settlementCaseId: 'demo-settlement-$sequence',
      caseStatus: 'pending_funding',
      replayedIntent: false,
    );
    final record = _DummyPromiseRecord(
      request: request,
      response: response,
      fingerprint: fingerprint,
    );
    _recordsByKey[request.internalIdempotencyKey] = record;
    _recordsByIntentId[response.promiseIntentId] = record;
    return response;
  }

  @override
  Future<PromiseStatusBundle> fetchPromiseStatus(
    String promiseIntentId, {
    String? settlementCaseId,
  }) async {
    await Future.delayed(const Duration(milliseconds: 300));
    final record = _recordsByIntentId[promiseIntentId];
    if (record == null) {
      return PromiseStatusBundle(
        promiseIntentId: promiseIntentId,
        initialSettlementCaseId: settlementCaseId,
        promise: null,
        settlement: null,
      );
    }

    return PromiseStatusBundle(
      promiseIntentId: promiseIntentId,
      initialSettlementCaseId: settlementCaseId,
      promise: PromiseProjectionView(
        promiseIntentId: record.response.promiseIntentId,
        realmId: record.request.realmId,
        initiatorAccountId: 'demo-current-account',
        counterpartyAccountId: record.request.counterpartyAccountId,
        currentIntentStatus: 'proposed',
        depositAmountMinorUnits: record.request.depositAmountMinorUnits,
        currencyCode: record.request.currencyCode,
        depositScale: 3,
        latestSettlementCaseId: record.response.settlementCaseId,
        latestSettlementStatus: record.response.caseStatus,
      ),
      settlement: ExpandedSettlementView(
        settlementCaseId: record.response.settlementCaseId,
        promiseIntentId: record.response.promiseIntentId,
        realmId: record.request.realmId,
        currentSettlementStatus: record.response.caseStatus,
        totalFundedMinorUnits: 0,
        currencyCode: record.request.currencyCode,
        proofStatus: 'unavailable',
        proofSignalCount: 0,
      ),
    );
  }

  String _fingerprint(CreatePromiseIntentRequest request) {
    return [
      request.realmId,
      request.counterpartyAccountId,
      request.depositAmountMinorUnits,
      request.currencyCode,
    ].join('|');
  }
}

class _DummyPromiseRecord {
  const _DummyPromiseRecord({
    required this.request,
    required this.response,
    required this.fingerprint,
  });

  final CreatePromiseIntentRequest request;
  final CreatePromiseIntentResponse response;
  final String fingerprint;
}

extension on CreatePromiseIntentResponse {
  CreatePromiseIntentResponse copyWith({bool? replayedIntent}) {
    return CreatePromiseIntentResponse(
      promiseIntentId: promiseIntentId,
      settlementCaseId: settlementCaseId,
      caseStatus: caseStatus,
      replayedIntent: replayedIntent ?? this.replayedIntent,
    );
  }
}
