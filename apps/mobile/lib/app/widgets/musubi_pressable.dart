import 'package:flutter/material.dart';

const Duration musubiSpringDuration = Duration(milliseconds: 320);
const Duration musubiFastDuration = Duration(milliseconds: 220);
const Curve musubiSpringCurve = Curves.easeOutExpo;
const double musubiPressedScale = 0.96;
const double musubiPressedOpacity = 0.7;
const Color musubiGlassStrokeColor = Color(0x0DFFFFFF);

List<BoxShadow> musubiAmbientGlow({
  Color color = const Color(0xFF2E3528),
  double opacity = 0.08,
  double blurRadius = 24,
  double spreadRadius = 0,
  Offset offset = const Offset(0, 12),
}) {
  return [
    BoxShadow(
      color: color.withValues(alpha: opacity),
      blurRadius: blurRadius,
      spreadRadius: spreadRadius,
      offset: offset,
    ),
  ];
}

class MusubiPressable extends StatefulWidget {
  const MusubiPressable({
    super.key,
    required this.child,
    this.onTap,
    this.padding = EdgeInsets.zero,
    this.decoration,
    this.disabledOpacity = 0.45,
    this.behavior = HitTestBehavior.translucent,
  });

  final Widget child;
  final VoidCallback? onTap;
  final EdgeInsetsGeometry padding;
  final Decoration? decoration;
  final double disabledOpacity;
  final HitTestBehavior behavior;

  @override
  State<MusubiPressable> createState() => _MusubiPressableState();
}

class _MusubiPressableState extends State<MusubiPressable> {
  bool _pressed = false;

  bool get _enabled => widget.onTap != null;

  void _setPressed(bool value) {
    if (!_enabled || _pressed == value) {
      return;
    }
    setState(() => _pressed = value);
  }

  @override
  Widget build(BuildContext context) {
    final opacity = !_enabled
        ? widget.disabledOpacity
        : _pressed
        ? musubiPressedOpacity
        : 1.0;
    final child = AnimatedContainer(
      duration: musubiSpringDuration,
      curve: musubiSpringCurve,
      padding: widget.padding,
      decoration: widget.decoration,
      child: widget.child,
    );

    return MouseRegion(
      cursor: _enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      child: GestureDetector(
        behavior: widget.behavior,
        onTap: widget.onTap,
        onTapDown: _enabled ? (_) => _setPressed(true) : null,
        onTapUp: _enabled ? (_) => _setPressed(false) : null,
        onTapCancel: _enabled ? () => _setPressed(false) : null,
        child: AnimatedOpacity(
          duration: musubiFastDuration,
          curve: musubiSpringCurve,
          opacity: opacity,
          child: AnimatedScale(
            duration: musubiSpringDuration,
            curve: musubiSpringCurve,
            scale: _enabled && _pressed ? musubiPressedScale : 1,
            child: child,
          ),
        ),
      ),
    );
  }
}

class MusubiPrimaryButton extends StatelessWidget {
  const MusubiPrimaryButton({
    super.key,
    required this.label,
    this.onPressed,
    this.icon,
    this.isBusy = false,
    this.backgroundColor = const Color(0xFFE2B76A),
    this.foregroundColor = const Color(0xFF1B1308),
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool isBusy;
  final Color backgroundColor;
  final Color foregroundColor;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null && !isBusy;
    return MusubiPressable(
      onTap: enabled ? onPressed : null,
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 16),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: musubiGlassStrokeColor),
        boxShadow: musubiAmbientGlow(
          color: backgroundColor,
          opacity: enabled ? 0.12 : 0.06,
          blurRadius: 30,
          spreadRadius: 1,
          offset: const Offset(0, 14),
        ),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (isBusy)
            const SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          else if (icon != null) ...[
            Icon(icon, color: foregroundColor, size: 18),
            const SizedBox(width: 10),
          ],
          Flexible(
            child: Text(
              label,
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: foregroundColor,
                fontWeight: FontWeight.w800,
                letterSpacing: 0.1,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class MusubiGhostButton extends StatelessWidget {
  const MusubiGhostButton({
    super.key,
    required this.label,
    this.onPressed,
    this.foregroundColor = const Color(0xFFEDE2CD),
    this.backgroundColor = const Color(0x14FFFFFF),
  });

  final String label;
  final VoidCallback? onPressed;
  final Color foregroundColor;
  final Color backgroundColor;

  @override
  Widget build(BuildContext context) {
    return MusubiPressable(
      onTap: onPressed,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(18),
        border: Border.all(color: musubiGlassStrokeColor),
        boxShadow: musubiAmbientGlow(
          color: foregroundColor,
          opacity: 0.05,
          blurRadius: 22,
          spreadRadius: 1,
          offset: const Offset(0, 10),
        ),
      ),
      child: Text(
        label,
        textAlign: TextAlign.center,
        style: Theme.of(context).textTheme.labelLarge?.copyWith(
          color: foregroundColor,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }
}

class MusubiSurfaceCard extends StatelessWidget {
  const MusubiSurfaceCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(18),
    this.color = const Color(0xFF12161C),
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(24),
        border: Border.all(color: musubiGlassStrokeColor),
        boxShadow: musubiAmbientGlow(
          color: const Color(0xFF6A5840),
          opacity: 0.08,
          blurRadius: 28,
          spreadRadius: 2,
          offset: const Offset(0, 16),
        ),
      ),
      child: Padding(padding: padding, child: child),
    );
  }
}
