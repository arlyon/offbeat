import 'package:flutter/material.dart';

import '../../data/check_in_controller.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import 'check_in_sheet.dart';

class CheckInBand extends StatelessWidget {
  final CheckInController controller;
  final List<Stage> stages;
  final List<FestSet> sets;
  final int groupCount;

  const CheckInBand({
    super.key,
    required this.controller,
    required this.stages,
    required this.sets,
    this.groupCount = 0,
  });

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final checkIn = controller.checkIn;
        final stageById = {for (final stage in stages) stage.id: stage};
        final label = switch (checkIn?.kind) {
          'campsite' => 'CAMPSITE',
          'stage' => stageById[checkIn?.value]?.name.toUpperCase() ?? 'STAGE',
          'custom' => checkIn?.value?.toUpperCase() ?? 'CUSTOM LOCATION',
          _ => 'NOT CHECKED IN',
        };
        final meta = controller.saving
            ? 'SAVING…'
            : checkIn == null
            ? 'CHECK IN →'
            : groupCount == 0
            ? 'SAVED LOCALLY · UPDATE →'
            : 'SHARED WITH $groupCount GROUP${groupCount == 1 ? '' : 'S'} · UPDATE →';
        return DottedBorder.bottom(
          child: Material(
            color: checkIn == null ? Colors.transparent : colorAccentWash,
            child: InkWell(
              onTap: controller.saving
                  ? null
                  : () => showCheckInSheet(
                      context,
                      controller: controller,
                      stages: stages,
                      sets: sets,
                    ),
              child: SizedBox(
                height: 58,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 18),
                  child: Row(
                    children: [
                      Icon(
                        checkIn == null
                            ? Icons.location_off_outlined
                            : Icons.location_on,
                        size: 18,
                        color: checkIn == null ? colorFg4 : colorAccent,
                      ),
                      const SizedBox(width: 11),
                      Expanded(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            const Text('YOU ARE AT', style: _metaStyle),
                            const SizedBox(height: 3),
                            Text(
                              label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 11,
                                fontWeight: FontWeight.w700,
                                color: colorFg,
                              ),
                            ),
                          ],
                        ),
                      ),
                      Text(meta, style: _actionStyle),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

const _metaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 8,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.08 * 8,
  color: colorFg4,
);

const _actionStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.05 * 9,
  color: colorAccent,
);
