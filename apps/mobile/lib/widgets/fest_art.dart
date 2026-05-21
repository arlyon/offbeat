// OFFBEAT FestArt — Festival art tile
// Gradient background based on hue index (5 palettes)
// SVG grain overlay as noise texture
// Optional label at bottom-left

import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class FestArt extends StatelessWidget {
  final int hue;
  final double width;
  final double height;
  final String? label;

  const FestArt({
    super.key,
    required this.hue,
    this.width = 80,
    this.height = 80,
    this.label,
  });

  @override
  Widget build(BuildContext context) {
    final colors = festArtGradient(hue);
    return SizedBox(
      width: width,
      height: height,
      child: CustomPaint(
        painter: _FestArtPainter(colors: colors),
        child: Stack(
          children: [
            // Grain overlay via shader (approximated via noise pattern)
            Positioned.fill(
              child: _GrainOverlay(),
            ),
            // Label bottom-left
            if (label != null)
              Positioned(
                bottom: 6,
                left: 8,
                child: Text(
                  label!.toUpperCase(),
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    color: Color(0xB3FFFFFF), // rgba(255,255,255,0.7)
                    letterSpacing: 0.1 * 9,
                    height: 1,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _FestArtPainter extends CustomPainter {
  final List<Color> colors;
  const _FestArtPainter({required this.colors});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: colors,
      ).createShader(Rect.fromLTWH(0, 0, size.width, size.height));
    canvas.drawRect(Rect.fromLTWH(0, 0, size.width, size.height), paint);
  }

  @override
  bool shouldRepaint(covariant _FestArtPainter old) => old.colors != colors;
}

class _GrainOverlay extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    // Approximate grain with a fine dot pattern using CustomPaint
    return CustomPaint(
      painter: _GrainPainter(),
    );
  }
}

class _GrainPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = Colors.white.withOpacity(0.04)
      ..style = PaintingStyle.fill;

    // Create a fine noise pattern by painting tiny dots pseudo-randomly
    // Use a simple LCG to get consistent "random" placement
    int seed = 42;
    int next() {
      seed = (seed * 1664525 + 1013904223) & 0xFFFFFFFF;
      return seed;
    }

    final dotCount = (size.width * size.height / 8).round();
    for (int i = 0; i < dotCount; i++) {
      final x = (next() % size.width.toInt()).toDouble();
      final y = (next() % size.height.toInt()).toDouble();
      canvas.drawCircle(Offset(x, y), 0.5, paint);
    }
  }

  @override
  bool shouldRepaint(covariant _GrainPainter old) => false;
}
