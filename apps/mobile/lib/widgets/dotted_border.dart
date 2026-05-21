// OFFBEAT DottedBorder — Utility for dotted/dashed borders
// Flutter doesn't have native dotted borders, so we use CustomPainter.
// The design uses 1.5px dotted var(--dotted) extensively.

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

enum DottedBorderSide { top, bottom, left, right, all }

class DottedBorder extends StatelessWidget {
  final Widget child;
  final Color color;
  final double strokeWidth;
  final double dashLength;
  final double gapLength;
  final Set<DottedBorderSide> sides;

  const DottedBorder({
    super.key,
    required this.child,
    this.color = colorDotted,
    this.strokeWidth = 1.5,
    this.dashLength = 3.0,
    this.gapLength = 3.0,
    this.sides = const {DottedBorderSide.all},
  });

  const DottedBorder.bottom({
    super.key,
    required this.child,
    this.color = colorDotted,
    this.strokeWidth = 1.5,
    this.dashLength = 3.0,
    this.gapLength = 3.0,
  }) : sides = const {DottedBorderSide.bottom};

  const DottedBorder.top({
    super.key,
    required this.child,
    this.color = colorDotted,
    this.strokeWidth = 1.5,
    this.dashLength = 3.0,
    this.gapLength = 3.0,
  }) : sides = const {DottedBorderSide.top};

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      painter: _DottedBorderPainter(
        color: color,
        strokeWidth: strokeWidth,
        dashLength: dashLength,
        gapLength: gapLength,
        sides: sides,
      ),
      child: child,
    );
  }
}

class _DottedBorderPainter extends CustomPainter {
  final Color color;
  final double strokeWidth;
  final double dashLength;
  final double gapLength;
  final Set<DottedBorderSide> sides;

  _DottedBorderPainter({
    required this.color,
    required this.strokeWidth,
    required this.dashLength,
    required this.gapLength,
    required this.sides,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = strokeWidth
      ..style = PaintingStyle.stroke;

    void drawDashedLine(Offset start, Offset end) {
      final dx = end.dx - start.dx;
      final dy = end.dy - start.dy;
      final len = (end - start).distance;
      final step = dashLength + gapLength;
      double covered = 0;
      while (covered < len) {
        final segEnd = (covered + dashLength).clamp(0.0, len);
        final ratio1 = covered / len;
        final ratio2 = segEnd / len;
        canvas.drawLine(
          Offset(start.dx + dx * ratio1, start.dy + dy * ratio1),
          Offset(start.dx + dx * ratio2, start.dy + dy * ratio2),
          paint,
        );
        covered += step;
      }
    }

    final drawAll = sides.contains(DottedBorderSide.all);

    if (drawAll || sides.contains(DottedBorderSide.top)) {
      drawDashedLine(Offset(0, strokeWidth / 2), Offset(size.width, strokeWidth / 2));
    }
    if (drawAll || sides.contains(DottedBorderSide.bottom)) {
      drawDashedLine(
        Offset(0, size.height - strokeWidth / 2),
        Offset(size.width, size.height - strokeWidth / 2),
      );
    }
    if (drawAll || sides.contains(DottedBorderSide.left)) {
      drawDashedLine(Offset(strokeWidth / 2, 0), Offset(strokeWidth / 2, size.height));
    }
    if (drawAll || sides.contains(DottedBorderSide.right)) {
      drawDashedLine(
        Offset(size.width - strokeWidth / 2, 0),
        Offset(size.width - strokeWidth / 2, size.height),
      );
    }
  }

  @override
  bool shouldRepaint(covariant _DottedBorderPainter oldDelegate) {
    return oldDelegate.color != color ||
        oldDelegate.strokeWidth != strokeWidth ||
        oldDelegate.dashLength != dashLength ||
        oldDelegate.gapLength != gapLength ||
        oldDelegate.sides != sides;
  }
}

// Convenience: dotted rule (horizontal line)
class DottedRule extends StatelessWidget {
  final Color color;
  final double height;

  const DottedRule({
    super.key,
    this.color = colorDotted,
    this.height = 1.5,
  });

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: Size(double.infinity, height),
      painter: _DottedLinePainter(color: color, strokeWidth: height),
    );
  }
}

class _DottedLinePainter extends CustomPainter {
  final Color color;
  final double strokeWidth;

  _DottedLinePainter({required this.color, required this.strokeWidth});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = strokeWidth
      ..style = PaintingStyle.stroke;
    const dashLength = 3.0;
    const gapLength = 3.0;
    double x = 0;
    final y = size.height / 2;
    while (x < size.width) {
      canvas.drawLine(
        Offset(x, y),
        Offset((x + dashLength).clamp(0.0, size.width), y),
        paint,
      );
      x += dashLength + gapLength;
    }
  }

  @override
  bool shouldRepaint(covariant _DottedLinePainter old) =>
      old.color != color || old.strokeWidth != strokeWidth;
}

// Vertical dotted rule
class VerticalDottedRule extends StatelessWidget {
  final Color color;
  final double width;

  const VerticalDottedRule({
    super.key,
    this.color = colorDotted,
    this.width = 1.5,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: width,
      height: double.infinity,
      child: CustomPaint(
        painter: _VerticalDottedPainter(color: color, strokeWidth: width),
      ),
    );
  }
}

class _VerticalDottedPainter extends CustomPainter {
  final Color color;
  final double strokeWidth;

  _VerticalDottedPainter({required this.color, required this.strokeWidth});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = strokeWidth
      ..style = PaintingStyle.stroke;
    const dashLength = 3.0;
    const gapLength = 3.0;
    final x = size.width / 2;
    double y = 0;
    while (y < size.height) {
      canvas.drawLine(
        Offset(x, y),
        Offset(x, (y + dashLength).clamp(0.0, size.height)),
        paint,
      );
      y += dashLength + gapLength;
    }
  }

  @override
  bool shouldRepaint(covariant _VerticalDottedPainter old) =>
      old.color != color || old.strokeWidth != strokeWidth;
}
