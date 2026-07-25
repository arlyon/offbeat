// OFFBEAT FestivalDetailScreen
// TopNav with back chevron + "OFFBEAT // FIELD DAY"
// View mode selector between the different views (V1-V6)
// Each view rendered in the Expanded body

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/chip.dart';
import 'gantt_view.dart';
import 'day_tabs_view.dart';
import 'stage_tabs_view.dart';
import 'filter_panel.dart';
import 'clash_radar_view.dart';
import 'now_strip_view.dart';

enum FestDetailView {
  gantt,
  mySchedule,
  dayTabs,
  stageTabs,
  filters,
  clashRadar,
  nowStrip,
}

class FestivalDetailScreen extends StatefulWidget {
  final Festival festival;
  final List<Stage>? stages;
  final List<Day>? days;
  final List<FestSet>? sets;
  final bool loading;
  final DateTime now;
  final void Function(String setId)? onStar;

  const FestivalDetailScreen({
    super.key,
    required this.festival,
    required this.now,
    this.stages,
    this.days,
    this.sets,
    this.loading = false,
    this.onStar,
  });

  @override
  State<FestivalDetailScreen> createState() => _FestivalDetailScreenState();
}

class _FestivalDetailScreenState extends State<FestivalDetailScreen> {
  FestDetailView _view = FestDetailView.gantt;

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
        // View mode selector
        _ViewSelector(
          active: _view,
          onChanged: (v) => setState(() => _view = v),
        ),
        // Content
        Expanded(child: _buildView(stages, days, sets)),
      ],
    );
  }

  Widget _buildView(List<Stage> stages, List<Day> days, List<FestSet> sets) {
    switch (_view) {
      case FestDetailView.gantt:
        return GanttView(
          sets: sets,
          stages: stages,
          days: days,
          now: widget.now,
          onStar: widget.onStar,
        );
      case FestDetailView.mySchedule:
        final liked = sets.where((set) => set.starred).toList();
        if (liked.isEmpty) return const _EmptyMySchedule();
        return DayTabsView(
          sets: liked,
          stages: stages,
          days: days,
          festivalWhere: widget.festival.where,
          onStar: widget.onStar,
        );
      case FestDetailView.dayTabs:
        return DayTabsView(
          sets: sets,
          stages: stages,
          days: days,
          festivalWhere: widget.festival.where,
          onStar: widget.onStar,
        );
      case FestDetailView.stageTabs:
        return StageTabsView(
          sets: sets,
          stages: stages,
          days: days,
          onStar: widget.onStar,
        );
      case FestDetailView.filters:
        return FilterView(sets: sets, stages: stages, days: days);
      case FestDetailView.clashRadar:
        return ClashRadarView(sets: sets, stages: stages, days: days);
      case FestDetailView.nowStrip:
        return NowStripView(sets: sets, stages: stages);
    }
  }
}

class _EmptyMySchedule extends StatelessWidget {
  const _EmptyMySchedule();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Padding(
        padding: EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              '☆',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 32,
                color: colorFg4,
              ),
            ),
            SizedBox(height: 12),
            Text(
              'MY SCHEDULE IS EMPTY',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 12,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 12,
                color: colorFg2,
              ),
            ),
            SizedBox(height: 6),
            Text(
              'STAR SETS TO BUILD IT',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                letterSpacing: 0.08 * 10,
                color: colorFg4,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ViewSelector extends StatelessWidget {
  final FestDetailView active;
  final ValueChanged<FestDetailView> onChanged;

  const _ViewSelector({required this.active, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final views = [
      (FestDetailView.gantt, 'GANTT'),
      (FestDetailView.mySchedule, 'MY SCHEDULE'),
      (FestDetailView.dayTabs, 'DAYS'),
      (FestDetailView.stageTabs, 'STAGES'),
      (FestDetailView.filters, 'FILTER'),
      (FestDetailView.clashRadar, 'CLASHES'),
      (FestDetailView.nowStrip, 'NOW'),
    ];

    return DottedBorder.bottom(
      child: SizedBox(
        height: 44,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 6),
          child: Row(
            children: views.map((entry) {
              final (view, label) = entry;
              return Padding(
                padding: const EdgeInsets.only(right: 6),
                child: MonoChip(
                  label: label,
                  active: active == view,
                  onTap: () => onChanged(view),
                ),
              );
            }).toList(),
          ),
        ),
      ),
    );
  }
}
