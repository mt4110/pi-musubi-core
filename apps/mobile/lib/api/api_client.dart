import 'package:dio/dio.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import 'dio_provider.dart';

class ApiClient {
  ApiClient(this._dio);

  final Dio _dio;

  Dio get dio => _dio;
}

final apiClientProvider = Provider<ApiClient>((ref) {
  return ApiClient(ref.watch(dioProvider));
});
