import 'package:flutter/material.dart';

import '../data/group_schedule_overlay.dart';
import '../theme/tokens.dart';
import 'dotted_border.dart';

String supporterFirstName(String displayName) {
  final trimmed = displayName.trim();
  if (trimmed.isEmpty) return 'anon';
  return trimmed.split(RegExp(r'\s+')).first;
}

String compactSupporterSummary(List<ScheduleSupporter> supporters) {
  final visible = supporters
      .take(2)
      .map((supporter) => supporterFirstName(supporter.displayName));
  final overflow = supporters.length - 2;
  return [...visible, if (overflow > 0) '+$overflow'].join(' · ');
}

void showCoLikersSheet(
  BuildContext context, {
  required String artist,
  required List<ScheduleSupporter> supporters,
}) {
  if (supporters.isEmpty) return;
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    builder: (context) => Container(
      color: colorSurface1,
      padding: const EdgeInsets.fromLTRB(18, 10, 18, 24),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(child: Container(width: 36, height: 3, color: colorFg4)),
            const SizedBox(height: 18),
            Text(
              '${supporters.length} ALSO SAVED',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 10,
                color: colorAccent,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              artist,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                fontFamily: 'Helvetica',
                fontSize: 24,
                fontWeight: FontWeight.w700,
                letterSpacing: -0.02 * 24,
                color: colorFg,
              ),
            ),
            const SizedBox(height: 14),
            DottedBorder.top(
              child: Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Column(
                  children: [
                    for (final supporter in supporters)
                      SizedBox(
                        height: 44,
                        child: Row(
                          children: [
                            const Text(
                              '★',
                              style: TextStyle(
                                fontSize: 12,
                                color: colorAccent,
                              ),
                            ),
                            const SizedBox(width: 10),
                            Expanded(
                              child: Text(
                                supporter.displayName,
                                style: const TextStyle(
                                  fontFamily: 'Helvetica',
                                  fontSize: 15,
                                  fontWeight: FontWeight.w700,
                                  color: colorFg,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

class CoLikerPins extends StatelessWidget {
  final String artist;
  final List<ScheduleSupporter> supporters;

  const CoLikerPins({
    super.key,
    required this.artist,
    required this.supporters,
  });

  @override
  Widget build(BuildContext context) {
    if (supporters.isEmpty) return const SizedBox.shrink();
    final visible = supporters.take(2).toList();
    final overflow = supporters.length - visible.length;

    return Semantics(
      button: true,
      label: '${supporters.length} people also saved $artist',
      child: InkWell(
        onTap: () =>
            showCoLikersSheet(context, artist: artist, supporters: supporters),
        splashColor: Colors.transparent,
        highlightColor: colorSurface2,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 44),
          child: Align(
            alignment: Alignment.centerLeft,
            child: Wrap(
              spacing: 4,
              runSpacing: 4,
              children: [
                for (final supporter in visible)
                  _NamePin(label: supporterFirstName(supporter.displayName)),
                if (overflow > 0) _NamePin(label: '+$overflow'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _NamePin extends StatelessWidget {
  final String label;

  const _NamePin({required this.label});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
      decoration: BoxDecoration(
        color: colorAccentWash,
        border: Border.all(color: colorAccent, width: 1),
      ),
      child: Text(
        label.toUpperCase(),
        style: const TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 8,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.04 * 8,
          color: colorAccent,
          height: 1,
        ),
      ),
    );
  }
}
