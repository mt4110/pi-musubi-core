import 'package:flutter/material.dart';

const _jpSerifFallback = <String>[
  'Hiragino Mincho ProN',
  'YuMincho',
  'Noto Serif CJK JP',
  'Noto Serif JP',
];

const _oledBlack = Color(0xFF0A0A0A);
const _surfaceBlack = Color(0xFF12161C);
const _surfaceRaised = Color(0xFF181E26);
const _warmAmber = Color(0xFFE6B866);
const _ivory = Color(0xFFF3EBDD);
const _mutedIvory = Color(0xFFA79E91);

final ThemeData musubiTheme = ThemeData(
  brightness: Brightness.dark,
  colorScheme: const ColorScheme.dark(
    primary: _warmAmber,
    secondary: Color(0xFF74A086),
    surface: _surfaceBlack,
    onPrimary: Color(0xFF241A0D),
    onSecondary: Color(0xFF08110B),
    onSurface: _ivory,
    error: Color(0xFFFF8F7A),
    onError: Color(0xFF2D0D08),
  ),
  useMaterial3: true,
  scaffoldBackgroundColor: _oledBlack,
  canvasColor: _oledBlack,
  splashFactory: NoSplash.splashFactory,
  highlightColor: Colors.transparent,
  hoverColor: Colors.transparent,
  dividerColor: const Color(0x14FFFFFF),
  appBarTheme: const AppBarTheme(
    centerTitle: false,
    backgroundColor: _oledBlack,
    foregroundColor: _ivory,
    surfaceTintColor: Colors.transparent,
    elevation: 0,
  ),
  textTheme: const TextTheme(
    displayLarge: TextStyle(
      fontSize: 72,
      height: 0.94,
      letterSpacing: -1.8,
      fontWeight: FontWeight.w700,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    headlineSmall: TextStyle(
      fontSize: 28,
      height: 1.18,
      letterSpacing: -0.2,
      fontWeight: FontWeight.w700,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    titleLarge: TextStyle(
      fontSize: 24,
      height: 1.22,
      letterSpacing: -0.16,
      fontWeight: FontWeight.w700,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    titleMedium: TextStyle(
      fontSize: 18,
      height: 1.38,
      letterSpacing: 0.08,
      fontWeight: FontWeight.w600,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    bodyLarge: TextStyle(
      fontSize: 16,
      height: 1.62,
      letterSpacing: 0.04,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    bodyMedium: TextStyle(
      fontSize: 15,
      height: 1.62,
      letterSpacing: 0.04,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    bodySmall: TextStyle(
      fontSize: 13,
      height: 1.5,
      letterSpacing: 0.04,
      color: _mutedIvory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    labelLarge: TextStyle(
      fontSize: 14,
      height: 1.4,
      letterSpacing: 0.12,
      fontWeight: FontWeight.w600,
      color: _ivory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    labelMedium: TextStyle(
      fontSize: 12,
      height: 1.35,
      letterSpacing: 0.12,
      fontWeight: FontWeight.w600,
      color: _mutedIvory,
      fontFamilyFallback: _jpSerifFallback,
    ),
    labelSmall: TextStyle(
      fontSize: 10,
      height: 1.3,
      letterSpacing: 1.6,
      fontWeight: FontWeight.w600,
      color: _mutedIvory,
      fontFamilyFallback: _jpSerifFallback,
    ),
  ),
  cardTheme: const CardThemeData(
    color: _surfaceBlack,
    elevation: 0,
    margin: EdgeInsets.zero,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(20)),
      side: BorderSide(color: Color(0x0DFFFFFF)),
    ),
  ),
  inputDecorationTheme: const InputDecorationTheme(
    filled: true,
    fillColor: _surfaceRaised,
    contentPadding: EdgeInsets.symmetric(horizontal: 18, vertical: 16),
    labelStyle: TextStyle(color: _mutedIvory),
    hintStyle: TextStyle(color: _mutedIvory),
    enabledBorder: OutlineInputBorder(
      borderRadius: BorderRadius.all(Radius.circular(18)),
      borderSide: BorderSide(color: Color(0x10FFFFFF)),
    ),
    focusedBorder: OutlineInputBorder(
      borderRadius: BorderRadius.all(Radius.circular(18)),
      borderSide: BorderSide(color: Color(0x3DE6B866)),
    ),
    border: OutlineInputBorder(
      borderRadius: BorderRadius.all(Radius.circular(18)),
      borderSide: BorderSide(color: Color(0x10FFFFFF)),
    ),
  ),
  progressIndicatorTheme: const ProgressIndicatorThemeData(
    color: _warmAmber,
    linearTrackColor: Color(0x1FFFFFFF),
    circularTrackColor: Color(0x1FFFFFFF),
  ),
  navigationBarTheme: const NavigationBarThemeData(
    backgroundColor: _oledBlack,
    indicatorColor: Colors.transparent,
    labelBehavior: NavigationDestinationLabelBehavior.alwaysHide,
    height: 74,
  ),
  snackBarTheme: const SnackBarThemeData(
    behavior: SnackBarBehavior.floating,
    backgroundColor: _surfaceRaised,
    contentTextStyle: TextStyle(color: _ivory),
    insetPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 12),
  ),
);
