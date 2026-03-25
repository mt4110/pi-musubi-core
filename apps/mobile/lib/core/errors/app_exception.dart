import 'dart:async';

import 'package:dio/dio.dart';

sealed class AppException implements Exception {
  const AppException({required this.message});

  final String message;

  @override
  String toString() => message;
}

class NetworkTimeoutException extends AppException {
  const NetworkTimeoutException({
    super.message = '通信がタイムアウトしました。時間を置いて再試行してください。',
  });
}

class NetworkConnectionException extends AppException {
  const NetworkConnectionException({super.message = 'ネットワークに接続できませんでした。'});
}

class ApiStatusException extends AppException {
  const ApiStatusException({required this.statusCode, required super.message});

  final int statusCode;
}

class BusinessException extends AppException {
  const BusinessException({super.message = '処理に失敗しました。'});
}

class AuthenticationException extends AppException {
  const AuthenticationException({super.message = '認証の有効期限が切れました。再ログインしてください。'});
}

class AuthenticationCancelledException extends AuthenticationException {
  const AuthenticationCancelledException({super.message = 'サインインをキャンセルしました。'});
}

class UnknownAppException extends AppException {
  const UnknownAppException({super.message = '予期しないエラーが発生しました。'});
}

class AppExceptionMapper {
  const AppExceptionMapper._();

  static AppException fromObject(Object error) {
    if (error is AppException) {
      return error;
    }
    if (error is DioException) {
      return _fromDioException(error);
    }
    if (error is TimeoutException) {
      return const NetworkTimeoutException();
    }

    final normalized = '$error'.toLowerCase();
    if (normalized.contains('cancel')) {
      return const AuthenticationCancelledException();
    }
    if (normalized.contains('auth')) {
      return AuthenticationException(message: error.toString());
    }
    return UnknownAppException(message: error.toString());
  }

  static AppException _fromDioException(DioException error) {
    switch (error.type) {
      case DioExceptionType.connectionTimeout:
      case DioExceptionType.sendTimeout:
      case DioExceptionType.receiveTimeout:
        return const NetworkTimeoutException();
      case DioExceptionType.connectionError:
      case DioExceptionType.badCertificate:
      case DioExceptionType.cancel:
        return const NetworkConnectionException();
      case DioExceptionType.badResponse:
        final statusCode = error.response?.statusCode ?? 500;
        if (statusCode == 401 || statusCode == 403) {
          return const AuthenticationException();
        }
        if (statusCode == 408) {
          return const NetworkTimeoutException();
        }
        return ApiStatusException(
          statusCode: statusCode,
          message: 'APIエラー ($statusCode) が発生しました。',
        );
      case DioExceptionType.unknown:
        return UnknownAppException(message: error.message ?? '通信に失敗しました。');
    }
  }
}
