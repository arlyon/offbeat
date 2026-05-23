// OFFBEAT — Invite sheet (QR + code)
// Ticket card with QR code, code display, copy/share actions
// Matches groups-screens.jsx InviteSheet (lines 633–681)

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class InviteSheet extends StatefulWidget {
  final String groupName;
  final String groupCode;
  final String festivalName;

  const InviteSheet({
    super.key,
    required this.groupName,
    required this.groupCode,
    required this.festivalName,
  });

  @override
  State<InviteSheet> createState() => _InviteSheetState();
}

class _InviteSheetState extends State<InviteSheet> {
  bool _copied = false;

  void _copyCode() {
    Clipboard.setData(ClipboardData(text: widget.groupCode));
    setState(() => _copied = true);
    Future.delayed(const Duration(milliseconds: 1600), () {
      if (mounted) setState(() => _copied = false);
    });
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.85,
      minChildSize: 0.5,
      maxChildSize: 0.95,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: Column(
          children: [
            // Grip
            Center(
              child: Container(
                margin: const EdgeInsets.only(top: 8),
                width: 36,
                height: 3,
                color: colorFg4,
              ),
            ),
            // Header
            _buildHeader(),
            // Body
            Expanded(
              child: SingleChildScrollView(
                controller: scrollController,
                padding: const EdgeInsets.all(18),
                child: _buildTicket(),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildHeader() {
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 12, 18, 10),
        child: Row(
          children: [
            Expanded(
              child: Text.rich(
                TextSpan(
                  children: [
                    const TextSpan(text: 'INVITE'),
                    const TextSpan(
                      text: '//',
                      style: TextStyle(color: colorAccent),
                    ),
                    TextSpan(text: widget.groupName.toUpperCase()),
                  ],
                ),
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.08 * 11,
                  color: colorFg,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            GestureDetector(
              onTap: () => Navigator.pop(context),
              child: const SizedBox(
                width: 28,
                height: 28,
                child: Center(
                  child: Icon(Icons.close, size: 16, color: colorFg2),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildTicket() {
    return Container(
      color: colorSurface1,
      margin: const EdgeInsets.only(top: 6),
      child: Column(
        children: [
          // Tear-off top
          _buildTear(isTop: true),
          // Stub meta
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 12, 18, 6),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text(
                  'OFFBEAT/GROUP',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.1 * 9,
                    color: colorFg3,
                  ),
                ),
                Text(
                  widget.festivalName.toUpperCase(),
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.1 * 9,
                    color: colorFg3,
                  ),
                ),
              ],
            ),
          ),
          // Group name
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 0, 18, 14),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text(
                widget.groupName.toUpperCase(),
                style: const TextStyle(
                  fontFamily: 'Helvetica',
                  fontWeight: FontWeight.w700,
                  fontSize: 22,
                  letterSpacing: -0.02 * 22,
                  height: 1,
                  color: colorFg,
                ),
              ),
            ),
          ),
          // QR frame
          Container(
            margin: const EdgeInsets.symmetric(horizontal: 18),
            padding: const EdgeInsets.all(14),
            color: colorFg,
            child: Center(
              child: _buildQrPlaceholder(),
            ),
          ),
          // Code block
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 14, 18, 16),
            child: Column(
              children: [
                const Text(
                  '\u2014 OR ENTER CODE \u2014',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.12 * 9,
                    color: colorFg3,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  widget.groupCode,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 28,
                    fontWeight: FontWeight.w500,
                    letterSpacing: 0.05 * 28,
                    color: colorFg,
                  ),
                ),
                const SizedBox(height: 12),
                // Copy / Share buttons
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    _codeActionButton(
                      label: _copied ? '\u2713 COPIED' : 'COPY CODE',
                      highlight: _copied,
                      onTap: _copyCode,
                    ),
                    const SizedBox(width: 8),
                    _codeActionButton(
                      label: 'SHARE LINK',
                      onTap: () {
                        // Share functionality — future feature
                      },
                    ),
                  ],
                ),
              ],
            ),
          ),
          // Tear-off bottom
          _buildTear(isTop: false),
        ],
      ),
    );
  }

  Widget _codeActionButton({
    required String label,
    bool highlight = false,
    required VoidCallback onTap,
  }) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        decoration: BoxDecoration(
          border: Border.all(
            color: highlight ? colorOk : colorDotted,
            width: 1.5,
          ),
        ),
        child: Text(
          label,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 10,
            fontWeight: FontWeight.w500,
            letterSpacing: 0.08 * 10,
            color: highlight ? colorOk : colorFg2,
          ),
        ),
      ),
    );
  }

  Widget _buildTear({required bool isTop}) {
    // Simplified tear-off visual (dotted line with semicircle notches)
    return SizedBox(
      height: 12,
      child: CustomPaint(
        size: const Size(double.infinity, 12),
        painter: _TearPainter(isTop: isTop),
      ),
    );
  }

  /// Generates a grid-pattern placeholder that looks like a QR code.
  Widget _buildQrPlaceholder() {
    return SizedBox(
      width: 200,
      height: 200,
      child: CustomPaint(
        painter: _QrPatternPainter(seed: widget.groupCode),
      ),
    );
  }
}

class _TearPainter extends CustomPainter {
  final bool isTop;
  _TearPainter({required this.isTop});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = colorDotted;
    final bgPaint = Paint()..color = colorBg;
    final notchY = isTop ? size.height : 0.0;

    // Draw dotted line
    const dashWidth = 3.0;
    const gapWidth = 3.0;
    double x = 0;
    while (x < size.width) {
      canvas.drawRect(
        Rect.fromLTWH(x, size.height / 2 - 0.75, dashWidth, 1.5),
        paint,
      );
      x += dashWidth + gapWidth;
    }

    // Draw notches
    const notchSpacing = 12.0;
    const notchRadius = 5.0;
    for (double nx = 6; nx < size.width; nx += notchSpacing) {
      canvas.drawCircle(Offset(nx, notchY), notchRadius, bgPaint);
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

/// Simple deterministic QR-like pattern painter
class _QrPatternPainter extends CustomPainter {
  final String seed;
  _QrPatternPainter({required this.seed});

  @override
  void paint(Canvas canvas, Size size) {
    final cellSize = size.width / 25;
    final fillPaint = Paint()..color = const Color(0xFF0B0B0C);
    final bgPaint = Paint()..color = const Color(0xFFF2F0EA);

    // Fill background
    canvas.drawRect(Offset.zero & size, bgPaint);

    // Simple LCG seeded by string
    int s = 0;
    for (int i = 0; i < seed.length; i++) {
      s = (s * 31 + seed.codeUnitAt(i)) & 0xFFFFFFFF;
    }
    int rng() {
      s = (s * 1664525 + 1013904223) & 0xFFFFFFFF;
      return s;
    }

    // Draw finder patterns (3 corners)
    void drawFinder(int r, int c) {
      for (int i = 0; i < 7; i++) {
        for (int j = 0; j < 7; j++) {
          final border = i == 0 || i == 6 || j == 0 || j == 6;
          final center = i >= 2 && i <= 4 && j >= 2 && j <= 4;
          if (border || center) {
            canvas.drawRect(
              Rect.fromLTWH(
                (c + j) * cellSize, (r + i) * cellSize,
                cellSize, cellSize,
              ),
              fillPaint,
            );
          }
        }
      }
    }

    drawFinder(0, 0);
    drawFinder(0, 18);
    drawFinder(18, 0);

    // Data fill
    for (int i = 0; i < 25; i++) {
      for (int j = 0; j < 25; j++) {
        // Skip finder areas
        if ((i < 8 && j < 8) || (i < 8 && j >= 17) || (i >= 17 && j < 8)) {
          continue;
        }
        if (rng() % 2 == 0) {
          canvas.drawRect(
            Rect.fromLTWH(j * cellSize, i * cellSize, cellSize, cellSize),
            fillPaint,
          );
        }
      }
    }

    // Brand mark in center
    final centerX = size.width / 2;
    final centerY = size.height / 2;
    final markSize = cellSize * 5;
    canvas.drawRect(
      Rect.fromCenter(center: Offset(centerX, centerY), width: markSize, height: markSize),
      bgPaint,
    );
    final accentPaint = Paint()..color = colorAccent;
    canvas.drawRect(
      Rect.fromCenter(center: Offset(centerX, centerY), width: markSize * 0.8, height: markSize * 0.8),
      accentPaint,
    );
    // Small bars inside
    final barPaint = Paint()..color = const Color(0xFF0B0B0C);
    final barW = markSize * 0.1;
    final barGap = markSize * 0.15;
    canvas.drawRect(
      Rect.fromLTWH(centerX - barGap - barW, centerY, barW, markSize * 0.25),
      barPaint,
    );
    canvas.drawRect(
      Rect.fromLTWH(centerX - barW / 2, centerY - markSize * 0.15, barW, markSize * 0.4),
      barPaint,
    );
    canvas.drawRect(
      Rect.fromLTWH(centerX + barGap, centerY - markSize * 0.05, barW, markSize * 0.3),
      barPaint,
    );
  }

  @override
  bool shouldRepaint(covariant _QrPatternPainter old) => old.seed != seed;
}
