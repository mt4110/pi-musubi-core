import 'dart:math' as math;

import 'package:flutter/material.dart';

class AmbientParticles extends StatefulWidget {
  const AmbientParticles({super.key});

  @override
  State<AmbientParticles> createState() => _AmbientParticlesState();
}

class _AmbientParticlesState extends State<AmbientParticles>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 12),
  );

  @override
  void initState() {
    super.initState();
    if (_isTestBinding) {
      _controller.value = 0.42;
    } else {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: RepaintBoundary(
        child: AnimatedBuilder(
          animation: _controller,
          builder: (context, _) {
            return CustomPaint(
              painter: _AmbientParticlesPainter(progress: _controller.value),
            );
          },
        ),
      ),
    );
  }
}

class _AmbientParticlesPainter extends CustomPainter {
  const _AmbientParticlesPainter({required this.progress});

  final double progress;

  static const _particles = <_ParticleSeed>[
    _ParticleSeed(0.14, 0.00, 34, 0.05),
    _ParticleSeed(0.32, 0.18, 22, 0.035),
    _ParticleSeed(0.54, 0.36, 28, 0.04),
    _ParticleSeed(0.72, 0.54, 18, 0.03),
    _ParticleSeed(0.86, 0.72, 26, 0.028),
  ];

  @override
  void paint(Canvas canvas, Size size) {
    for (final particle in _particles) {
      final shifted = (progress + particle.offset) % 1;
      final eased = Curves.easeOut.transform(shifted);
      final y = size.height * (1.08 - eased * 1.28);
      final drift = math.sin((shifted * math.pi * 2) + particle.phase * 8) * 18;
      final x = (size.width * particle.phase) + drift;
      final radius = particle.radius * (0.88 + (1 - shifted) * 0.22);
      final color = const Color(
        0xFFE1B96F,
      ).withValues(alpha: particle.opacity * (1 - shifted * 0.55));
      final paint = Paint()
        ..shader = RadialGradient(
          colors: [
            color,
            color.withValues(alpha: color.a * 0.42),
            Colors.transparent,
          ],
        ).createShader(Rect.fromCircle(center: Offset(x, y), radius: radius));
      canvas.drawCircle(Offset(x, y), radius, paint);
    }
  }

  @override
  bool shouldRepaint(covariant _AmbientParticlesPainter oldDelegate) {
    return oldDelegate.progress != progress;
  }
}

class _ParticleSeed {
  const _ParticleSeed(this.phase, this.offset, this.radius, this.opacity);

  final double phase;
  final double offset;
  final double radius;
  final double opacity;
}

bool get _isTestBinding {
  return WidgetsBinding.instance.runtimeType.toString().contains(
    'TestWidgetsFlutterBinding',
  );
}
