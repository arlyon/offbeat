// OFFBEAT FestivalRow — Single festival list item
// Grid: [68px FestArt | body]
// Body: name (17px bold) + optional LIVE/PAST badge + dates/city meta + stages/sets/genre meta
// Star button top-right of body
// Dotted bottom border

import 'package:flutter/material.dart';
import '../../data/mock_data.dart';
import '../../theme/tokens.dart';
import '../../widgets/fest_art.dart';
import '../../widgets/star_button.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/live_dot.dart';

class FestivalRow extends StatelessWidget {
  final Festival fest;
  final bool saved;
  final VoidCallback onToggleSave;
  final VoidCallback onTap;

  const FestivalRow({
    super.key,
    required this.fest,
    required this.saved,
    required this.onToggleSave,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          splashColor: Colors.transparent,
          highlightColor: colorSurface1,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Festival art tile
                FestArt(
                  hue: fest.hue,
                  width: 68,
                  height: 68,
                  label: fest.id.substring(0, 3),
                ),
                const SizedBox(width: 14),
                // Body
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // Name row + star
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(
                            child: Wrap(
                              crossAxisAlignment: WrapCrossAlignment.center,
                              spacing: 8,
                              children: [
                                Text(
                                  fest.name,
                                  style: const TextStyle(
                                    fontFamily: 'Helvetica',
                                    fontWeight: FontWeight.w700,
                                    fontSize: 17,
                                    letterSpacing: -0.02 * 17,
                                    height: 1.15,
                                    color: colorFg,
                                  ),
                                ),
                                if (fest.status == FestStatus.live)
                                  _LiveBadge(),
                                if (fest.status == FestStatus.past)
                                  _PastBadge(year: fest.year),
                              ],
                            ),
                          ),
                          StarButton(
                            starred: saved,
                            onToggle: onToggleSave,
                            size: 18,
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      // Dates + city
                      Wrap(
                        spacing: 6,
                        children: [
                          _MetaText(fest.dates.replaceAll('· 2025', '').trim()),
                          const _MetaDot(),
                          _MetaText(fest.city),
                        ],
                      ),
                      const SizedBox(height: 2),
                      // Stages / sets / genre
                      Wrap(
                        spacing: 6,
                        children: [
                          _MetaText('${fest.stages} STAGES', dim: true),
                          _MetaDot(dim: true),
                          _MetaText('${fest.sets > 0 ? fest.sets : '—'} SETS', dim: true),
                          _MetaDot(dim: true),
                          _MetaText(fest.genres.first, dim: true),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _LiveBadge extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        border: Border.all(color: colorAccent, width: 1.5),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          const LiveDot(size: 6),
          const SizedBox(width: 5),
          const Text(
            'LIVE',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 9,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.08 * 9,
              color: colorAccent,
              height: 1,
            ),
          ),
        ],
      ),
    );
  }
}

class _PastBadge extends StatelessWidget {
  final String year;
  const _PastBadge({required this.year});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        border: Border.all(color: colorHairline, width: 1),
      ),
      child: Text(
        year,
        style: const TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 9,
          letterSpacing: 0.06 * 9,
          color: colorFg4,
          height: 1,
        ),
      ),
    );
  }
}

class _MetaText extends StatelessWidget {
  final String text;
  final bool dim;
  const _MetaText(this.text, {this.dim = false});

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: dim ? 10 : 11,
        color: dim ? colorFg3 : colorFg2,
        letterSpacing: 0.08 * (dim ? 10 : 11),
        height: 1.3,
      ),
    );
  }
}

class _MetaDot extends StatelessWidget {
  final bool dim;
  const _MetaDot({this.dim = false});

  @override
  Widget build(BuildContext context) {
    return Text(
      '·',
      style: TextStyle(color: dim ? colorFg4 : colorFg4, fontSize: dim ? 10 : 11),
    );
  }
}
