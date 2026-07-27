// OFFBEAT FestivalListScreen — Variant A "Index"
// Page header: "Festivals." + subtitle meta row
// Search bar: dotted border, surface1 bg, search icon, placeholder, ⌘K badge
// "// SAVED" section with star count pill (accent bg)
// "// DISCOVER" section with festival count
// Empty state: "NO RESULTS // {query}"

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../services/festival_import_service.dart';
import '../../theme/tokens.dart';
import 'clashfinder_import_panel.dart';
import '../../widgets/dotted_border.dart';
import 'festival_row.dart';

class FestivalListScreen extends StatefulWidget {
  final void Function(Festival) onFestivalTap;
  final List<Festival> festivals;
  final bool loading;
  final String? error;
  final VoidCallback? onRefresh;
  final bool importRegistered;
  final Future<void> Function() onRegister;
  final Future<ClashfinderPreviewResult> Function(String source)
  onPreviewClashfinder;
  final Future<Festival> Function({
    required String previewId,
    required String name,
    required String location,
    required String city,
    required String country,
  })
  onPublishClashfinder;
  final Future<void> Function(Festival festival) onFestivalPublished;

  const FestivalListScreen({
    super.key,
    required this.onFestivalTap,
    required this.festivals,
    this.loading = false,
    this.error,
    this.onRefresh,
    required this.importRegistered,
    required this.onRegister,
    required this.onPreviewClashfinder,
    required this.onPublishClashfinder,
    required this.onFestivalPublished,
  });

  @override
  State<FestivalListScreen> createState() => _FestivalListScreenState();
}

class _FestivalListScreenState extends State<FestivalListScreen> {
  String _query = '';
  final _searchController = TextEditingController();
  // Initial saved state: fieldday26 and ade25
  final Set<String> _saved = {'fieldday26', 'ade25'};
  bool _showImport = false;

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  List<Festival> get _filtered {
    if (_query.isEmpty) return widget.festivals;
    final q = _query.toLowerCase();
    return widget.festivals.where((f) {
      return f.name.toLowerCase().contains(q) ||
          f.city.toLowerCase().contains(q) ||
          f.genres.any((g) => g.toLowerCase().contains(q));
    }).toList();
  }

  int get _activeCount =>
      widget.festivals.where((f) => f.status != FestStatus.past).length;

  @override
  Widget build(BuildContext context) {
    final filtered = _filtered;
    final savedFests = filtered.where((f) => _saved.contains(f.id)).toList();
    final nowFests = filtered
        .where((f) => !_saved.contains(f.id) && f.status == FestStatus.live)
        .toList();
    final discoverFests = filtered
        .where((f) => !_saved.contains(f.id) && f.status != FestStatus.live)
        .toList();

    return Column(
      children: [
        // Scrollable body
        Expanded(
          child: widget.loading && widget.festivals.isEmpty
              ? const Center(
                  child: CircularProgressIndicator(
                    color: colorAccent,
                    strokeWidth: 1.5,
                  ),
                )
              : RefreshIndicator(
                  color: colorAccent,
                  backgroundColor: colorSurface1,
                  onRefresh: () async => widget.onRefresh?.call(),
                  child: ListView(
                    padding: EdgeInsets.zero,
                    children: [
                      // Page header
                      _PageHeader(
                        activeCount: _activeCount,
                        importExpanded: _showImport,
                        onAddClashfinder: () => setState(() {
                          _showImport = !_showImport;
                        }),
                      ),
                      if (_showImport)
                        ClashfinderImportPanel(
                          registered: widget.importRegistered,
                          onRegister: widget.onRegister,
                          onPreview: widget.onPreviewClashfinder,
                          onPublish: widget.onPublishClashfinder,
                          onPublished: widget.onFestivalPublished,
                          onClose: () => setState(() => _showImport = false),
                        ),
                      // Error banner
                      if (widget.error != null)
                        _ErrorBanner(
                          message: widget.error!,
                          onRetry: widget.onRefresh,
                        ),
                      // Search bar
                      _SearchBar(
                        controller: _searchController,
                        query: _query,
                        onChanged: (q) => setState(() => _query = q),
                        onClear: () {
                          _searchController.clear();
                          setState(() => _query = '');
                        },
                      ),
                      // Saved section
                      if (savedFests.isNotEmpty) ...[
                        _EyebrowRow(
                          label: '// SAVED',
                          pill: '★ ${savedFests.length}',
                          right: 'EDIT',
                          onRightTap: () {},
                        ),
                        ...savedFests.map(
                          (f) => FestivalRow(
                            fest: f,
                            saved: _saved.contains(f.id),
                            onToggleSave: () => setState(() {
                              if (_saved.contains(f.id)) {
                                _saved.remove(f.id);
                              } else {
                                _saved.add(f.id);
                              }
                            }),
                            onTap: () => widget.onFestivalTap(f),
                          ),
                        ),
                      ],
                      // Now section (live festivals not in saved)
                      if (nowFests.isNotEmpty) ...[
                        _EyebrowRow(
                          label: '// NOW',
                          right: '${nowFests.length} LIVE',
                        ),
                        ...nowFests.map(
                          (f) => FestivalRow(
                            fest: f,
                            saved: _saved.contains(f.id),
                            onToggleSave: () => setState(() {
                              if (_saved.contains(f.id)) {
                                _saved.remove(f.id);
                              } else {
                                _saved.add(f.id);
                              }
                            }),
                            onTap: () => widget.onFestivalTap(f),
                          ),
                        ),
                      ],
                      // Discover section
                      _EyebrowRow(
                        label: '// DISCOVER',
                        right: '${discoverFests.length} FESTIVALS',
                      ),
                      ...discoverFests.map(
                        (f) => FestivalRow(
                          fest: f,
                          saved: _saved.contains(f.id),
                          onToggleSave: () => setState(() {
                            if (_saved.contains(f.id)) {
                              _saved.remove(f.id);
                            } else {
                              _saved.add(f.id);
                            }
                          }),
                          onTap: () => widget.onFestivalTap(f),
                        ),
                      ),
                      // Empty state
                      if (filtered.isEmpty && !widget.loading)
                        _EmptyState(query: _query),
                      const SizedBox(height: 24),
                    ],
                  ),
                ),
        ),
      ],
    );
  }
}

class _PageHeader extends StatelessWidget {
  final int activeCount;
  final bool importExpanded;
  final VoidCallback onAddClashfinder;

  const _PageHeader({
    required this.activeCount,
    required this.importExpanded,
    required this.onAddClashfinder,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Festivals.',
            style: TextStyle(
              fontFamily: 'Helvetica',
              fontWeight: FontWeight.w700,
              fontSize: 34,
              letterSpacing: -0.02 * 34,
              height: 1,
              color: colorFg,
            ),
          ),
          const SizedBox(height: 4),
          Row(
            children: [
              Expanded(child: Text('$activeCount ACTIVE', style: _metaStyle)),
              Semantics(
                button: true,
                expanded: importExpanded,
                label: 'Add a Clashfinder event',
                child: InkWell(
                  onTap: onAddClashfinder,
                  child: SizedBox(
                    height: 44,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          importExpanded ? Icons.remove : Icons.add,
                          size: 14,
                          color: colorAccent,
                        ),
                        const SizedBox(width: 5),
                        const Text('ADD CLASHFINDER', style: _actionStyle),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  static const _actionStyle = TextStyle(
    fontFamily: 'JetBrainsMono',
    fontSize: 10,
    fontWeight: FontWeight.w700,
    color: colorAccent,
    letterSpacing: 0.08 * 10,
  );

  static const _metaStyle = TextStyle(
    fontFamily: 'JetBrainsMono',
    fontSize: 11,
    color: colorFg3,
    letterSpacing: 0.08 * 11,
    height: 1.3,
  );
}

class _SearchBar extends StatelessWidget {
  final TextEditingController controller;
  final String query;
  final ValueChanged<String> onChanged;
  final VoidCallback onClear;

  const _SearchBar({
    required this.controller,
    required this.query,
    required this.onChanged,
    required this.onClear,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 8, 18, 14),
      child: DottedBorder(
        color: query.isNotEmpty ? colorAccent : colorDotted,
        child: Container(
          color: colorSurface1,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Row(
            children: [
              const Icon(Icons.search, size: 16, color: colorFg3),
              const SizedBox(width: 8),
              Expanded(
                child: TextField(
                  controller: controller,
                  onChanged: onChanged,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontSize: 14,
                    color: colorFg,
                    letterSpacing: -0.01 * 14,
                  ),
                  decoration: const InputDecoration(
                    isDense: true,
                    contentPadding: EdgeInsets.zero,
                    border: InputBorder.none,
                    hintText: 'search festivals, cities, genres',
                    hintStyle: TextStyle(
                      color: colorFg4,
                      fontFamily: 'Helvetica',
                      fontSize: 14,
                    ),
                  ),
                  cursorColor: colorAccent,
                ),
              ),
              if (query.isNotEmpty)
                GestureDetector(
                  onTap: onClear,
                  child: const Padding(
                    padding: EdgeInsets.all(2),
                    child: Icon(Icons.close, size: 14, color: colorFg3),
                  ),
                )
              else
                // ⌘K badge
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 5,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: colorSurface2,
                    border: Border.all(color: colorHairline),
                  ),
                  child: const Text(
                    '⌘K',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      color: colorFg3,
                      letterSpacing: 0.04 * 10,
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

class _EyebrowRow extends StatelessWidget {
  final String label;
  final String? pill;
  final String? right;
  final VoidCallback? onRightTap;

  const _EyebrowRow({
    required this.label,
    this.pill,
    this.right,
    this.onRightTap,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 14, 18, 8),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        crossAxisAlignment: CrossAxisAlignment.baseline,
        textBaseline: TextBaseline.alphabetic,
        children: [
          Row(
            children: [
              Text(
                label,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: colorFg2,
                  letterSpacing: 0.08 * 11,
                  height: 1,
                ),
              ),
              if (pill != null) ...[
                const SizedBox(width: 8),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 2,
                  ),
                  color: colorAccent,
                  child: Text(
                    pill!,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      fontWeight: FontWeight.w700,
                      color: colorAccentInk,
                      letterSpacing: 0.06 * 9,
                      height: 1,
                    ),
                  ),
                ),
              ],
            ],
          ),
          if (right != null)
            GestureDetector(
              onTap: onRightTap,
              child: Text(
                right!,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  color: colorFg4,
                  letterSpacing: 0.08 * 10,
                  height: 1,
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  final String message;
  final VoidCallback? onRetry;
  const _ErrorBanner({required this.message, this.onRetry});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 0, 18, 8),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(
          border: Border.all(color: colorAccent, width: 1),
          color: colorSurface1,
        ),
        child: Row(
          children: [
            const Icon(Icons.cloud_off, size: 14, color: colorFg3),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                message.toUpperCase(),
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  color: colorFg3,
                  letterSpacing: 0.06 * 10,
                ),
              ),
            ),
            if (onRetry != null)
              GestureDetector(
                onTap: onRetry,
                child: const Text(
                  'RETRY',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w700,
                    color: colorAccent,
                    letterSpacing: 0.08 * 10,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _EmptyState extends StatelessWidget {
  final String query;
  const _EmptyState({required this.query});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(32),
      child: Center(
        child: RichText(
          textAlign: TextAlign.center,
          text: TextSpan(
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              color: colorFg3,
              letterSpacing: 0.08 * 11,
              height: 1.4,
            ),
            children: [
              const TextSpan(text: 'NO RESULTS // '),
              TextSpan(
                text: '"$query"',
                style: const TextStyle(color: colorAccent),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
