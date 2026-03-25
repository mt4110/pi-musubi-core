import 'package:go_router/go_router.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../features/auth/presentation/pi_sign_in_screen.dart';
import '../features/home/presentation/home_screen.dart';
import '../features/home/presentation/match_detail_screen.dart';
import '../repositories/auth/auth_session_controller.dart';

final goRouterProvider = Provider<GoRouter>((ref) {
  final authAsync = ref.watch(authSessionControllerProvider);
  final isAuthenticated = authAsync.valueOrNull != null;

  return GoRouter(
    initialLocation: isAuthenticated ? '/home' : '/sign-in',
    redirect: (_, state) {
      final path = state.uri.path;
      final isSignInRoute = path == '/sign-in';
      final isHomeFlow = path == '/home' || path.startsWith('/detail/');

      if (authAsync.isLoading) {
        return isSignInRoute ? null : '/sign-in';
      }
      if (!isAuthenticated) {
        return isSignInRoute ? null : '/sign-in';
      }
      if (path == '/' || !isHomeFlow) {
        return '/home';
      }
      if (isAuthenticated && isSignInRoute) {
        return '/home';
      }
      return null;
    },
    routes: [
      GoRoute(
        path: '/',
        redirect: (_, __) => isAuthenticated ? '/home' : '/sign-in',
      ),
      GoRoute(
        path: '/sign-in',
        builder: (context, state) => const PiSignInScreen(),
      ),
      GoRoute(path: '/home', builder: (context, state) => const HomeScreen()),
      GoRoute(
        path: '/detail/:profileId',
        builder: (context, state) => MatchDetailScreen(
          profileId: state.pathParameters['profileId'] ?? '',
        ),
      ),
    ],
  );
});
