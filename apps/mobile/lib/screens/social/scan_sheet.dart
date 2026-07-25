// OFFBEAT -- QR scanner sheet for joining groups
// Scans offbeat:// invite URIs via camera

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class ScanSheet extends StatefulWidget {
  final void Function(String uri) onScanned;

  const ScanSheet({super.key, required this.onScanned});

  @override
  State<ScanSheet> createState() => _ScanSheetState();
}

class _ScanSheetState extends State<ScanSheet> {
  final MobileScannerController _controller = MobileScannerController();
  String? _error;
  bool _scanned = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _handleBarcode(BarcodeCapture capture) {
    if (_scanned) return;
    for (final barcode in capture.barcodes) {
      final value = barcode.rawValue;
      if (value == null) continue;
      if (value.startsWith('offbeat://group/')) {
        setState(() => _scanned = true);
        _controller.stop();
        widget.onScanned(value);
        Navigator.pop(context);
        return;
      } else {
        setState(() => _error = 'NOT AN OFFBEAT CODE');
        Future.delayed(const Duration(seconds: 2), () {
          if (mounted) setState(() => _error = null);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.75,
      minChildSize: 0.5,
      maxChildSize: 0.9,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorBg,
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
            DottedBorder.bottom(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(18, 12, 18, 10),
                child: Row(
                  children: [
                    const Expanded(
                      child: Text(
                        'SCAN//QR',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 11,
                          fontWeight: FontWeight.w500,
                          letterSpacing: 0.08 * 11,
                          color: colorFg,
                        ),
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
            ),
            // Scanner
            Expanded(
              child: Stack(
                children: [
                  Padding(
                    padding: const EdgeInsets.all(18),
                    child: ClipRRect(
                      child: MobileScanner(
                        controller: _controller,
                        onDetect: _handleBarcode,
                      ),
                    ),
                  ),
                  // Corner accents
                  ..._buildCornerAccents(),
                  // Error message
                  if (_error != null)
                    Positioned(
                      bottom: 36,
                      left: 0,
                      right: 0,
                      child: Center(
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 6,
                          ),
                          color: colorBg,
                          child: Text(
                            _error!,
                            style: const TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 10,
                              fontWeight: FontWeight.w500,
                              letterSpacing: 0.08 * 10,
                              color: colorErr,
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            // Hint
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 0, 18, 18),
              child: Text(
                'POINT AT AN OFFBEAT INVITE QR',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  letterSpacing: 0.1 * 9,
                  color: colorFg3,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  List<Widget> _buildCornerAccents() {
    const size = 24.0;
    const thickness = 2.0;
    const margin = 18.0;

    Widget corner(Alignment align) {
      final isLeft =
          align == Alignment.topLeft || align == Alignment.bottomLeft;
      final isTop = align == Alignment.topLeft || align == Alignment.topRight;
      return Positioned(
        left: isLeft ? margin : null,
        right: isLeft ? null : margin,
        top: isTop ? margin : null,
        bottom: isTop ? null : margin,
        child: SizedBox(
          width: size,
          height: size,
          child: CustomPaint(
            painter: _CornerPainter(
              alignment: align,
              color: colorAccent,
              thickness: thickness,
            ),
          ),
        ),
      );
    }

    return [
      corner(Alignment.topLeft),
      corner(Alignment.topRight),
      corner(Alignment.bottomLeft),
      corner(Alignment.bottomRight),
    ];
  }
}

class _CornerPainter extends CustomPainter {
  final Alignment alignment;
  final Color color;
  final double thickness;

  _CornerPainter({
    required this.alignment,
    required this.color,
    required this.thickness,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = thickness
      ..style = PaintingStyle.stroke;

    final path = Path();
    if (alignment == Alignment.topLeft) {
      path.moveTo(0, size.height);
      path.lineTo(0, 0);
      path.lineTo(size.width, 0);
    } else if (alignment == Alignment.topRight) {
      path.moveTo(0, 0);
      path.lineTo(size.width, 0);
      path.lineTo(size.width, size.height);
    } else if (alignment == Alignment.bottomLeft) {
      path.moveTo(0, 0);
      path.lineTo(0, size.height);
      path.lineTo(size.width, size.height);
    } else {
      path.moveTo(0, size.height);
      path.lineTo(size.width, size.height);
      path.lineTo(size.width, 0);
    }
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
