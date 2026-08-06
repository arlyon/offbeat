import 'package:flutter/material.dart';

import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import 'filter_panel.dart';

class LineupSearchScreen extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final ValueChanged<FestSet> onSetTap;

  const LineupSearchScreen({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    required this.onSetTap,
  });

  @override
  State<LineupSearchScreen> createState() => _LineupSearchScreenState();
}

class _LineupSearchScreenState extends State<LineupSearchScreen> {
  final _queryController = TextEditingController();
  FilterState _filter = const FilterState();
  bool _filterOpen = false;

  Map<String, Stage> get _stageById => {
    for (final stage in widget.stages) stage.id: stage,
  };

  Map<String, Day> get _dayById => {for (final day in widget.days) day.id: day};

  Map<String, int> get _dayOrder => {
    for (var i = 0; i < widget.days.length; i++) widget.days[i].id: i,
  };

  bool get _timeFiltered =>
      _filter.timeRange[0] != 18 * 60 || _filter.timeRange[1] != 26 * 60;

  List<FestSet> get _results {
    final query = _queryController.text.trim().toLowerCase();
    final filter = _filter;
    final stages = _stageById;
    final dayOrder = _dayOrder;

    final results = widget.sets.where((set) {
      final stage = stages[set.stage];
      final haystack = '${set.artist} ${set.genre} ${stage?.name ?? ''}'.toLowerCase();
      if (query.isNotEmpty && !haystack.contains(query)) return false;
      if (filter.genres.isNotEmpty && !filter.genres.contains(set.genre)) {
        return false;
      }
      if (filter.stages.isNotEmpty && !filter.stages.contains(set.stage)) {
        return false;
      }
      if (_timeFiltered &&
          (set.t < filter.timeRange[0] ||
              set.t + set.dur > filter.timeRange[1] + 30)) {
        return false;
      }
      if (filter.scope == LineupScope.mine && !set.starred) return false;
      if (filter.scope == LineupScope.ours &&
          !set.starred &&
          !set.likedByGroup) {
        return false;
      }
      if (filter.hideClashes && set.clashes.isNotEmpty) return false;
      return true;
    }).toList();

    results.sort((a, b) {
      final byDay = (dayOrder[a.day] ?? 1 << 20).compareTo(
        dayOrder[b.day] ?? 1 << 20,
      );
      if (byDay != 0) return byDay;
      final byTime = a.t.compareTo(b.t);
      if (byTime != 0) return byTime;
      return a.artist.toLowerCase().compareTo(b.artist.toLowerCase());
    });
    return results;
  }

  bool get _hasCriteria =>
      _queryController.text.trim().isNotEmpty || _filter.totalActive > 0;

  @override
  void dispose() {
    _queryController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final results = _results;
    return Scaffold(
      backgroundColor: colorBg,
      body: SafeArea(
        child: Stack(
          children: [
            Column(
              children: [
                _SearchHeader(
                  controller: _queryController,
                  filterCount: _filter.totalActive,
                  onChanged: (_) => setState(() {}),
                  onBack: () => Navigator.pop(context),
                  onFilter: () => setState(() => _filterOpen = true),
                ),
                if (_filter.totalActive > 0)
                  _ActiveFilterBar(
                    filter: _filter,
                    onClear: () => setState(() => _filter = _filter.cleared),
                  ),
                Expanded(
                  child: !_hasCriteria
                      ? const _SearchPrompt()
                      : results.isEmpty
                      ? const _NoResults()
                      : ListView.builder(
                          padding: const EdgeInsets.only(bottom: 24),
                          itemCount: results.length + 1,
                          itemBuilder: (context, index) {
                            if (index == 0) {
                              return Padding(
                                padding: const EdgeInsets.fromLTRB(18, 12, 18, 8),
                                child: Text(
                                  '${results.length} MATCH${results.length == 1 ? '' : 'ES'}',
                                  style: _metaStyle,
                                ),
                              );
                            }
                            final set = results[index - 1];
                            final stage = _stageById[set.stage];
                            final day = _dayById[set.day];
                            if (stage == null || day == null) {
                              return const SizedBox.shrink();
                            }
                            return _SearchResult(
                              set: set,
                              stage: stage,
                              day: day,
                              onTap: () => widget.onSetTap(set),
                            );
                          },
                        ),
                ),
              ],
            ),
            if (_filterOpen)
              LineupFilterPanel(
                filter: _filter,
                stages: widget.stages,
                genres: widget.sets
                    .map((set) => set.genre)
                    .where((genre) => genre.trim().isNotEmpty)
                    .toSet()
                    .toList()
                  ..sort(),
                filteredCount: results.length,
                onFilterChanged: (filter) => setState(() => _filter = filter),
                onClose: () => setState(() => _filterOpen = false),
              ),
          ],
        ),
      ),
    );
  }
}

class _SearchHeader extends StatelessWidget {
  final TextEditingController controller;
  final int filterCount;
  final ValueChanged<String> onChanged;
  final VoidCallback onBack;
  final VoidCallback onFilter;

  const _SearchHeader({
    required this.controller,
    required this.filterCount,
    required this.onChanged,
    required this.onBack,
    required this.onFilter,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: 58,
        child: Row(
          children: [
            _HeaderButton(
              semanticLabel: 'Close lineup search',
              icon: Icons.arrow_back,
              onTap: onBack,
            ),
            Expanded(
              child: TextField(
                controller: controller,
                autofocus: true,
                onChanged: onChanged,
                textInputAction: TextInputAction.search,
                style: const TextStyle(
                  fontFamily: 'Helvetica',
                  fontSize: 16,
                  fontWeight: FontWeight.w700,
                  color: colorFg,
                ),
                decoration: const InputDecoration(
                  hintText: 'ARTIST, STAGE OR GENRE',
                  hintStyle: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.06 * 10,
                    color: colorFg4,
                  ),
                  border: InputBorder.none,
                ),
              ),
            ),
            Stack(
              clipBehavior: Clip.none,
              children: [
                _HeaderButton(
                  semanticLabel: 'Open lineup filters',
                  icon: Icons.tune,
                  onTap: onFilter,
                ),
                if (filterCount > 0)
                  Positioned(
                    top: 7,
                    right: 6,
                    child: Container(
                      width: 16,
                      height: 16,
                      color: colorAccent,
                      alignment: Alignment.center,
                      child: Text(
                        '$filterCount',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 8,
                          fontWeight: FontWeight.w700,
                          color: colorAccentInk,
                        ),
                      ),
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

class _HeaderButton extends StatelessWidget {
  final String semanticLabel;
  final IconData icon;
  final VoidCallback onTap;

  const _HeaderButton({
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
          width: 52,
          height: 58,
          child: Icon(icon, size: 20, color: colorFg2),
        ),
      ),
    );
  }
}

class _ActiveFilterBar extends StatelessWidget {
  final FilterState filter;
  final VoidCallback onClear;

  const _ActiveFilterBar({required this.filter, required this.onClear});

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: 40,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 18),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  '${filter.totalActive} ACTIVE FILTER${filter.totalActive == 1 ? '' : 'S'}',
                  style: _metaStyle,
                ),
              ),
              Semantics(
                button: true,
                label: 'Clear all lineup filters',
                child: InkWell(
                  onTap: onClear,
                  child: const SizedBox(
                    height: 40,
                    child: Center(
                      child: Text(
                        'CLEAR',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          color: colorAccent,
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
    );
  }
}

class _SearchResult extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final Day day;
  final VoidCallback onTap;

  const _SearchResult({
    required this.set,
    required this.stage,
    required this.day,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 68),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
              child: Row(
                children: [
                  Container(width: 10, height: 10, color: Color(stage.color)),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            if (set.starred)
                              const Padding(
                                padding: EdgeInsets.only(right: 6),
                                child: Icon(Icons.star, size: 13, color: colorAccent),
                              ),
                            Expanded(
                              child: Text(
                                set.artist,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontFamily: 'Helvetica',
                                  fontSize: 16,
                                  fontWeight: FontWeight.w700,
                                  color: colorFg,
                                ),
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 4),
                        Text(
                          '${day.label.toUpperCase()} ${day.dayNum} · ${fmtTime(set.t)} · '
                          '${stage.name.toUpperCase()} · ${set.genre.toUpperCase()}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: _metaStyle,
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 10),
                  const Icon(Icons.chevron_right, size: 18, color: colorFg4),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _SearchPrompt extends StatelessWidget {
  const _SearchPrompt();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Padding(
        padding: EdgeInsets.all(28),
        child: Text(
          'SEARCH THE OFFLINE LINEUP\nOR OPEN FILTERS',
          textAlign: TextAlign.center,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 11,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.08 * 11,
            color: colorFg3,
            height: 1.5,
          ),
        ),
      ),
    );
  }
}

class _NoResults extends StatelessWidget {
  const _NoResults();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Text(
        'NO MATCHES // TRY FEWER FILTERS',
        style: TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 11,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.08 * 11,
          color: colorFg3,
        ),
      ),
    );
  }
}

const _metaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.08 * 9,
  color: colorFg3,
);
