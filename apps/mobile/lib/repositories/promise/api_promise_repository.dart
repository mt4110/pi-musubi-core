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
      final promise = await _fetchPromiseProjection(promiseIntentId);
      final caseId = promise?.latestSettlementCaseId ?? settlementCaseId;
      final settlement =
          caseId == null ? null : await _fetchExpandedSettlementView(caseId);
      return PromiseStatusBundle(
        promiseIntentId: promiseIntentId,
        initialSettlementCaseId: settlementCaseId,
        promise: promise,
        settlement: settlement,
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

  AppException _mapPromiseError(Object error) {
    if (error is DioException && error.response?.statusCode == 404) {
      return const BusinessException(
        message: '相手または約束の準備がまだ整っていません。時間を置いてもう一度確認してください。',
      );
    }
    return AppExceptionMapper.fromObject(error);
  }
}
