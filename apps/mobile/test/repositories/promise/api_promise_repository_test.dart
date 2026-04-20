import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/api/api_client.dart';
import 'package:musubi_mobile/core/errors/app_exception.dart';
import 'package:musubi_mobile/features/promise/models/promise_models.dart';
import 'package:musubi_mobile/repositories/promise/api_promise_repository.dart';

void main() {
  test(
      'api promise repository returns bounded copy when counterparty account is missing',
      () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      expect(options.path, '/api/promise/intents');
      return _jsonResponse(
        400,
        {'error': 'counterparty account was not found'},
      );
    });
    final repository = ApiPromiseRepository(ApiClient(dio));

    expect(
      repository.createPromiseIntent(
        const CreatePromiseIntentRequest(
          internalIdempotencyKey: 'promise-action-1',
          realmId: 'realm-1',
          counterpartyAccountId: 'counterparty-1',
          depositAmountMinorUnits: 10000,
          currencyCode: 'PI',
        ),
      ),
      throwsA(
        isA<BusinessException>().having(
          (error) => error.message,
          'message',
          '相手または約束の準備がまだ整っていません。時間を置いてもう一度確認してください。',
        ),
      ),
    );
  });

  test(
      'api promise repository starts settlement lookup before promise projection finishes',
      () async {
    final promiseRequested = Completer<void>();
    final settlementRequested = Completer<void>();
    final promiseResponse = Completer<ResponseBody>();
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      switch (options.path) {
        case '/api/projection/promise-views/promise-1':
          promiseRequested.complete();
          return promiseResponse.future;
        case '/api/projection/settlement-views/settlement-1/expanded':
          settlementRequested.complete();
          return _jsonResponse(200, {
            'settlement_case_id': 'settlement-1',
            'promise_intent_id': 'promise-1',
            'realm_id': 'realm-1',
            'current_settlement_status': 'pending_funding',
            'total_funded_minor_units': 0,
            'currency_code': 'PI',
            'proof_status': 'unavailable',
            'proof_signal_count': 0,
          });
      }
      throw StateError('unexpected path: ${options.path}');
    });
    final repository = ApiPromiseRepository(ApiClient(dio));

    final future = repository.fetchPromiseStatus(
      'promise-1',
      settlementCaseId: 'settlement-1',
    );

    await promiseRequested.future;
    await Future<void>.delayed(Duration.zero);
    expect(settlementRequested.isCompleted, isTrue);

    promiseResponse.complete(
      _jsonResponse(200, {
        'promise_intent_id': 'promise-1',
        'realm_id': 'realm-1',
        'initiator_account_id': 'account-a',
        'counterparty_account_id': 'account-b',
        'current_intent_status': 'proposed',
        'deposit_amount_minor_units': 10000,
        'currency_code': 'PI',
        'deposit_scale': 3,
        'latest_settlement_case_id': 'settlement-1',
        'latest_settlement_status': 'pending_funding',
      }),
    );

    final bundle = await future;
    expect(bundle.promise?.promiseIntentId, 'promise-1');
    expect(bundle.settlement?.settlementCaseId, 'settlement-1');
  });
}

class _StubHttpClientAdapter implements HttpClientAdapter {
  _StubHttpClientAdapter(this._handler);

  final Future<ResponseBody> Function(RequestOptions options) _handler;

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) {
    return _handler(options);
  }

  @override
  void close({bool force = false}) {}
}

ResponseBody _jsonResponse(int statusCode, Map<String, Object?> body) {
  return ResponseBody.fromString(
    jsonEncode(body),
    statusCode,
    headers: {
      Headers.contentTypeHeader: [Headers.jsonContentType],
    },
  );
}
