// OFFBEAT MonoChip — Mono chip widget
// Font: mono, uppercase, 10px, letter-spacing 0.08em
// Inactive: dotted border, fg2 text
// Active: accent bg, accentInk text, solid border

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import 'dotted_border.dart';

class MonoChip extends StatelessWidget {
  final String label;
  final bool active;
  final VoidCallback? onTap;
  final Widget? prefix;

  const MonoChip({
    super.key,
    required this.label,
    this.active = false,
    this.onTap,
    this.prefix,
  });

  @override
  Widget build(BuildContext context) {
    final Widget content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (prefix != null) ...[
            prefix!,
            const SizedBox(width: 6),
          ],
          Text(
            label.toUpperCase(),
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              fontWeight: FontWeight.w500,
              letterSpacing: trMeta * 10,
              color: active ? colorAccentInk : colorFg2,
              height: 1,
            ),
          ),
        ],
      ),
    );

    if (active) {
      return GestureDetector(
        onTap: onTap,
        child: Container(
          decoration: const BoxDecoration(
            color: colorAccent,
            border: Border.fromBorderSide(
              BorderSide(color: colorAccent, width: bdDotWidth),
            ),
          ),
          child: content,
        ),
      );
    }

    return GestureDetector(
      onTap: onTap,
      child: DottedBorder(
        child: content,
      ),
    );
  }
}
