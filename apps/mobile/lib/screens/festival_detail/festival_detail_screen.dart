// OFFBEAT FestivalDetailScreen
// TopNav with back chevron + "OFFBEAT // FIELD DAY"
// View mode selector between the different views (V1-V6)
// Each view rendered in the Expanded body

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import 'clash_radar_view.dart';
import 'gantt_view.dart';
import 'set_details_sheet.dart';
import 'stage_tabs_view.dart';

enum FestDetailView { gantt, stageTabs, clashRadar }

class FestivalDetailScreen extends StatefulWidget {
  final Festival festival;
  final List<Stage>? stages;
  final List<Day>? days;
  final List<FestSet>? sets;
  final bool loading;
  final DateTime now;
  final FestDetailView initialView;
  final ValueChanged<FestDetailView>? onViewChanged;
  final void Function(String setId)? onStar;
  final void Function(Stage stage)? onStageChat;

  const FestivalDetailScreen({
    super.key,
    required this.festival,
    required this.now,
    this.initialView = FestDetailView.gantt,
    this.onViewChanged,
    this.stages,
    this.days,
    this.sets,
    this.loading = false,
    this.onStar,
    this.onStageChat,
  });

  @override
  State<FestivalDetailScreen> createState() => _FestivalDetailScreenState();
}

class _FestivalDetailScreenState extends State<FestivalDetailScreen> {
  late FestDetailView _view;

  @override
  void initState() {
    super.initState();
    _view = widget.initialView;
  }

  @override
  void didUpdateWidget(FestivalDetailScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.festival.id != widget.festival.id) {
      _view = widget.initialView;
    }
  }

  void _changeView(FestDetailView view) {
    setState(() => _view = view);
    widget.onViewChanged?.call(view);
  }

  @override
  Widget build(BuildContext context) {
    if (widget.loading) {
      return const Center(
        child: CircularProgressIndicator(
          color: Color(0xFFFF2D8F),
          strokeWidth: 1.5,
        ),
      );
    }

    final stages = widget.stages;
    final days = widget.days;
    final sets = widget.sets;

    if (stages == null ||
        days == null ||
        sets == null ||
        stages.isEmpty ||
        days.isEmpty ||
        sets.isEmpty) {
      debugPrint(
        '[FestivalDetail] guard tripped: '
        'stages=${stages?.length}, days=${days?.length}, sets=${sets?.length}',
      );
      return const Center(
        child: Text(
          'NO LINEUP DATA',
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 11,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.1 * 11,
            color: Color(0xFF555555),
            height: 1,
          ),
        ),
      );
    }

    return Column(
      children: [
        _ViewSelector(active: _view, onChanged: _changeView),
        Expanded(child: _buildView(stages, days, sets)),
      ],
    );
  }

  void _openSetDetails(FestSet set, List<Stage> stages, List<Day> days) {
    showSetDetailsSheet(
      context,
      set: set,
      stages: stages,
      days: days,
      allSets: widget.sets ?? const [],
      onStar: widget.onStar,
      onStageChat: widget.onStageChat,
    );
  }

  Widget _buildView(List<Stage> stages, List<Day> days, List<FestSet> sets) {
    void onSetTap(FestSet set) => _openSetDetails(set, stages, days);

    switch (_view) {
      case FestDetailView.gantt:
        return GanttView(
          sets: sets,
          stages: stages,
          days: days,
          now: widget.now,
          onStar: widget.onStar,
          onSetTap: onSetTap,
        );
      case FestDetailView.stageTabs:
        return StageTabsView(
          sets: sets,
          stages: stages,
          days: days,
          onStar: widget.onStar,
          onStageChat: widget.onStageChat,
          onSetTap: onSetTap,
        );
      case FestDetailView.clashRadar:
        return ClashRadarView(
          sets: sets,
          stages: stages,
          days: days,
          onSetTap: onSetTap,
        );
    }
  }
}

class _ViewSelector extends StatelessWidget {
  final FestDetailView active;
  final ValueChanged<FestDetailView> onChanged;

  const _ViewSelector({required this.active, required this.onChanged});

  static const views = [
    (FestDetailView.gantt, 'DAYS'),
    (FestDetailView.stageTabs, 'STAGES'),
    (FestDetailView.clashRadar, 'LIKED'),
  ];

  void _step(int delta) {
    final index = views.indexWhere((entry) => entry.$1 == active);
    final next = (index + delta) % views.length;
    onChanged(views[next].$1);
  }

  @override
  Widget build(BuildContext context) {
    final label = views.firstWhere((entry) => entry.$1 == active).$2;
    return DottedBorder.bottom(
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onHorizontalDragEnd: (details) {
          final velocity = details.primaryVelocity ?? 0;
          if (velocity.abs() < 150) return;
          _step(velocity < 0 ? 1 : -1);
        },
        child: SizedBox(
          height: 44,
          child: Row(
            children: [
              _ViewArrow(
                semanticLabel: 'Previous schedule view',
                icon: Icons.chevron_left,
                onTap: () => _step(-1),
              ),
              Expanded(
                child: Center(
                  child: AnimatedSwitcher(
                    duration: const Duration(milliseconds: 160),
                    child: Text(
                      label,
                      key: ValueKey(label),
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.1 * 11,
                        color: colorFg,
                      ),
                    ),
                  ),
                ),
              ),
              _ViewArrow(
                semanticLabel: 'Next schedule view',
                icon: Icons.chevron_right,
                onTap: () => _step(1),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ViewArrow extends StatelessWidget {
  final String semanticLabel;
  final IconData icon;
  final VoidCallback onTap;

  const _ViewArrow({
    required this.semanticLabel,
    required this.icon,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: semanticLabel,
      child: InkWell(
        onTap: onTap,
        child: SizedBox(
          width: 56,
          height: 44,
          child: Icon(icon, size: 22, color: colorFg2),
        ),
      ),
    );
  }
}
