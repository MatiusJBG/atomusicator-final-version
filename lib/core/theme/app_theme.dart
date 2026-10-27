import 'package:flutter/material.dart';

abstract final class AppColors {
  static const canvas = Color(0xFF111B22);
  static const panel = Color(0xFF19262E);
  static const panelRaised = Color(0xFF20313A);
  static const line = Color(0xFF30434B);
  static const text = Color(0xFFE5ECE8);
  static const muted = Color(0xFF91A5A9);
  static const seaGlass = Color(0xFF72D7C3);
  static const amber = Color(0xFFF0B46A);
  static const coral = Color(0xFFE98172);
  static const lavender = Color(0xFFB6A8DB);
  static const blue = Color(0xFF82B9E7);
}

abstract final class AppTheme {
  static final dark = ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    scaffoldBackgroundColor: AppColors.canvas,
    colorScheme: const ColorScheme.dark(
      primary: AppColors.seaGlass,
      onPrimary: AppColors.canvas,
      secondary: AppColors.amber,
      surface: AppColors.panel,
      onSurface: AppColors.text,
      outline: AppColors.line,
      error: AppColors.coral,
    ),
    textTheme: const TextTheme(
      headlineMedium: TextStyle(
        color: AppColors.text,
        fontSize: 26,
        fontWeight: FontWeight.w600,
        letterSpacing: -0.6,
      ),
      titleLarge: TextStyle(
        color: AppColors.text,
        fontSize: 17,
        fontWeight: FontWeight.w600,
      ),
      titleMedium: TextStyle(
        color: AppColors.text,
        fontSize: 14,
        fontWeight: FontWeight.w600,
      ),
      bodyLarge: TextStyle(color: AppColors.text, fontSize: 14, height: 1.45),
      bodyMedium: TextStyle(color: AppColors.muted, fontSize: 13, height: 1.45),
      labelLarge: TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      labelMedium: TextStyle(
        color: AppColors.muted,
        fontSize: 11,
        fontWeight: FontWeight.w500,
      ),
    ),
    iconTheme: const IconThemeData(color: AppColors.muted, size: 19),
    dividerColor: AppColors.line,
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(
        color: AppColors.panelRaised,
        borderRadius: BorderRadius.circular(6),
      ),
      textStyle: const TextStyle(color: AppColors.text, fontSize: 12),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: AppColors.seaGlass,
        foregroundColor: AppColors.canvas,
        disabledBackgroundColor: AppColors.panelRaised,
        disabledForegroundColor: AppColors.muted,
        padding: const EdgeInsets.symmetric(horizontal: 17, vertical: 13),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: AppColors.text,
        side: const BorderSide(color: AppColors.line),
        padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 12),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)),
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(
        foregroundColor: AppColors.muted,
        minimumSize: const Size(40, 40),
      ),
    ),
    sliderTheme: SliderThemeData(
      activeTrackColor: AppColors.seaGlass,
      inactiveTrackColor: AppColors.line,
      thumbColor: AppColors.text,
      overlayColor: AppColors.seaGlass.withValues(alpha: 0.12),
      trackHeight: 3,
    ),
  );
}
