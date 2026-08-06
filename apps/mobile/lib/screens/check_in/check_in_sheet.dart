import 'package:flutter/material.dart';

import '../../data/check_in_controller.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

void showCheckInSheet(
  BuildContext context, {
  required CheckInController controller,
  required List<Stage> stages,
  required List<FestSet> sets,
}) {
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    builder: (_) =>
        _CheckInSheet(controller: controller, stages: stages, sets: sets),
  );
}

class _CheckInSheet extends StatefulWidget {
  final CheckInController controller;
  final List<Stage> stages;
  final List<FestSet> sets;

  const _CheckInSheet({
    required this.controller,
    required this.stages,
    required this.sets,
  });

  @override
  State<_CheckInSheet> createState() => _CheckInSheetState();
}

class _CheckInSheetState extends State<_CheckInSheet> {
  final _custom = TextEditingController();

  @override
  void dispose() {
    _custom.dispose();
    super.dispose();
  }

  Future<void> _run(Future<bool> Function() action) async {
    if (await action() && mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) {
    final activeByStage = <String, FestSet>{};
    for (final set in widget.sets.where((set) => set.live && !set.cancelled)) {
      activeByStage.putIfAbsent(set.stage, () => set);
    }
    final current = widget.controller.checkIn;
    final stageOrder = {
      for (var index = 0; index < widget.stages.length; index++)
        widget.stages[index].id: index,
    };
    final orderedStages = [...widget.stages]
      ..sort((a, b) {
        int rank(Stage stage) {
          if (current?.kind == 'stage' && current?.value == stage.id) return 0;
          final set = activeByStage[stage.id];
          if (set?.starred ?? false) return 1;
          if (set != null) return 2;
          return 3;
        }

        final byRank = rank(a).compareTo(rank(b));
        return byRank != 0
            ? byRank
            : (stageOrder[a.id] ?? 0).compareTo(stageOrder[b.id] ?? 0);
      });

    return DraggableScrollableSheet(
      initialChildSize: 0.78,
      minChildSize: 0.5,
      maxChildSize: 0.94,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: SafeArea(
          top: false,
          child: Column(
            children: [
              Container(
                width: 36,
                height: 3,
                margin: const EdgeInsets.only(top: 8),
                color: colorFg4,
              ),
              DottedBorder.bottom(
                child: const SizedBox(
                  height: 58,
                  child: Padding(
                    padding: EdgeInsets.symmetric(horizontal: 18),
                    child: Row(
                      children: [
                        Expanded(
                          child: Text(
                            'CHECK IN',
                            style: TextStyle(
                              fontFamily: 'Helvetica',
                              fontSize: 24,
                              fontWeight: FontWeight.w700,
                              color: colorFg,
                            ),
                          ),
                        ),
                        Text('SHARED WITH YOUR GROUPS', style: _metaStyle),
                      ],
                    ),
                  ),
                ),
              ),
              Expanded(
                child: ListView(
                  controller: scrollController,
                  children: [
                    _LocationRow(
                      label: 'CAMPSITE',
                      meta: current?.kind == 'campsite'
                          ? 'CURRENT'
                          : 'ALWAYS AVAILABLE',
                      icon: Icons.cabin_outlined,
                      active: current?.kind == 'campsite',
                      onTap: () => _run(widget.controller.setCampsite),
                    ),
                    for (final stage in orderedStages)
                      _LocationRow(
                        label: stage.name.toUpperCase(),
                        meta: _stageMeta(
                          stage,
                          activeByStage[stage.id],
                          current?.kind == 'stage' &&
                              current?.value == stage.id,
                        ),
                        icon: Icons.location_on_outlined,
                        active:
                            current?.kind == 'stage' &&
                            current?.value == stage.id,
                        starred: activeByStage[stage.id]?.starred ?? false,
                        onTap: () =>
                            _run(() => widget.controller.setStage(stage.id)),
                      ),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(18, 18, 18, 8),
                      child: TextField(
                        controller: _custom,
                        maxLength: 80,
                        decoration: const InputDecoration(
                          labelText: 'CUSTOM LOCATION',
                          hintText: 'FOOD HALL, NORTH GATE…',
                        ),
                        onSubmitted: (value) {
                          if (value.trim().isNotEmpty) {
                            _run(
                              () => widget.controller.setCustom(value.trim()),
                            );
                          }
                        },
                      ),
                    ),
                    if (current != null)
                      _LocationRow(
                        label: 'STOP SHARING',
                        meta: 'CLEAR CHECK-IN',
                        icon: Icons.location_off_outlined,
                        destructive: true,
                        onTap: () => _run(widget.controller.clear),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _stageMeta(Stage stage, FestSet? set, bool current) {
    if (current) return 'CURRENT';
    if (set == null) return 'NO SET ON NOW';
    final prefix = set.starred ? '★ YOUR SET · ' : 'ON NOW · ';
    return '$prefix${set.artist.toUpperCase()} · UNTIL ${fmtTime(set.t + set.dur)}';
  }
}

class _LocationRow extends StatelessWidget {
  final String label;
  final String meta;
  final IconData icon;
  final bool active;
  final bool starred;
  final bool destructive;
  final VoidCallback onTap;

  const _LocationRow({
    required this.label,
    required this.meta,
    required this.icon,
    required this.onTap,
    this.active = false,
    this.starred = false,
    this.destructive = false,
  });

  @override
  Widget build(BuildContext context) {
    final color = destructive
        ? colorErr
        : active
        ? colorAccent
        : starred
        ? colorAccent
        : colorFg;
    return DottedBorder.bottom(
      child: Material(
        color: active ? colorAccentWash : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 58),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 8),
              child: Row(
                children: [
                  Icon(icon, size: 18, color: color),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          label,
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            fontWeight: FontWeight.w700,
                            color: color,
                          ),
                        ),
                        const SizedBox(height: 3),
                        Text(
                          meta,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: _metaStyle,
                        ),
                      ],
                    ),
                  ),
                  if (active)
                    const Icon(Icons.check, size: 18, color: colorAccent),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

const _metaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.05 * 9,
  color: colorFg3,
);
