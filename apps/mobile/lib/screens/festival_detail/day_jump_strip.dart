import 'package:flutter/material.dart';

import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class DayJumpStrip extends StatelessWidget {
  final String activeDayId;
  final List<Day> days;
  final ValueChanged<String> onDayTap;
  final bool showDayPicker;

  const DayJumpStrip({
    super.key,
    required this.activeDayId,
    required this.days,
    required this.onDayTap,
    this.showDayPicker = true,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: 44,
        child: showDayPicker
            ? ListView.separated(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 14),
                itemCount: days.length,
                separatorBuilder: (_, _) => const SizedBox(width: 4),
                itemBuilder: (context, index) {
                  final day = days[index];
                  final active = day.id == activeDayId;
                  return Semantics(
                    button: true,
                    selected: active,
                    label: '${day.label} ${day.dayNum}',
                    child: InkWell(
                      onTap: () => onDayTap(day.id),
                      child: Center(
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 10,
                            vertical: 6,
                          ),
                          decoration: BoxDecoration(
                            color: active ? colorFg : Colors.transparent,
                            border: Border.all(
                              color: active ? colorFg : colorDotted,
                              width: 1.5,
                            ),
                          ),
                          child: Text(
                            '${day.label} ${day.dayNum}',
                            style: TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 10,
                              fontWeight: FontWeight.w700,
                              letterSpacing: 0.08 * 10,
                              color: active ? colorBg : colorFg2,
                              height: 1,
                            ),
                          ),
                        ),
                      ),
                    ),
                  );
                },
              )
            : const SizedBox.shrink(),
      ),
    );
  }
}
