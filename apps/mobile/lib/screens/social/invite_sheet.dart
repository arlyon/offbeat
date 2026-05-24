// OFFBEAT — Invite sheet (QR + code)
// Ticket card with QR code, code display, copy/share actions
// Matches groups-screens.jsx InviteSheet (lines 633–681)

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:qr_flutter/qr_flutter.dart';
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

  /// Renders a real QR code from the invite payload URI.
  Widget _buildQrPlaceholder() {
    return QrImageView(
      data: widget.groupCode,
      version: QrVersions.auto,
      size: 200,
      backgroundColor: const Color(0xFFF2F0EA),
      dataModuleStyle: const QrDataModuleStyle(
        color: Color(0xFF0B0B0C),
        dataModuleShape: QrDataModuleShape.square,
      ),
      eyeStyle: const QrEyeStyle(
        color: Color(0xFF0B0B0C),
        eyeShape: QrEyeShape.square,
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

