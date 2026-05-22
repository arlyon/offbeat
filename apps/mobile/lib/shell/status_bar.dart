// OFFBEAT StatusBar — 28px strip
// Left: time (mono, 12px, bold)
// Right: BLE indicator + relay indicator + "OFFBEAT" + battery % (mono, 11px)

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class OffbeatStatusBar extends StatelessWidget {
  final String time;
  final String carrier;
  final String battery;
  final bool relayConnected;
  final bool bleActive;
  final int blePeerCount;

  const OffbeatStatusBar({
    super.key,
    this.time = '20:30',
    this.carrier = 'OFFBEAT',
    this.battery = '87%',
    this.relayConnected = false,
    this.bleActive = false,
    this.blePeerCount = 0,
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
            // Right: transports + carrier + battery
            Row(
              children: [
                if (bleActive) ...[
                  Text(
                    'BLE:$blePeerCount',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      color: colorCoAccent,
                      height: 1,
                    ),
                  ),
                  const SizedBox(width: 6),
                ],
                // Relay dot: green=connected, red=disconnected
                Text(
                  '●',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: relayConnected ? colorOk : colorErr,
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
