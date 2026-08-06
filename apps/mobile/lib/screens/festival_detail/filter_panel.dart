// OFFBEAT FilterPanel — V4 Filter bottom sheet
// Slides up from bottom, absolutely positioned
// Grip handle, "Filters" title, "CLEAR ALL" button
// Stage grid: 2-column, colored swatches 14×14px, checkbox indicator (●/○)
// Time range: visual track (dotted) + accent fill + handles (14×14px accent squares)
// Genre chips: wrap row, dotted inactive, accent active
// Smart toggles: "★ Starred only" + "× Hide clashing sets" with sliding switch
// Footer: RESET ghost button + "SHOW X SETS →" primary button
// Filter summary bar above the list when active

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/chip.dart';
import 'day_tabs_view.dart';

enum LineupScope { all, mine, ours }

class FilterState {
  final Set<String> genres;
  final Set<String> stages;
  final List<int> timeRange; // [startMin, endMin]
  final LineupScope scope;
  final bool hideClashes;

  const FilterState({
    this.genres = const {},
    this.stages = const {},
    this.timeRange = const [18 * 60, 26 * 60],
    this.scope = LineupScope.all,
    this.hideClashes = false,
  });

  FilterState copyWith({
    Set<String>? genres,
    Set<String>? stages,
    List<int>? timeRange,
    LineupScope? scope,
    bool? hideClashes,
  }) => FilterState(
    genres: genres ?? this.genres,
    stages: stages ?? this.stages,
    timeRange: timeRange ?? this.timeRange,
    scope: scope ?? this.scope,
    hideClashes: hideClashes ?? this.hideClashes,
  );

  int get totalActive =>
      genres.length +
      stages.length +
      (timeRange[0] != 18 * 60 || timeRange[1] != 26 * 60 ? 1 : 0) +
      (scope == LineupScope.all ? 0 : 1) +
      (hideClashes ? 1 : 0);

  FilterState get cleared => const FilterState();
}

class FilterView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;

  const FilterView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
  });

  @override
  State<FilterView> createState() => _FilterViewState();
}

class _FilterViewState extends State<FilterView> {
  FilterState _filter = const FilterState();
  bool _panelOpen = true;
  late String _day;

  @override
  void initState() {
    super.initState();
    _day = widget.days.first.id;
  }

  Map<String, Stage> get _stageById => {for (final s in widget.stages) s.id: s};

  List<FestSet> get _filtered {
    final f = _filter;
    return widget.sets.where((s) {
      if (s.day != _day) return false;
      if (f.genres.isNotEmpty && !f.genres.contains(s.genre)) return false;
      if (f.stages.isNotEmpty && !f.stages.contains(s.stage)) return false;
      if (s.t < f.timeRange[0]) return false;
      if (s.t + s.dur > f.timeRange[1] + 30) return false;
      if (f.scope == LineupScope.mine && !s.starred) return false;
      if (f.scope == LineupScope.ours && !s.starred && !s.likedByGroup) {
        return false;
      }
      if (f.hideClashes && s.clashes.isNotEmpty) return false;
      return true;
    }).toList()..sort((a, b) => a.t.compareTo(b.t));
  }

  Set<String> _toggle(Set<String> set, String val) {
    final n = Set<String>.from(set);
    if (n.contains(val)) {
      n.remove(val);
    } else {
      n.add(val);
    }
    return n;
  }

  @override
  Widget build(BuildContext context) {
    final stageById = _stageById;
    final filtered = _filtered;
    final f = _filter;

    return Stack(
      children: [
        // Content behind the panel
        Column(
          children: [
            // Filter summary bar
            _FilterSummaryBar(
              filter: f,
              stageById: stageById,
              onOpenPanel: () => setState(() => _panelOpen = true),
              onRemoveStage: (sid) => setState(() {
                _filter = _filter.copyWith(stages: _toggle(f.stages, sid));
              }),
              onRemoveGenre: (g) => setState(() {
                _filter = _filter.copyWith(genres: _toggle(f.genres, g));
              }),
              onRemoveHideClashes: () => setState(() {
                _filter = _filter.copyWith(hideClashes: false);
              }),
            ),
            // Set list
            Expanded(
              child: ListView(
                padding: EdgeInsets.zero,
                children: [
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 18,
                      vertical: 10,
                    ),
                    child: Text(
                      '${filtered.length} SETS // ${widget.days.firstWhere((d) => d.id == _day).label} ${widget.days.firstWhere((d) => d.id == _day).dayNum}',
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        color: colorFg3,
                        letterSpacing: 0.08 * 10,
                        height: 1,
                      ),
                    ),
                  ),
                  ...filtered.map(
                    (s) => SetRow(set: s, stage: stageById[s.stage]!),
                  ),
                  if (filtered.isEmpty)
                    const Padding(
                      padding: EdgeInsets.all(30),
                      child: Center(
                        child: Text(
                          'NO SETS // ADJUST FILTERS',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            color: colorFg3,
                            letterSpacing: 0.08 * 11,
                            height: 1,
                          ),
                        ),
                      ),
                    ),
                  const SizedBox(height: 60),
                ],
              ),
            ),
          ],
        ),
        // Filter sheet
        if (_panelOpen)
          LineupFilterPanel(
            filter: f,
            stages: widget.stages,
            genres: widget.sets.map((s) => s.genre).toSet().toList()..sort(),
            filteredCount: filtered.length,
            onFilterChanged: (newF) => setState(() => _filter = newF),
            onClose: () => setState(() => _panelOpen = false),
          ),
      ],
    );
  }
}

class _FilterSummaryBar extends StatelessWidget {
  final FilterState filter;
  final Map<String, Stage> stageById;
  final VoidCallback onOpenPanel;
  final void Function(String) onRemoveStage;
  final void Function(String) onRemoveGenre;
  final VoidCallback onRemoveHideClashes;

  const _FilterSummaryBar({
    required this.filter,
    required this.stageById,
    required this.onOpenPanel,
    required this.onRemoveStage,
    required this.onRemoveGenre,
    required this.onRemoveHideClashes,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: 44,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 14),
          child: Row(
            children: [
              // Open button
              GestureDetector(
                onTap: onOpenPanel,
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  color: colorFg,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(Icons.tune, size: 14, color: colorBg),
                      const SizedBox(width: 8),
                      const Text(
                        'Filters',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 11,
                          fontWeight: FontWeight.w700,
                          color: colorBg,
                          letterSpacing: 0.08 * 11,
                          height: 1,
                        ),
                      ),
                      if (filter.totalActive > 0) ...[
                        const SizedBox(width: 8),
                        Container(
                          width: 18,
                          height: 18,
                          color: colorAccent,
                          child: Center(
                            child: Text(
                              '${filter.totalActive}',
                              style: const TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 10,
                                color: colorAccentInk,
                                height: 1,
                              ),
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
              // Active stage chips
              ...filter.stages.map((sid) {
                final stage = stageById[sid];
                return Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: MonoChip(
                    label: '${stage?.name ?? sid} ×',
                    active: true,
                    onTap: () => onRemoveStage(sid),
                    prefix: Container(
                      width: 8,
                      height: 8,
                      color: Color(stage?.color ?? 0xFFFF2D8F),
                    ),
                  ),
                );
              }),
              // Active genre chips
              ...filter.genres.map(
                (g) => Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: MonoChip(
                    label: '$g ×',
                    active: true,
                    onTap: () => onRemoveGenre(g),
                  ),
                ),
              ),
              if (filter.hideClashes)
                Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: MonoChip(
                    label: 'NO CLASHES ×',
                    active: true,
                    onTap: onRemoveHideClashes,
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class LineupFilterPanel extends StatelessWidget {
  final FilterState filter;
  final List<Stage> stages;
  final List<String> genres;
  final int filteredCount;
  final ValueChanged<FilterState> onFilterChanged;
  final VoidCallback onClose;

  const LineupFilterPanel({
    super.key,
    required this.filter,
    required this.stages,
    required this.genres,
    required this.filteredCount,
    required this.onFilterChanged,
    required this.onClose,
  });

  Set<String> _toggle(Set<String> set, String val) {
    final n = Set<String>.from(set);
    if (n.contains(val)) {
      n.remove(val);
    } else {
      n.add(val);
    }
    return n;
  }

  @override
  Widget build(BuildContext context) {
    return Positioned.fill(
      top: 0,
      child: Container(
        color: colorSurface1,
        child: Column(
          children: [
            // Grip
            Container(
              margin: const EdgeInsets.symmetric(vertical: 8),
              width: 36,
              height: 3,
              color: colorFg4,
            ),
            // Header
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 0, 18, 12),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text(
                    'Filters',
                    style: TextStyle(
                      fontFamily: 'Helvetica',
                      fontWeight: FontWeight.w700,
                      fontSize: 22,
                      letterSpacing: -0.02 * 22,
                      color: colorFg,
                      height: 1,
                    ),
                  ),
                  GestureDetector(
                    onTap: () => onFilterChanged(filter.cleared),
                    child: Text(
                      'CLEAR ALL (${filter.totalActive})',
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.1 * 10,
                        color: colorAccent,
                        height: 1,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            // Body
            Expanded(
              child: SingleChildScrollView(
                child: Column(
                  children: [
                    // Stages section
                    _FpSection(
                      label: '// STAGES',
                      value: filter.stages.isEmpty
                          ? 'ALL'
                          : '${filter.stages.length}',
                      child: _StageGrid(
                        stages: stages,
                        activeStages: filter.stages,
                        onToggle: (sid) => onFilterChanged(
                          filter.copyWith(stages: _toggle(filter.stages, sid)),
                        ),
                      ),
                    ),
                    // Time range section
                    _FpSection(
                      label: '// TIME WINDOW',
                      value:
                          '${fmtTime(filter.timeRange[0])} → ${fmtTime(filter.timeRange[1])}',
                      child: _TimeRangeSlider(
                        timeRange: filter.timeRange,
                        onChanged: (r) =>
                            onFilterChanged(filter.copyWith(timeRange: r)),
                      ),
                    ),
                    // Genres section
                    _FpSection(
                      label: '// GENRES',
                      value: filter.genres.isEmpty
                          ? 'ALL'
                          : '${filter.genres.length}',
                      child: Wrap(
                        spacing: 6,
                        runSpacing: 6,
                        children: genres
                            .map(
                              (g) => MonoChip(
                                label: g,
                                active: filter.genres.contains(g),
                                onTap: () => onFilterChanged(
                                  filter.copyWith(
                                    genres: _toggle(filter.genres, g),
                                  ),
                                ),
                              ),
                            )
                            .toList(),
                      ),
                    ),
                    _FpSection(
                      label: '// PICKS',
                      value: filter.scope.name.toUpperCase(),
                      child: Row(
                        children: [
                          for (final scope in LineupScope.values) ...[
                            Expanded(
                              child: MonoChip(
                                label: scope.name.toUpperCase(),
                                active: filter.scope == scope,
                                onTap: () => onFilterChanged(
                                  filter.copyWith(scope: scope),
                                ),
                              ),
                            ),
                            if (scope != LineupScope.values.last)
                              const SizedBox(width: 6),
                          ],
                        ],
                      ),
                    ),
                    _FpSection(
                      label: '// SMART FILTERS',
                      child: _ToggleRow(
                        label: '× Hide clashing sets',
                        sublabel: 'Skip overlaps with your stars',
                        value: filter.hideClashes,
                        onToggle: () => onFilterChanged(
                          filter.copyWith(hideClashes: !filter.hideClashes),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
            // Footer
            DottedBorder.top(
              child: Padding(
                padding: const EdgeInsets.all(14),
                child: Row(
                  children: [
                    Expanded(
                      flex: 1,
                      child: GestureDetector(
                        onTap: () => onFilterChanged(filter.cleared),
                        child: Container(
                          padding: const EdgeInsets.all(12),
                          decoration: BoxDecoration(
                            border: Border.all(color: colorFg3, width: 1.5),
                          ),
                          child: const Center(
                            child: Text(
                              'RESET',
                              style: TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 11,
                                fontWeight: FontWeight.w500,
                                letterSpacing: 0.08 * 11,
                                color: colorFg,
                                height: 1,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      flex: 2,
                      child: GestureDetector(
                        onTap: onClose,
                        child: Container(
                          padding: const EdgeInsets.all(12),
                          color: colorAccent,
                          child: Center(
                            child: Text(
                              'SHOW $filteredCount SETS →',
                              style: const TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 11,
                                fontWeight: FontWeight.w500,
                                letterSpacing: 0.08 * 11,
                                color: colorAccentInk,
                                height: 1,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _FpSection extends StatelessWidget {
  final String label;
  final String? value;
  final Widget child;

  const _FpSection({required this.label, this.value, required this.child});

  @override
  Widget build(BuildContext context) {
    return DottedBorder.top(
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  label,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.08 * 11,
                    color: colorFg,
                    height: 1,
                  ),
                ),
                if (value != null)
                  Text(
                    value!,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      color: colorAccent,
                      height: 1,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 10),
            child,
          ],
        ),
      ),
    );
  }
}

class _StageGrid extends StatelessWidget {
  final List<Stage> stages;
  final Set<String> activeStages;
  final void Function(String) onToggle;

  const _StageGrid({
    required this.stages,
    required this.activeStages,
    required this.onToggle,
  });

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: stages.map((s) {
        final isOn = activeStages.contains(s.id);
        return GestureDetector(
          onTap: () => onToggle(s.id),
          child: Container(
            width: (MediaQuery.of(context).size.width - 18 * 2 - 8) / 2,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(
              color: isOn ? colorSurface2 : Colors.transparent,
              border: Border.all(
                color: isOn ? colorFg : colorDotted,
                width: 1.5,
                style: isOn ? BorderStyle.solid : BorderStyle.solid,
              ),
            ),
            child: Row(
              children: [
                Container(width: 14, height: 14, color: Color(s.color)),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    s.name,
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.06 * 11,
                      color: isOn ? colorFg : colorFg2,
                      height: 1,
                    ),
                  ),
                ),
                Text(
                  isOn ? '●' : '○',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: isOn ? colorAccent : colorFg4,
                    height: 1,
                  ),
                ),
              ],
            ),
          ),
        );
      }).toList(),
    );
  }
}

class _TimeRangeSlider extends StatelessWidget {
  final List<int> timeRange;
  final ValueChanged<List<int>> onChanged;

  const _TimeRangeSlider({required this.timeRange, required this.onChanged});

  static const int _min = 18 * 60;
  static const int _max = 26 * 60;

  double _toPct(int val) => (val - _min) / (_max - _min);

  @override
  Widget build(BuildContext context) {
    final pctL = _toPct(timeRange[0]);
    final pctR = _toPct(timeRange[1]);

    return Column(
      children: [
        SizedBox(
          height: 36,
          child: LayoutBuilder(
            builder: (context, constraints) {
              final w = constraints.maxWidth;
              final xL = pctL * w;
              final xR = pctR * w;

              return Stack(
                clipBehavior: Clip.none,
                children: [
                  // Track
                  Positioned(
                    left: 0,
                    right: 0,
                    top: 17,
                    child: const DottedRule(),
                  ),
                  // Fill
                  Positioned(
                    left: xL,
                    width: xR - xL,
                    top: 16,
                    height: 2,
                    child: Container(color: colorAccent),
                  ),
                  // Left label
                  Positioned(
                    left: xL - 14,
                    top: -16,
                    child: Text(
                      fmtTime(timeRange[0]),
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        color: colorAccent,
                        height: 1,
                      ),
                    ),
                  ),
                  // Right label
                  Positioned(
                    left: xR - 14,
                    top: -16,
                    child: Text(
                      fmtTime(timeRange[1]),
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        color: colorAccent,
                        height: 1,
                      ),
                    ),
                  ),
                  // Left handle
                  Positioned(
                    left: xL - 7,
                    top: 11,
                    child: GestureDetector(
                      onHorizontalDragUpdate: (d) {
                        final newX = (xL + d.delta.dx).clamp(0.0, w);
                        final newMin =
                            _min + ((newX / w) * (_max - _min)).round();
                        if (newMin < timeRange[1]) {
                          onChanged([newMin.clamp(_min, _max), timeRange[1]]);
                        }
                      },
                      child: Container(
                        width: 14,
                        height: 14,
                        decoration: BoxDecoration(
                          color: colorAccent,
                          border: Border.all(color: colorBg, width: 2),
                        ),
                      ),
                    ),
                  ),
                  // Right handle
                  Positioned(
                    left: xR - 7,
                    top: 11,
                    child: GestureDetector(
                      onHorizontalDragUpdate: (d) {
                        final newX = (xR + d.delta.dx).clamp(0.0, w);
                        final newMax =
                            _min + ((newX / w) * (_max - _min)).round();
                        if (newMax > timeRange[0]) {
                          onChanged([timeRange[0], newMax.clamp(_min, _max)]);
                        }
                      },
                      child: Container(
                        width: 14,
                        height: 14,
                        decoration: BoxDecoration(
                          color: colorAccent,
                          border: Border.all(color: colorBg, width: 2),
                        ),
                      ),
                    ),
                  ),
                ],
              );
            },
          ),
        ),
        const SizedBox(height: 14),
        // Time labels
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: const [
            _TimeLabel('18:00'),
            _TimeLabel('20:00'),
            _TimeLabel('22:00'),
            _TimeLabel('00:00'),
            _TimeLabel('02:00'),
          ],
        ),
      ],
    );
  }
}

class _TimeLabel extends StatelessWidget {
  final String text;
  const _TimeLabel(this.text);

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: const TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: 9,
        color: colorFg4,
        letterSpacing: 0.05 * 9,
        height: 1,
      ),
    );
  }
}

class _ToggleRow extends StatelessWidget {
  final String label;
  final String sublabel;
  final bool value;
  final VoidCallback onToggle;

  const _ToggleRow({
    required this.label,
    required this.sublabel,
    required this.value,
    required this.onToggle,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onToggle,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 10),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    label,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.06 * 12,
                      color: colorFg,
                      height: 1,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    sublabel,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      letterSpacing: 0.08 * 10,
                      color: colorFg3,
                      height: 1,
                    ),
                  ),
                ],
              ),
            ),
            // Switch
            AnimatedContainer(
              duration: const Duration(milliseconds: 140),
              width: 38,
              height: 20,
              decoration: BoxDecoration(
                color: value ? colorAccent : colorSurface3,
                border: Border.all(
                  color: value ? colorAccent : colorHairline,
                  width: 1.5,
                ),
              ),
              child: Stack(
                children: [
                  AnimatedPositioned(
                    duration: const Duration(milliseconds: 140),
                    left: value ? 38 - 16.5 : 1.5,
                    top: 1.5,
                    child: Container(
                      width: 13,
                      height: 13,
                      color: value ? colorAccentInk : colorFg2,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
