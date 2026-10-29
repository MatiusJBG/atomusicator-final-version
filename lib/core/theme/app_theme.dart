import 'package:flutter/material.dart';

abstract final class AppColors {
  static const canvas = Color(0xFF0B0F14);
  static const sidebar = Color(0xFF090E14);
  static const panel = Color(0xFF131A22);
  static const panelRaised = Color(0xFF1C2731);
  static const line = Color(0xFF22302F);
  static const text = Color(0xFFE6EDF3);
  static const muted = Color(0xFF7E93A8);
  static const seaGlass = Color(0xFF2DD4BF);
  static const amber = Color(0xFFFFB000);
  static const coral = Color(0xFFFF4163);
  static const lavender = Color(0xFFA653F5);
  static const blue = Color(0xFF3789FF);
  static const green = Color(0xFF00C495);
  static const playhead = Color(0xFFFFA147);
}

abstract final class AppTheme {
  static final dark = ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    fontFamily: 'Inter',
    visualDensity: VisualDensity.compact,
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
      bodyMedium: TextStyle(color: AppColors.text, fontSize: 13, height: 1.45),
      labelLarge: TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      labelMedium: TextStyle(
        color: AppColors.muted,
        fontSize: 11,
        fontWeight: FontWeight.w500,
      ),
    ),
    iconTheme: const IconThemeData(color: AppColors.muted, size: 19),
    dividerColor: AppColors.line,
    dividerTheme: const DividerThemeData(color: AppColors.line, thickness: 1),
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
        textStyle: const TextStyle(
          fontFamily: 'Inter',
          fontSize: 12,
          fontWeight: FontWeight.w600,
        ),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(7)),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: AppColors.text,
        side: const BorderSide(color: AppColors.line),
        padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 12),
        textStyle: const TextStyle(
          fontFamily: 'Inter',
          fontSize: 12,
          fontWeight: FontWeight.w600,
        ),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(7)),
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(
        foregroundColor: AppColors.muted,
        minimumSize: const Size(40, 40),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        textStyle: const TextStyle(fontFamily: 'Inter', fontSize: 11),
      ),
    ),
    chipTheme: const ChipThemeData(
      backgroundColor: AppColors.canvas,
      selectedColor: Color(0xFF10332E),
      checkmarkColor: AppColors.seaGlass,
      labelStyle: TextStyle(
        fontFamily: 'Inter',
        fontSize: 10,
        color: AppColors.text,
      ),
      secondaryLabelStyle: TextStyle(
        fontFamily: 'Inter',
        fontSize: 10,
        color: AppColors.seaGlass,
      ),
      side: BorderSide(color: AppColors.line),
      shape: StadiumBorder(),
    ),
    sliderTheme: SliderThemeData(
      activeTrackColor: AppColors.seaGlass,
      inactiveTrackColor: AppColors.line,
      thumbColor: AppColors.text,
      overlayColor: AppColors.seaGlass.withValues(alpha: 0.12),
      trackHeight: 3,
      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 4),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: AppColors.panel,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: const BorderSide(color: AppColors.line),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: AppColors.canvas,
      isDense: true,
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(5)),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(5),
        borderSide: const BorderSide(color: AppColors.line),
      ),
    ),
  );
}
