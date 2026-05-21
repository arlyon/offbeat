// OFFBEAT FestivalDetailScreen
// TopNav with back chevron + "OFFBEAT // FIELD DAY"
// View mode selector between the different views (V1-V6)
// Each view rendered in the Expanded body

import 'package:flutter/material.dart';
import '../../data/mock_data.dart';
import '../../theme/tokens.dart';
import '../../shell/top_nav.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/chip.dart';
import 'gantt_view.dart';
import 'day_tabs_view.dart';
import 'stage_tabs_view.dart';
import 'filter_panel.dart';
import 'clash_radar_view.dart';
import 'now_strip_view.dart';

enum FestDetailView { gantt, dayTabs, stageTabs, filters, clashRadar, nowStrip }

class FestivalDetailScreen extends StatefulWidget {
  final Festival festival;
  final VoidCallback onBack;

  const FestivalDetailScreen({
    super.key,
    required this.festival,
    required this.onBack,
  });

  @override
  State<FestivalDetailScreen> createState() => _FestivalDetailScreenState();
}

class _FestivalDetailScreenState extends State<FestivalDetailScreen> {
  FestDetailView _view = FestDetailView.gantt;

  // Build the sets for this festival (using Field Day mock data)
  late final List<FestSet> _sets = buildSets();

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        // Top nav with back chevron
        TopNav(
          festivalName: widget.festival.name.toUpperCase(),
          showBack: true,
          onBack: widget.onBack,
          rightWidgets: [
            NavIconButton(icon: Icons.search),
            NavIconButton(icon: Icons.tune),
          ],
        ),
        // View mode selector
        _ViewSelector(
          active: _view,
          onChanged: (v) => setState(() => _view = v),
        ),
        // Content
        Expanded(
          child: _buildView(),
        ),
      ],
    );
  }

  Widget _buildView() {
    switch (_view) {
      case FestDetailView.gantt:
        return GanttView(
          sets: _sets,
          stages: kStages,
          days: kDays,
        );
      case FestDetailView.dayTabs:
        return DayTabsView(
          sets: _sets,
          stages: kStages,
          days: kDays,
          festivalWhere: 'Brockwell Park · London',
        );
      case FestDetailView.stageTabs:
        return StageTabsView(
          sets: _sets,
          stages: kStages,
          days: kDays,
        );
      case FestDetailView.filters:
        return FilterView(
          sets: _sets,
          stages: kStages,
        );
      case FestDetailView.clashRadar:
        return ClashRadarView(
          sets: _sets,
          stages: kStages,
        );
      case FestDetailView.nowStrip:
        return NowStripView(
          sets: _sets,
          stages: kStages,
        );
    }
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
