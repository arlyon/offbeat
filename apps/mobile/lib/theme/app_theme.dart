// OFFBEAT App Theme
// Dark-mode only, brutalist, zero border-radius.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'tokens.dart';

ThemeData buildAppTheme() {
  return ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    scaffoldBackgroundColor: colorBg,
    colorScheme: const ColorScheme.dark(
      surface: colorBg,
      primary: colorAccent,
      onPrimary: colorAccentInk,
      secondary: colorCoAccent,
      onSecondary: colorBg,
      error: colorErr,
      onError: colorFg,
      onSurface: colorFg,
    ),
    // Typography — system sans as body, will swap mono via GoogleFonts where needed
    textTheme: const TextTheme(
      displayLarge: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tDisplay,
        fontWeight: FontWeight.w700,
        letterSpacing: trTight * tDisplay,
        height: lhTight,
        color: colorFg,
      ),
      headlineLarge: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tH1,
        fontWeight: FontWeight.w700,
        letterSpacing: trTight * tH1,
        height: lhTight,
        color: colorFg,
      ),
      headlineMedium: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tH2,
        fontWeight: FontWeight.w700,
        letterSpacing: trTight * tH2,
        height: lhSnug,
        color: colorFg,
      ),
      headlineSmall: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tH3,
        fontWeight: FontWeight.w700,
        height: lhSnug,
        color: colorFg,
      ),
      bodyLarge: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tBody,
        height: lhBody,
        color: colorFg,
      ),
      bodyMedium: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tSmall,
        height: lhBody,
        color: colorFg,
      ),
      bodySmall: TextStyle(
        fontFamily: 'Helvetica',
        fontSize: tMeta,
        height: lhBody,
        color: colorFg3,
      ),
    ),
    // AppBar
    appBarTheme: const AppBarTheme(
      backgroundColor: colorBg,
      foregroundColor: colorFg,
      elevation: 0,
      systemOverlayStyle: SystemUiOverlayStyle(
        statusBarColor: Colors.transparent,
        statusBarBrightness: Brightness.dark,
        statusBarIconBrightness: Brightness.light,
      ),
    ),
    // All shapes: zero border-radius
    cardTheme: const CardThemeData(
      color: colorSurface1,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.zero,
      ),
      margin: EdgeInsets.zero,
      elevation: 0,
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: colorSurface1,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.zero,
        borderSide: BorderSide(color: colorDotted, width: bdDotWidth),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.zero,
        borderSide: BorderSide(color: colorDotted, width: bdDotWidth),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.zero,
        borderSide: BorderSide(color: colorAccent, width: bdDotWidth),
      ),
      hintStyle: const TextStyle(color: colorFg4, fontSize: 14),
      contentPadding: const EdgeInsets.symmetric(horizontal: sp3, vertical: sp2),
    ),
    dividerTheme: const DividerThemeData(
      color: colorHairline,
      thickness: 1.0,
      space: 0,
    ),
    elevatedButtonTheme: ElevatedButtonThemeData(
      style: ElevatedButton.styleFrom(
        backgroundColor: colorAccent,
        foregroundColor: colorAccentInk,
        shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
        elevation: 0,
        textStyle: const TextStyle(
          fontSize: tMeta,
          fontWeight: FontWeight.w500,
          letterSpacing: trMeta * tMeta,
        ),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: colorFg,
        side: const BorderSide(color: colorFg3, width: bdDotWidth),
        shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
        elevation: 0,
      ),
    ),
    iconTheme: const IconThemeData(color: colorFg, size: 18),
    bottomNavigationBarTheme: const BottomNavigationBarThemeData(
      backgroundColor: colorBg,
      selectedItemColor: colorAccent,
      unselectedItemColor: colorFg3,
      type: BottomNavigationBarType.fixed,
      elevation: 0,
    ),
    splashFactory: NoSplash.splashFactory,
    highlightColor: Colors.transparent,
    splashColor: Colors.transparent,
  );
}
