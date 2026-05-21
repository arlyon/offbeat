// OFFBEAT Mark — 3-bar equalizer logo
// 3 bars: widths 3px each, gap 2px, heights 9/14/6px
// First two bars: fg color, third bar: accent color
// Aligned to bottom

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class OffbeatMark extends StatelessWidget {
  const OffbeatMark({super.key});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 16,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          _Bar(height: 9, color: colorFg),
          const SizedBox(width: 2),
          _Bar(height: 14, color: colorFg),
          const SizedBox(width: 2),
          _Bar(height: 6, color: colorAccent),
        ],
      ),
    );
  }
}

class _Bar extends StatelessWidget {
  final double height;
  final Color color;
  const _Bar({required this.height, required this.color});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 3,
      height: height,
      color: color,
    );
  }
}
