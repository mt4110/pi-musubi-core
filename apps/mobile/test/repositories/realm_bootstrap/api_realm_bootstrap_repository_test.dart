import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/api/api_client.dart';
import 'package:musubi_mobile/core/errors/app_exception.dart';
import 'package:musubi_mobile/features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'package:musubi_mobile/repositories/realm_bootstrap/api_realm_bootstrap_repository.dart';

void main() {
  test('api realm repository posts participant request payload', () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      expect(options.path, '/api/realms/requests');
      final body = options.data as Map;
      expect(body['display_name'], 'Tokyo slow coffee');
      expect(body['venue_context_json'], {'summary': 'Tokyo cafe'});
      expect(body['expected_member_shape_json'], {'summary': 'small'});
      expect(body['proposed_sponsor_account_id'], isNull);
      expect(body['request_idempotency_key'], 'realm-request-1');
      return _jsonResponse(200, {
        'realm_request_id': 'request-1',
        'display_name': 'Tokyo slow coffee',
        'slug_candidate': 'tokyo-slow-coffee',
        'purpose_text': 'Calm local meetings.',
        'venue_context_json': {'summary': 'Tokyo cafe'},
        'expected_member_shape_json': {'summary': 'small'},
        'bootstrap_rationale_text': 'Start bounded.',
        'request_state': 'requested',
        'review_reason_code': 'request_received',
      });
    });
    final repository = ApiRealmBootstrapRepository(ApiClient(dio));

    final request = await repository.createRealmRequest(
      const CreateRealmRequestDraft(
        displayName: 'Tokyo slow coffee',
        slugCandidate: 'tokyo-slow-coffee',
        purposeText: 'Calm local meetings.',
        venueContextText: 'Tokyo cafe',
        expectedMemberShapeText: 'small',
        bootstrapRationaleText: 'Start bounded.',
        requestIdempotencyKey: ' realm-request-1 ',
      ),
    );

    expect(request.realmRequestId, 'request-1');
    expect(request.requestState, 'requested');
  });

  test('api realm repository fetches participant-safe bootstrap summary',
      () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      expect(
        options.path,
        '/api/projection/realms/realm-1/bootstrap-summary',
      );
      return _jsonResponse(200, {
        'realm_request': null,
        'bootstrap_view': {
          'realm_id': 'realm-1',
          'slug': 'realm-one',
          'display_name': 'Realm one',
          'realm_status': 'limited_bootstrap',
          'admission_posture': 'limited',
          'corridor_status': 'active',
          'public_reason_code': 'limited_bootstrap_active',
          'sponsor_display_state': 'sponsor_backed',
        },
        'admission_view': null,
      });
    });
    final repository = ApiRealmBootstrapRepository(ApiClient(dio));

    final summary = await repository.fetchBootstrapSummary('realm-1');

    expect(summary.bootstrapView.realmStatus, 'limited_bootstrap');
    expect(summary.admissionView, isNull);
  });

  test('api realm repository maps malformed request response to business error',
      () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      return _jsonResponse(400, {
        'error': 'request_idempotency_key replay payload mismatch',
      });
    });
    final repository = ApiRealmBootstrapRepository(ApiClient(dio));

    expect(
      () => repository.createRealmRequest(
        const CreateRealmRequestDraft(
          displayName: 'Tokyo slow coffee',
          slugCandidate: 'tokyo-slow-coffee',
          purposeText: 'Calm local meetings.',
          venueContextText: 'Tokyo cafe',
          expectedMemberShapeText: 'small',
          bootstrapRationaleText: 'Start bounded.',
          requestIdempotencyKey: 'realm-request-1',
        ),
      ),
      throwsA(
        isA<BusinessException>().having(
          (error) => error.message,
          'message',
          'Realm申請の内容を確認してください。',
        ),
      ),
    );
  });

  test('api realm repository maps safe backend errors to participant copy',
      () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      return _jsonResponse(409, {
        'error': 'slug_candidate already has an open realm request',
      });
    });
    final repository = ApiRealmBootstrapRepository(ApiClient(dio));

    expect(
      () => repository.createRealmRequest(
        const CreateRealmRequestDraft(
          displayName: 'Tokyo slow coffee',
          slugCandidate: 'tokyo-slow-coffee',
          purposeText: 'Calm local meetings.',
          venueContextText: 'Tokyo cafe',
          expectedMemberShapeText: 'small',
          bootstrapRationaleText: 'Start bounded.',
          requestIdempotencyKey: 'realm-request-1',
        ),
      ),
      throwsA(
        isA<BusinessException>().having(
          (error) => error.message,
          'message',
          'このslugは確認中です。別のslugで申請してください。',
        ),
      ),
    );
  });

  test('api realm repository maps missing bootstrap summary to business error',
      () async {
    final dio = Dio();
    dio.httpClientAdapter = _StubHttpClientAdapter((options) async {
      return _jsonResponse(
        404,
        {'error': 'realm bootstrap view was not found'},
      );
    });
    final repository = ApiRealmBootstrapRepository(ApiClient(dio));

    expect(
      () => repository.fetchBootstrapSummary('realm-missing'),
      throwsA(
        isA<BusinessException>().having(
          (error) => error.message,
          'message',
          'Realmの状態を確認できませんでした。',
        ),
      ),
    );
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
