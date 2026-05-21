// OFFBEAT StatusBar — 28px strip
// Left: time (mono, 12px, bold)
// Right: signal dots + "OFFBEAT" + battery % (mono, 11px)

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class OffbeatStatusBar extends StatelessWidget {
  final String time;
  final String carrier;
  final String battery;

  const OffbeatStatusBar({
    super.key,
    this.time = '20:30',
    this.carrier = 'OFFBEAT',
    this.battery = '87%',
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: statusBarH,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 18),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            // Left: time
            Text(
              time,
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 12,
                fontWeight: FontWeight.w700,
                letterSpacing: trMono * 12,
                color: colorFg,
                height: 1,
              ),
            ),
            // Right: signal + carrier + battery
            Row(
              children: [
                Text(
                  '●●●',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: colorFg,
                    letterSpacing: 2,
                    height: 1,
                  ),
                ),
                const SizedBox(width: 6),
                Text(
                  carrier,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: colorFg,
                    height: 1,
                  ),
                ),
                const SizedBox(width: 6),
                Text(
                  battery,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
