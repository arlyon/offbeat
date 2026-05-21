// OFFBEAT Design Tokens
// Translated from docs/designs/lineup/project/ds/colors_and_type.css

import 'package:flutter/material.dart';

// ── Colors: Ground & Surface ─────────────────────────────────
const Color colorVoid = Color(0xFF000000);
const Color colorBg = Color(0xFF0B0B0C);
const Color colorSurface1 = Color(0xFF131315);
const Color colorSurface2 = Color(0xFF1B1B1E);
const Color colorSurface3 = Color(0xFF232327);
const Color colorHairline = Color(0xFF2A2A2E);
const Color colorDotted = Color(0xFF3A3A40);

// ── Colors: Foreground ───────────────────────────────────────
const Color colorFg = Color(0xFFF2F0EA);
const Color colorFg2 = Color(0xFFB8B6B0);
const Color colorFg3 = Color(0xFF7A7873);
const Color colorFg4 = Color(0xFF4A4845);

// ── Colors: Brand & Semantic ─────────────────────────────────
const Color colorAccent = Color(0xFFFF2D8F);
const Color colorAccentInk = Color(0xFF0B0B0C);
const Color colorAccentDim = Color(0xFFB81E68);
const Color colorAccentWash = Color(0xFF2A0F1E);

const Color colorCoAccent = Color(0xFF3DDBD9);
const Color colorWarn = Color(0xFFFFB347);
const Color colorOk = Color(0xFF9BE15D);
const Color colorErr = Color(0xFFFF4D4D);

// ── Colors: Stages ───────────────────────────────────────────
const Color colorStage1 = Color(0xFFFF2D8F);
const Color colorStage2 = Color(0xFF3DDBD9);
const Color colorStage3 = Color(0xFFFFB347);
const Color colorStage4 = Color(0xFF9BE15D);
const Color colorStage5 = Color(0xFFC77DFF);
const Color colorStage6 = Color(0xFFFF8C42);

const List<Color> stageColors = [
  colorStage1,
  colorStage2,
  colorStage3,
  colorStage4,
  colorStage5,
  colorStage6,
];

// ── Typography ───────────────────────────────────────────────
// Sans: system sans (mapped to default Flutter sans)
// Mono: JetBrains Mono (via google_fonts)

// Type scale (logical px values)
const double tDisplay = 40.0; // clamp, use 40 as base for mobile
const double tH1 = 28.0;
const double tH2 = 20.0;
const double tH3 = 16.0;
const double tBody = 15.0;
const double tSmall = 13.0;
const double tMeta = 11.0;

// Line heights
const double lhTight = 1.02;
const double lhSnug = 1.18;
const double lhBody = 1.45;

// Letter spacings (em values converted, approx at 15px base)
const double trTight = -0.02; // em — negative tracking for big sans heads
const double trMono = -0.01; // em — mono runs
const double trMeta = 0.08; // em — spaced caps for labels

// ── Spacing (4px base) ───────────────────────────────────────
const double sp1 = 4.0;
const double sp2 = 8.0;
const double sp3 = 12.0;
const double sp4 = 16.0;
const double sp5 = 24.0;
const double sp6 = 32.0;
const double sp7 = 48.0;
const double sp8 = 64.0;
const double sp9 = 96.0;

// ── Layout ───────────────────────────────────────────────────
const double navH = 52.0;
const double tabH = 56.0;
const double statusBarH = 28.0;
const double tapMin = 44.0; // minimum tap target

// ── Borders ──────────────────────────────────────────────────
const double bdDotWidth = 1.5;
const double bdWidth = 1.0;

// ── Motion ──────────────────────────────────────────────────
const Duration durationFast = Duration(milliseconds: 150);
const Duration durationMedium = Duration(milliseconds: 280);
const Duration durationSlow = Duration(milliseconds: 380);
const Cubic curveBrutalist = Cubic(0.2, 0.7, 0.2, 1.0);

// ── Gantt layout constants ───────────────────────────────────
const int ganttStartMin = 18 * 60; // 18:00
const int ganttEndMin = 26 * 60; // 02:00+1
const int ganttRangeMin = ganttEndMin - ganttStartMin; // 480 min
const double ganttPxPerMin = 3.0;
const double ganttContentW = ganttRangeMin * ganttPxPerMin; // 1440px
const double ganttStageLabelW = 46.0;

// ── FestArt gradient palettes (hue index 1–5) ────────────────
List<Color> festArtGradient(int hue) {
  switch (hue) {
    case 1:
      return [const Color(0xFF14040A), const Color(0xFFFF2D8F)];
    case 2:
      return [const Color(0xFF001818), const Color(0xFF3DDBD9)];
    case 3:
      return [const Color(0xFF1A1408), const Color(0xFFFFB347)];
    case 4:
      return [const Color(0xFF0A1A08), const Color(0xFF9BE15D)];
    case 5:
      return [const Color(0xFF1A0A1F), const Color(0xFFC77DFF)];
    default:
      return [const Color(0xFF14040A), const Color(0xFFFF2D8F)];
  }
}
