import 'package:dio/dio.dart';

import '../../api/api_client.dart';
import '../../core/errors/app_exception.dart';
import '../../features/promise/models/promise_models.dart';
import 'promise_repository.dart';

class ApiPromiseRepository implements PromiseRepository {
  ApiPromiseRepository(this._apiClient);

  final ApiClient _apiClient;

  @override
  Future<CreatePromiseIntentResponse> createPromiseIntent(
    CreatePromiseIntentRequest request,
  ) async {
    try {
      final response = await _apiClient.dio.post<Map<String, dynamic>>(
        '/api/promise/intents',
        data: request.toJson(),
      );
      return CreatePromiseIntentResponse.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } catch (error) {
      throw _mapPromiseError(error);
    }
  }

  @override
  Future<PromiseStatusBundle> fetchPromiseStatus(
    String promiseIntentId, {
    String? settlementCaseId,
  }) async {
    try {
      PromiseProjectionView? promise;
      ExpandedSettlementView? settlement;
      if (settlementCaseId == null) {
        promise = await _fetchPromiseProjection(promiseIntentId);
        final caseId = promise?.latestSettlementCaseId;
        settlement = caseId == null
            ? null
            : _matchingSettlement(
                await _fetchExpandedSettlementView(caseId),
                promiseIntentId,
              );
      } else {
        final results = await Future.wait<Object?>([
          _fetchPromiseProjection(promiseIntentId),
          _fetchExpandedSettlementView(settlementCaseId),
        ]);
        promise = results[0] as PromiseProjectionView?;
        settlement = _matchingSettlement(
          results[1] as ExpandedSettlementView?,
          promiseIntentId,
        );

        final latestCaseId = promise?.latestSettlementCaseId;
        if (latestCaseId != null && latestCaseId != settlementCaseId) {
          settlement = _matchingSettlement(
            await _fetchExpandedSettlementView(latestCaseId),
            promiseIntentId,
          );
        }
      }
      final displayAvailability = promise == null
          ? null
          : await _fetchParticipantSafeDisplayAvailability(
              promise,
            ).timeout(const Duration(seconds: 1), onTimeout: () => null);

      return PromiseStatusBundle(
        promiseIntentId: promiseIntentId,
        initialSettlementCaseId: settlementCaseId,
        promise: promise,
        settlement: settlement,
        participantSafeDisplayAvailability: displayAvailability,
      );
    } catch (error) {
      throw _mapPromiseError(error);
    }
  }

  Future<PromiseProjectionView?> _fetchPromiseProjection(
    String promiseIntentId,
  ) async {
    try {
      final response = await _apiClient.dio.get<Map<String, dynamic>>(
        '/api/projection/promise-views/$promiseIntentId',
      );
      return PromiseProjectionView.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } on DioException catch (error) {
      if (error.response?.statusCode == 404) {
        return null;
      }
      rethrow;
    }
  }

  Future<ExpandedSettlementView?> _fetchExpandedSettlementView(
    String settlementCaseId,
  ) async {
    try {
      final response = await _apiClient.dio.get<Map<String, dynamic>>(
        '/api/projection/settlement-views/$settlementCaseId/expanded',
      );
      return ExpandedSettlementView.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } on DioException catch (error) {
      if (error.response?.statusCode == 404) {
        return null;
      }
      rethrow;
    }
  }

  Future<ParticipantSafeDisplayAvailability?>
      _fetchParticipantSafeDisplayAvailability(
    PromiseProjectionView promise,
  ) async {
    final promiseReference = promise.promiseIntentId.trim();
    final realmId = promise.realmId.trim();
    if (promiseReference.isEmpty || realmId.isEmpty) {
      return null;
    }

    try {
      final response = await _apiClient.dio.get<Map<String, dynamic>>(
        '/api/promise-completion/participant-safe-display-availability/'
        '${Uri.encodeComponent(promiseReference)}',
        queryParameters: {'realm_id': realmId},
      );
      return ParticipantSafeDisplayAvailability.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } on DioException {
      return null;
    } on FormatException {
      return null;
    } on TypeError {
      return null;
    }
  }

  ExpandedSettlementView? _matchingSettlement(
    ExpandedSettlementView? settlement,
    String promiseIntentId,
  ) {
    if (settlement == null) {
      return null;
    }
    if (settlement.promiseIntentId != promiseIntentId) {
      return null;
    }
    return settlement;
  }

  AppException _mapPromiseError(Object error) {
    if (error is DioException) {
      final statusCode = error.response?.statusCode;
      final responseMessage = _errorResponseMessage(error.response?.data);
      if (statusCode == 404 ||
          (statusCode == 400 &&
              responseMessage == 'counterparty account was not found')) {
        return const BusinessException(
          message: '相手または約束の準備がまだ整っていません。時間を置いてもう一度確認してください。',
        );
      }
    }
    return AppExceptionMapper.fromObject(error);
  }

  String? _errorResponseMessage(Object? data) {
    if (data is Map<String, dynamic>) {
      final message = data['error'];
      if (message is String && message.isNotEmpty) {
        return message;
      }
    }
    if (data is Map) {
      final message = data['error'];
      if (message is String && message.isNotEmpty) {
        return message;
      }
    }
    return null;
  }
}
