import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart' as markdown;

import '../theme/tokens.dart';
import '../widgets/chip.dart';
import '../widgets/dotted_border.dart';
import 'wiki_page.dart';
import 'wiki_repository.dart';

const _categoryLabels = <String, String>{
  'emergency': 'Emergency',
  'campsite': 'Campsite',
  'mobility': 'Mobility',
  'drug-testing': 'Drug testing',
  'substances': 'Substances',
  'meshtastic': 'Meshtastic',
  'offbeat': 'OFFBEAT',
};

class WikiScreen extends StatefulWidget {
  final String? countryCode;
  final WikiRepository? repository;

  const WikiScreen({super.key, this.countryCode, this.repository});

  @override
  State<WikiScreen> createState() => _WikiScreenState();
}

class _WikiScreenState extends State<WikiScreen> {
  late final WikiRepository _repository;
  late final Future<WikiCatalog> _catalogFuture;
  final _searchController = TextEditingController();
  String? _category;

  @override
  void initState() {
    super.initState();
    _repository = widget.repository ?? WikiRepository();
    _catalogFuture = _repository.load(countryCode: widget.countryCode);
    _searchController.addListener(_refresh);
  }

  @override
  void dispose() {
    _searchController
      ..removeListener(_refresh)
      ..dispose();
    super.dispose();
  }

  void _refresh() => setState(() {});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: colorBg,
      body: SafeArea(
        child: Column(
          children: [
            _WikiHeader(
              title: 'FIELD GUIDE',
              onBack: () => Navigator.of(context).pop(),
            ),
            Expanded(
              child: FutureBuilder<WikiCatalog>(
                future: _catalogFuture,
                builder: (context, snapshot) {
                  if (snapshot.hasError) {
                    return _WikiError(error: snapshot.error!);
                  }
                  if (!snapshot.hasData) {
                    return const Center(
                      child: CircularProgressIndicator(
                        color: colorAccent,
                        strokeWidth: 1.5,
                      ),
                    );
                  }
                  return _buildCatalog(snapshot.data!);
                },
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildCatalog(WikiCatalog catalog) {
    final pages = _repository.search(
      catalog.pages,
      _searchController.text,
      category: _category,
    );
    final emergencyPage = catalog.pages
        .where((page) => page.isEmergency)
        .firstOrNull;
    final categories = _categoryLabels.keys
        .where(
          (category) => catalog.pages.any((page) => page.category == category),
        )
        .toList(growable: false);

    return ListView(
      padding: const EdgeInsets.fromLTRB(sp4, sp4, sp4, sp7),
      children: [
        if (!catalog.countrySupported)
          Padding(
            padding: const EdgeInsets.only(bottom: sp4),
            child: _CountryWarning(countryCode: catalog.countryCode),
          ),
        Row(
          children: [
            _MetaLabel(
              label: catalog.countrySupported
                  ? '${catalog.countryCode} CONTENT PACK'
                  : 'UNIVERSAL CONTENT ONLY',
              color: catalog.countrySupported ? colorOk : colorWarn,
            ),
            const Spacer(),
            const _MetaLabel(label: 'AVAILABLE OFFLINE', color: colorFg3),
          ],
        ),
        if (emergencyPage != null) ...[
          const SizedBox(height: sp4),
          _EmergencyAction(
            page: emergencyPage,
            onTap: () => _openPage(catalog, emergencyPage),
          ),
        ],
        const SizedBox(height: sp5),
        Semantics(
          textField: true,
          label: 'Search the offline field guide',
          child: TextField(
            controller: _searchController,
            style: const TextStyle(color: colorFg, fontSize: tBody),
            decoration: InputDecoration(
              hintText: 'Search symptoms, substances or features',
              hintStyle: const TextStyle(color: colorFg3),
              prefixIcon: const Icon(Icons.search, color: colorFg3, size: 20),
              suffixIcon: _searchController.text.isEmpty
                  ? null
                  : IconButton(
                      tooltip: 'Clear search',
                      onPressed: _searchController.clear,
                      icon: const Icon(Icons.close, color: colorFg3, size: 18),
                    ),
              filled: true,
              fillColor: colorSurface1,
              enabledBorder: const OutlineInputBorder(
                borderRadius: BorderRadius.zero,
                borderSide: BorderSide(color: colorDotted),
              ),
              focusedBorder: const OutlineInputBorder(
                borderRadius: BorderRadius.zero,
                borderSide: BorderSide(color: colorAccent, width: 1.5),
              ),
            ),
          ),
        ),
        const SizedBox(height: sp3),
        SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            children: [
              MonoChip(
                label: 'All',
                active: _category == null,
                onTap: () => setState(() => _category = null),
              ),
              for (final category in categories) ...[
                const SizedBox(width: sp2),
                MonoChip(
                  label: _categoryLabels[category]!,
                  active: _category == category,
                  onTap: () => setState(() => _category = category),
                ),
              ],
            ],
          ),
        ),
        const SizedBox(height: sp5),
        _ResultsHeader(
          query: _searchController.text,
          category: _category,
          count: pages.length,
        ),
        const SizedBox(height: sp2),
        if (pages.isEmpty)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: sp7),
            child: Center(
              child: Text(
                'NO MATCHES IN THIS OFFLINE PACK',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: tMeta,
                  color: colorFg3,
                  letterSpacing: trMeta * tMeta,
                ),
              ),
            ),
          )
        else
          DottedBorder(
            child: Column(
              children: [
                for (var index = 0; index < pages.length; index++) ...[
                  if (index > 0) const Divider(height: 1, color: colorHairline),
                  _WikiPageRow(
                    page: pages[index],
                    onTap: () => _openPage(catalog, pages[index]),
                  ),
                ],
              ],
            ),
          ),
      ],
    );
  }

  void _openPage(WikiCatalog catalog, WikiPage page) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => WikiArticleScreen(catalog: catalog, page: page),
      ),
    );
  }
}

class WikiArticleScreen extends StatelessWidget {
  final WikiCatalog catalog;
  final WikiPage page;

  const WikiArticleScreen({
    super.key,
    required this.catalog,
    required this.page,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: colorBg,
      body: SafeArea(
        child: Column(
          children: [
            _WikiHeader(
              title:
                  _categoryLabels[page.category]?.toUpperCase() ??
                  'FIELD GUIDE',
              onBack: () => Navigator.of(context).pop(),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(sp4, sp5, sp4, sp7),
                child: Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 720),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Row(
                          children: [
                            _MetaLabel(
                              label: page.isEmergency
                                  ? 'URGENT'
                                  : page.contentStatus,
                              color: page.isEmergency ? colorErr : colorFg3,
                            ),
                            const Spacer(),
                            _MetaLabel(
                              label: 'VERIFIED ${_date(page.lastVerified)}',
                              color: colorFg3,
                            ),
                          ],
                        ),
                        const SizedBox(height: sp4),
                        markdown.MarkdownBody(
                          data: page.markdown,
                          selectable: true,
                          styleSheet: _markdownStyle(context),
                          onTapLink: (text, href, title) {
                            if (href == null) return;
                            if (href.startsWith('wiki:')) {
                              final target = catalog.pageById(
                                href.substring(5),
                              );
                              if (target != null) {
                                Navigator.of(context).push(
                                  MaterialPageRoute<void>(
                                    builder: (_) => WikiArticleScreen(
                                      catalog: catalog,
                                      page: target,
                                    ),
                                  ),
                                );
                              }
                              return;
                            }
                            _showExternalSource(context, text, href);
                          },
                        ),
                        for (final referenceId in page.generatedRefs)
                          if (catalog.generatedRecords[referenceId]
                              case final record?) ...[
                            const SizedBox(height: sp5),
                            _GeneratedReference(record: record),
                          ],
                        const SizedBox(height: sp6),
                        _Sources(page: page),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  static String _date(DateTime value) =>
      '${value.year}-${value.month.toString().padLeft(2, '0')}-${value.day.toString().padLeft(2, '0')}';
}

class _WikiHeader extends StatelessWidget {
  final String title;
  final VoidCallback onBack;

  const _WikiHeader({required this.title, required this.onBack});

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: navH,
        child: Row(
          children: [
            Semantics(
              button: true,
              label: 'Back',
              child: InkWell(
                onTap: onBack,
                child: const SizedBox(
                  width: tapMin,
                  height: tapMin,
                  child: Icon(Icons.chevron_left, color: colorFg, size: 20),
                ),
              ),
            ),
            const SizedBox(width: sp2),
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                  color: colorFg,
                  letterSpacing: 0.04 * 12,
                ),
              ),
            ),
            const Padding(
              padding: EdgeInsets.only(right: sp4),
              child: Icon(
                Icons.offline_bolt_outlined,
                color: colorOk,
                size: 17,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _EmergencyAction extends StatelessWidget {
  final WikiPage page;
  final VoidCallback onTap;

  const _EmergencyAction({required this.page, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: 'Emergency help: ${page.title}',
      child: Material(
        color: colorErr.withValues(alpha: 0.1),
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 72),
            padding: const EdgeInsets.all(sp4),
            decoration: BoxDecoration(
              border: Border.all(color: colorErr, width: 1.5),
            ),
            child: Row(
              children: [
                const Icon(Icons.emergency, color: colorErr, size: 24),
                const SizedBox(width: sp3),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Text(
                        'SOMEONE NEEDS HELP NOW',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 11,
                          fontWeight: FontWeight.w700,
                          color: colorErr,
                          letterSpacing: trMeta * 11,
                        ),
                      ),
                      const SizedBox(height: sp1),
                      Text(
                        page.summary,
                        style: const TextStyle(color: colorFg2),
                      ),
                    ],
                  ),
                ),
                const Icon(Icons.chevron_right, color: colorErr),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _WikiPageRow extends StatelessWidget {
  final WikiPage page;
  final VoidCallback onTap;

  const _WikiPageRow({required this.page, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: '${page.title}. ${page.summary}',
      child: Material(
        color: colorSurface1,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 72),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: sp4,
                vertical: sp3,
              ),
              child: Row(
                children: [
                  _CategoryGlyph(
                    category: page.category,
                    urgent: page.isEmergency,
                  ),
                  const SizedBox(width: sp3),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          page.title,
                          style: const TextStyle(
                            color: colorFg,
                            fontSize: tBody,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        const SizedBox(height: sp1),
                        Text(
                          page.summary,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            color: colorFg3,
                            fontSize: tSmall,
                            height: 1.25,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: sp2),
                  const Icon(Icons.chevron_right, color: colorFg4, size: 18),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _CategoryGlyph extends StatelessWidget {
  final String category;
  final bool urgent;

  const _CategoryGlyph({required this.category, required this.urgent});

  @override
  Widget build(BuildContext context) {
    final icon = switch (category) {
      'emergency' => Icons.emergency_outlined,
      'campsite' => Icons.terrain_outlined,
      'mobility' => Icons.directions_walk,
      'drug-testing' => Icons.science_outlined,
      'substances' => Icons.medication_outlined,
      'meshtastic' => Icons.cell_tower_outlined,
      _ => Icons.auto_stories_outlined,
    };
    return SizedBox(
      width: 32,
      height: 32,
      child: Icon(icon, color: urgent ? colorErr : colorAccent, size: 20),
    );
  }
}

class _ResultsHeader extends StatelessWidget {
  final String query;
  final String? category;
  final int count;

  const _ResultsHeader({
    required this.query,
    required this.category,
    required this.count,
  });

  @override
  Widget build(BuildContext context) {
    final label = query.trim().isNotEmpty
        ? 'SEARCH RESULTS'
        : category == null
        ? 'ALL GUIDES'
        : _categoryLabels[category]!.toUpperCase();
    return Row(
      children: [
        Text(
          label,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            color: colorFg2,
            fontSize: tMeta,
            fontWeight: FontWeight.w700,
            letterSpacing: trMeta * tMeta,
          ),
        ),
        const Spacer(),
        Text(
          '$count',
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            color: colorFg3,
            fontSize: tMeta,
          ),
        ),
      ],
    );
  }
}

class _CountryWarning extends StatelessWidget {
  final String countryCode;

  const _CountryWarning({required this.countryCode});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(sp3),
      decoration: BoxDecoration(border: Border.all(color: colorWarn)),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(Icons.public, color: colorWarn, size: 18),
          const SizedBox(width: sp2),
          Expanded(
            child: Text(
              'NO $countryCode COUNTRY PACK IS INSTALLED. Country-specific emergency numbers and guidance are hidden.',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                color: colorWarn,
                fontSize: tMeta,
                height: 1.35,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _GeneratedReference extends StatelessWidget {
  final WikiGeneratedRecord record;

  const _GeneratedReference({required this.record});

  @override
  Widget build(BuildContext context) {
    final routes = record.routes.where((route) => !route.dose.isEmpty).toList();
    if (routes.isEmpty) return const SizedBox.shrink();

    return Container(
      decoration: BoxDecoration(border: Border.all(color: colorWarn)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            color: colorWarn.withValues(alpha: 0.08),
            padding: const EdgeInsets.all(sp3),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'PSYCHONAUTWIKI REFERENCE DATA',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: tMeta,
                    fontWeight: FontWeight.w700,
                    color: colorWarn,
                    letterSpacing: trMeta * tMeta,
                  ),
                ),
                const SizedBox(height: sp1),
                Text(
                  'Values, units, route names and categories below are reproduced verbatim from ${record.sourceName}. They are source labels, not safe doses. Identity, purity, concentration, route and individual response are uncertain.',
                  style: TextStyle(
                    color: colorFg2,
                    fontSize: tSmall,
                    height: 1.35,
                  ),
                ),
              ],
            ),
          ),
          for (final route in routes) ...[
            const Divider(height: 1, color: colorHairline),
            Padding(
              padding: const EdgeInsets.all(sp3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    route.name.toUpperCase(),
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: tMeta,
                      fontWeight: FontWeight.w700,
                      color: colorFg,
                      letterSpacing: trMeta * tMeta,
                    ),
                  ),
                  const SizedBox(height: sp2),
                  Wrap(
                    spacing: sp3,
                    runSpacing: sp2,
                    children: [
                      if (route.dose.threshold != null)
                        _ReferenceValue(
                          label: 'Threshold',
                          value: _number(
                            route.dose.threshold!,
                            route.dose.units,
                          ),
                        ),
                      if (!route.dose.light.isEmpty)
                        _ReferenceValue(
                          label: 'Light',
                          value: _range(route.dose.light, route.dose.units),
                        ),
                      if (!route.dose.common.isEmpty)
                        _ReferenceValue(
                          label: 'Common',
                          value: _range(route.dose.common, route.dose.units),
                        ),
                      if (!route.dose.strong.isEmpty)
                        _ReferenceValue(
                          label: 'Strong',
                          value: _range(route.dose.strong, route.dose.units),
                        ),
                      if (route.dose.heavy != null)
                        _ReferenceValue(
                          label: 'Heavy+',
                          value: _number(route.dose.heavy!, route.dose.units),
                        ),
                    ],
                  ),
                  if (!route.onset.isEmpty || !route.total.isEmpty) ...[
                    const SizedBox(height: sp3),
                    Text(
                      [
                        if (!route.onset.isEmpty)
                          'Onset ${_range(route.onset, route.onset.units)}',
                        if (!route.total.isEmpty)
                          'Total ${_range(route.total, route.total.units)}',
                      ].join(' · '),
                      style: const TextStyle(color: colorFg3, fontSize: tSmall),
                    ),
                  ],
                ],
              ),
            ),
          ],
          Padding(
            padding: const EdgeInsets.fromLTRB(sp3, 0, sp3, sp3),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Imported ${WikiArticleScreen._date(record.retrievedAt)}. OFFBEAT has not altered the source values or labels.',
                  style: const TextStyle(color: colorFg3, fontSize: tMeta),
                ),
                const SizedBox(height: sp2),
                InkWell(
                  onTap: () => _showExternalSource(
                    context,
                    record.sourceName,
                    record.sourceUrl,
                  ),
                  child: Text(
                    'SOURCE: PsychonautWiki — ${record.sourceName} · revision ${record.sourceRevision ?? 'unknown'}',
                    style: const TextStyle(
                      color: colorFg2,
                      fontSize: tMeta,
                      decoration: TextDecoration.underline,
                    ),
                  ),
                ),
                const SizedBox(height: sp1),
                InkWell(
                  onTap: () => _showExternalSource(
                    context,
                    record.contentLicense,
                    record.contentLicenseUrl,
                  ),
                  child: Text(
                    'LICENCE: ${record.contentLicense}',
                    style: const TextStyle(
                      color: colorFg2,
                      fontSize: tMeta,
                      decoration: TextDecoration.underline,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  static String _number(double value, String? units) {
    final text = value == value.roundToDouble()
        ? value.toInt().toString()
        : value
              .toStringAsFixed(2)
              .replaceFirst(RegExp(r'0+$'), '')
              .replaceFirst(RegExp(r'\.$'), '');
    return units == null ? text : '$text $units';
  }

  static String _range(WikiRange range, String? fallbackUnits) {
    final units = range.units ?? fallbackUnits;
    if (range.min != null && range.max != null) {
      return '${_number(range.min!, null)}–${_number(range.max!, units)}';
    }
    if (range.min != null) return '${_number(range.min!, units)}+';
    if (range.max != null) return 'up to ${_number(range.max!, units)}';
    return 'not stated';
  }
}

class _ReferenceValue extends StatelessWidget {
  final String label;
  final String value;

  const _ReferenceValue({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 88,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label.toUpperCase(),
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              color: colorFg3,
              fontSize: 9,
              letterSpacing: trMeta * 9,
            ),
          ),
          const SizedBox(height: 2),
          Text(
            value,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              color: colorFg,
              fontSize: tSmall,
              fontWeight: FontWeight.w700,
            ),
          ),
        ],
      ),
    );
  }
}

class _Sources extends StatelessWidget {
  final WikiPage page;

  const _Sources({required this.page});

  @override
  Widget build(BuildContext context) {
    return DottedBorder(
      child: Padding(
        padding: const EdgeInsets.all(sp4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text(
              'SOURCES & ATTRIBUTION',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: tMeta,
                fontWeight: FontWeight.w700,
                color: colorFg2,
                letterSpacing: trMeta * tMeta,
              ),
            ),
            const SizedBox(height: sp2),
            Text(
              'Status: ${page.contentStatus}. Last verified ${WikiArticleScreen._date(page.lastVerified)}.',
              style: const TextStyle(color: colorFg3, fontSize: tSmall),
            ),
            const SizedBox(height: sp3),
            for (final source in page.sources)
              InkWell(
                onTap: () =>
                    _showExternalSource(context, source.title, source.url),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: sp2),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Icon(Icons.open_in_new, color: colorFg3, size: 16),
                      const SizedBox(width: sp2),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              source.title,
                              style: const TextStyle(
                                color: colorFg,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            Text(
                              source.publisher,
                              style: const TextStyle(
                                color: colorFg3,
                                fontSize: tSmall,
                              ),
                            ),
                            if (source.revision != null)
                              Text(
                                'Revision ${source.revision}',
                                style: const TextStyle(
                                  color: colorFg3,
                                  fontSize: tMeta,
                                ),
                              ),
                            if (source.license != null)
                              Text(
                                source.license!,
                                style: const TextStyle(
                                  color: colorFg3,
                                  fontSize: tMeta,
                                ),
                              ),
                          ],
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

class _MetaLabel extends StatelessWidget {
  final String label;
  final Color color;

  const _MetaLabel({required this.label, required this.color});

  @override
  Widget build(BuildContext context) {
    return Text(
      label.toUpperCase(),
      style: TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: 9,
        fontWeight: FontWeight.w700,
        color: color,
        letterSpacing: trMeta * 9,
      ),
    );
  }
}

class _WikiError extends StatelessWidget {
  final Object error;

  const _WikiError({required this.error});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(sp5),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.error_outline, color: colorErr, size: 28),
            const SizedBox(height: sp3),
            const Text(
              'THE OFFLINE GUIDE COULD NOT BE LOADED',
              textAlign: TextAlign.center,
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                color: colorFg,
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: sp2),
            Text(
              '$error',
              textAlign: TextAlign.center,
              style: const TextStyle(color: colorFg3, fontSize: tSmall),
            ),
          ],
        ),
      ),
    );
  }
}

markdown.MarkdownStyleSheet _markdownStyle(BuildContext context) {
  return markdown.MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
    p: const TextStyle(color: colorFg2, fontSize: tBody, height: lhBody),
    h1: const TextStyle(
      color: colorFg,
      fontSize: tH1,
      height: lhSnug,
      fontWeight: FontWeight.w800,
    ),
    h2: const TextStyle(
      color: colorFg,
      fontSize: tH2,
      height: lhSnug,
      fontWeight: FontWeight.w700,
    ),
    h3: const TextStyle(
      color: colorFg2,
      fontSize: tH3,
      height: lhSnug,
      fontWeight: FontWeight.w700,
    ),
    strong: const TextStyle(color: colorFg, fontWeight: FontWeight.w800),
    em: const TextStyle(color: colorFg2, fontStyle: FontStyle.italic),
    a: const TextStyle(
      color: colorCoAccent,
      decoration: TextDecoration.underline,
      decorationColor: colorCoAccent,
    ),
    listBullet: const TextStyle(color: colorAccent, fontSize: tBody),
    blockquote: const TextStyle(
      color: colorFg,
      fontSize: tBody,
      height: lhBody,
    ),
    blockquoteDecoration: BoxDecoration(
      color: colorWarn.withValues(alpha: 0.08),
      border: Border.all(color: colorWarn),
    ),
    blockquotePadding: const EdgeInsets.all(sp3),
    horizontalRuleDecoration: const BoxDecoration(
      border: Border(top: BorderSide(color: colorDotted)),
    ),
    tableBorder: TableBorder.all(color: colorDotted),
    tableHead: const TextStyle(color: colorFg, fontWeight: FontWeight.w700),
    tableBody: const TextStyle(color: colorFg2, fontSize: tSmall),
    tableCellsPadding: const EdgeInsets.all(sp2),
  );
}

void _showExternalSource(BuildContext context, String text, String url) {
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: colorBg,
    builder: (sheetContext) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(sp5),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text(
              'EXTERNAL SOURCE',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                color: colorFg3,
                fontSize: tMeta,
                fontWeight: FontWeight.w700,
                letterSpacing: trMeta * tMeta,
              ),
            ),
            const SizedBox(height: sp3),
            Text(
              text,
              style: const TextStyle(
                color: colorFg,
                fontSize: tH3,
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: sp2),
            SelectableText(
              url,
              style: const TextStyle(color: colorCoAccent, fontSize: tSmall),
            ),
            const SizedBox(height: sp4),
            SizedBox(
              height: tapMin,
              child: OutlinedButton.icon(
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: url));
                  if (sheetContext.mounted) Navigator.of(sheetContext).pop();
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('SOURCE LINK COPIED')),
                    );
                  }
                },
                icon: const Icon(Icons.copy, size: 16),
                label: const Text('COPY LINK'),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
