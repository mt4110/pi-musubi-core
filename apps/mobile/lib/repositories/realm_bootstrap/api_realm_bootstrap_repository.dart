import 'package:dio/dio.dart';

import '../../api/api_client.dart';
import '../../core/errors/app_exception.dart';
import '../../features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'realm_bootstrap_repository.dart';

class ApiRealmBootstrapRepository implements RealmBootstrapRepository {
  ApiRealmBootstrapRepository(this._apiClient);

  final ApiClient _apiClient;

  @override
  Future<RealmRequestView> createRealmRequest(
    CreateRealmRequestDraft draft,
  ) async {
    try {
      final response = await _apiClient.dio.post<Map<String, dynamic>>(
        '/api/realms/requests',
        data: draft.toJson(),
      );
      return RealmRequestView.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } catch (error) {
      throw _mapRealmBootstrapError(error);
    }
  }

  @override
  Future<RealmRequestView> fetchRealmRequest(String realmRequestId) async {
    try {
      final response = await _apiClient.dio.get<Map<String, dynamic>>(
        '/api/realms/requests/${Uri.encodeComponent(realmRequestId)}',
      );
      return RealmRequestView.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } catch (error) {
      throw _mapRealmBootstrapError(
        error,
        notFoundMessage: 'Realm申請を確認できませんでした。',
      );
    }
  }

  @override
  Future<RealmBootstrapSummaryBundle> fetchBootstrapSummary(
    String realmId,
  ) async {
    try {
      final response = await _apiClient.dio.get<Map<String, dynamic>>(
        '/api/projection/realms/${Uri.encodeComponent(realmId)}/bootstrap-summary',
      );
      return RealmBootstrapSummaryBundle.fromJson(
        response.data ?? const <String, dynamic>{},
      );
    } catch (error) {
      throw _mapRealmBootstrapError(
        error,
        notFoundMessage: 'Realmの状態を確認できませんでした。',
      );
    }
  }

  AppException _mapRealmBootstrapError(
    Object error, {
    String? notFoundMessage,
  }) {
    if (error is DioException) {
      final statusCode = error.response?.statusCode;
      if (statusCode == 404 && notFoundMessage != null) {
        return BusinessException(message: notFoundMessage);
      }
      if (statusCode == 400 || statusCode == 409) {
        return BusinessException(
          message: _errorResponseMessage(error.response?.data) ??
              'Realm申請の内容を確認してください。',
        );
      }
    }
    return AppExceptionMapper.fromObject(error);
  }

  String? _errorResponseMessage(Object? data) {
    if (data is! Map) {
      return null;
    }
    final message = data['error'];
    if (message is String && message.isNotEmpty) {
      return message;
    }
    return null;
  }
}
