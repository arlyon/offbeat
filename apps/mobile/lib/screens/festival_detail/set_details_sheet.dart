import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../data/group_schedule_overlay.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

void showSetDetailsSheet(
  BuildContext context, {
  required FestSet set,
  required List<Stage> stages,
  required List<Day> days,
  required List<FestSet> allSets,
  void Function(String setId)? onStar,
  void Function(Stage stage)? onStageChat,
}) {
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    builder: (_) => _SetDetailsSheet(
      set: set,
      stages: stages,
      days: days,
      allSets: allSets,
      onStar: onStar,
      onStageChat: onStageChat,
    ),
  );
}

class _SetDetailsSheet extends StatefulWidget {
  final FestSet set;
  final List<Stage> stages;
  final List<Day> days;
  final List<FestSet> allSets;
  final void Function(String setId)? onStar;
  final void Function(Stage stage)? onStageChat;

  const _SetDetailsSheet({
    required this.set,
    required this.stages,
    required this.days,
    required this.allSets,
    this.onStar,
    this.onStageChat,
  });

  @override
  State<_SetDetailsSheet> createState() => _SetDetailsSheetState();
}

class _SetDetailsSheetState extends State<_SetDetailsSheet> {
  late FestSet _selectedSet;
  final _history = <FestSet>[];
  final _starredOverrides = <String, bool>{};
  bool _connectionsExpanded = false;

  Map<String, Stage> get _stageById => {
    for (final stage in widget.stages) stage.id: stage,
  };

  Map<String, Day> get _dayById => {for (final day in widget.days) day.id: day};

  Map<String, int> get _dayOrder => {
    for (var index = 0; index < widget.days.length; index++)
      widget.days[index].id: index,
  };

  @override
  void initState() {
    super.initState();
    _selectedSet = widget.set;
  }

  bool get _starred =>
      _starredOverrides[_selectedSet.id] ?? _selectedSet.starred;

  List<FestSet> get _artistSets {
    final artistIds = _selectedSet.artistIds.toSet();
    final artist = _normalizeArtist(_selectedSet.artist);
    final sets = widget.allSets.where((set) {
      if (artistIds.isNotEmpty) {
        return set.artistIds.any(artistIds.contains);
      }
      return _normalizeArtist(set.artist) == artist;
    }).toList();
    _sortSets(sets);
    return sets;
  }

  List<FestSet> get _clashes {
    final ids = _selectedSet.clashes.toSet();
    final sets = widget.allSets.where((set) => ids.contains(set.id)).toList();
    _sortSets(sets);
    return sets;
  }

  List<({ScheduleSupporter supporter, int appearances})> get _friendOverlap {
    final supporters = <String, ScheduleSupporter>{};
    final appearances = <String, Set<String>>{};
    for (final set in _artistSets) {
      for (final supporter in set.supporters) {
        supporters[supporter.userId] = supporter;
        (appearances[supporter.userId] ??= {}).add(set.id);
      }
    }
    final result = supporters.entries
        .map(
          (entry) => (
            supporter: entry.value,
            appearances: appearances[entry.key]?.length ?? 0,
          ),
        )
        .toList();
    result.sort((a, b) {
      final byCount = b.appearances.compareTo(a.appearances);
      if (byCount != 0) return byCount;
      return a.supporter.displayName.toLowerCase().compareTo(
        b.supporter.displayName.toLowerCase(),
      );
    });
    return result;
  }

  void _sortSets(List<FestSet> sets) {
    final order = _dayOrder;
    sets.sort((a, b) {
      final byDay = (order[a.day] ?? 1 << 20).compareTo(
        order[b.day] ?? 1 << 20,
      );
      if (byDay != 0) return byDay;
      final byTime = a.t.compareTo(b.t);
      if (byTime != 0) return byTime;
      return a.stage.compareTo(b.stage);
    });
  }

  void _toggleStar() {
    final next = !_starred;
    setState(() => _starredOverrides[_selectedSet.id] = next);
    widget.onStar?.call(_selectedSet.id);
  }

  void _selectSet(FestSet set) {
    if (set.id == _selectedSet.id) return;
    setState(() {
      _history.add(_selectedSet);
      _selectedSet = set;
    });
  }

  void _goBack() {
    if (_history.isEmpty) return;
    setState(() => _selectedSet = _history.removeLast());
  }

  Future<void> _openArtistLink(ArtistLink link) async {
    final uri = Uri.tryParse(link.url);
    final isSafe = uri != null && uri.scheme == 'https' && uri.host.isNotEmpty;
    var launched = false;
    if (isSafe) {
      try {
        launched = await launchUrl(uri, mode: LaunchMode.externalApplication);
      } on Exception {
        launched = false;
      }
    }
    if (!mounted || launched) return;
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(const SnackBar(content: Text('COULD NOT OPEN LINK')));
  }

  @override
  Widget build(BuildContext context) {
    final set = _selectedSet;
    final stage = _stageById[set.stage];
    final day = _dayById[set.day];
    if (stage == null || day == null) return const SizedBox.shrink();

    final artistSets = _artistSets;
    final clashes = _clashes;
    final friendOverlap = _friendOverlap;

    return DraggableScrollableSheet(
      initialChildSize: 0.68,
      minChildSize: 0.42,
      maxChildSize: 0.92,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: SafeArea(
          top: false,
          child: Column(
            children: [
              Center(
                child: Container(
                  margin: const EdgeInsets.only(top: 8),
                  width: 36,
                  height: 3,
                  color: colorFg4,
                ),
              ),
              _Header(
                artist: set.artist,
                onBack: _history.isEmpty ? null : _goBack,
              ),
              Expanded(
                child: ListView(
                  controller: scrollController,
                  padding: const EdgeInsets.fromLTRB(18, 16, 18, 24),
                  children: [
                    if (set.live || set.cancelled) ...[
                      Wrap(
                        spacing: 6,
                        runSpacing: 6,
                        children: [
                          if (set.live)
                            const _Status(label: 'ON NOW', color: colorAccent),
                          if (set.cancelled)
                            const _Status(label: 'CANCELLED', color: colorErr),
                        ],
                      ),
                      const SizedBox(height: 16),
                    ],
                    _DetailRow(
                      label: 'WHEN',
                      value:
                          '${day.label.toUpperCase()} ${day.dayNum} ${day.month.toUpperCase()}  '
                          '${fmtTime(set.t)} → ${fmtTime(set.t + set.dur)}',
                    ),
                    _DetailRow(
                      label: 'STAGE',
                      value: stage.name.toUpperCase(),
                      swatch: Color(stage.color),
                    ),
                    _DetailRow(label: 'LENGTH', value: '${set.dur} MIN'),
                    if (set.genre.trim().isNotEmpty)
                      _DetailRow(
                        label: 'GENRE',
                        value: set.genre.toUpperCase(),
                      ),
                    if (set.artistProfiles.isNotEmpty) ...[
                      const SizedBox(height: 22),
                      _ArtistProfileSection(
                        profiles: set.artistProfiles,
                        onLinkTap: _openArtistLink,
                      ),
                    ],
                    const SizedBox(height: 22),
                    _ArtistPath(
                      sets: artistSets,
                      selectedSetId: set.id,
                      stageById: _stageById,
                      dayById: _dayById,
                      expanded: _connectionsExpanded,
                      onSetTap: _selectSet,
                    ),
                    if (friendOverlap.isNotEmpty || clashes.isNotEmpty) ...[
                      const SizedBox(height: 22),
                      _Connections(
                        friends: friendOverlap,
                        clashes: clashes,
                        stageById: _stageById,
                        dayById: _dayById,
                        expanded: _connectionsExpanded,
                        onSetTap: _selectSet,
                      ),
                    ],
                    const SizedBox(height: 8),
                    _ExpandConnections(
                      expanded: _connectionsExpanded,
                      onTap: () => setState(
                        () => _connectionsExpanded = !_connectionsExpanded,
                      ),
                    ),
                  ],
                ),
              ),
              DottedBorder.top(
                child: Padding(
                  padding: const EdgeInsets.all(14),
                  child: Row(
                    children: [
                      Expanded(
                        child: _Action(
                          label: _starred ? 'UNLIKE' : 'LIKE',
                          icon: _starred ? Icons.star : Icons.star_border,
                          primary: true,
                          onTap: _toggleStar,
                        ),
                      ),
                      if (widget.onStageChat != null) ...[
                        const SizedBox(width: 8),
                        Expanded(
                          child: _Action(
                            label: 'STAGE CHAT',
                            icon: Icons.chat_bubble_outline,
                            onTap: () {
                              Navigator.pop(context);
                              widget.onStageChat?.call(stage);
                            },
                          ),
                        ),
                      ],
                    ],
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

String _normalizeArtist(String artist) =>
    artist.trim().toLowerCase().replaceAll(RegExp(r'\s+'), ' ');

class _Header extends StatelessWidget {
  final String artist;
  final VoidCallback? onBack;

  const _Header({required this.artist, this.onBack});

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Padding(
        padding: EdgeInsets.fromLTRB(onBack == null ? 18 : 6, 12, 18, 14),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (onBack != null)
              Semantics(
                button: true,
                label: 'Previous set details',
                child: InkWell(
                  onTap: onBack,
                  child: const SizedBox(
                    width: 44,
                    height: 44,
                    child: Icon(Icons.chevron_left, size: 20, color: colorFg2),
                  ),
                ),
              ),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'SET DETAILS //',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.08 * 10,
                      color: colorAccent,
                    ),
                  ),
                  const SizedBox(height: 5),
                  Text(
                    artist,
                    maxLines: 3,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontSize: 28,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -0.02 * 28,
                      height: 1,
                      color: colorFg,
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

class _ArtistProfileSection extends StatelessWidget {
  final List<ArtistProfile> profiles;
  final ValueChanged<ArtistLink> onLinkTap;

  const _ArtistProfileSection({
    required this.profiles,
    required this.onLinkTap,
  });

  static bool _safeLink(ArtistLink link) {
    final uri = Uri.tryParse(link.url);
    return uri != null && uri.scheme == 'https' && uri.host.isNotEmpty;
  }

  static List<ArtistLink> _visibleLinks(ArtistProfile profile) {
    const supportedKinds = {'spotify', 'soundcloud', 'website'};
    final byKind = <String, ArtistLink>{};
    for (final link in profile.links) {
      final kind = link.kind.toLowerCase();
      if (supportedKinds.contains(kind) && _safeLink(link)) {
        byKind.putIfAbsent(kind, () => link);
      }
    }
    return [
      for (final kind in const ['spotify', 'soundcloud', 'website'])
        ?byKind[kind],
    ];
  }

  static bool _hasContent(ArtistProfile profile) {
    return (profile.description?.trim().isNotEmpty ?? false) ||
        profile.genres.isNotEmpty ||
        (profile.country?.trim().isNotEmpty ?? false) ||
        (profile.artistType?.trim().isNotEmpty ?? false) ||
        _visibleLinks(profile).isNotEmpty;
  }

  @override
  Widget build(BuildContext context) {
    final visibleProfiles = profiles.where(_hasContent).toList();
    if (visibleProfiles.isEmpty) return const SizedBox.shrink();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionLabel(label: 'ABOUT'),
        const SizedBox(height: 10),
        for (var index = 0; index < visibleProfiles.length; index++) ...[
          if (index > 0) ...[
            const SizedBox(height: 16),
            DottedBorder.top(child: const SizedBox(height: 16)),
          ],
          _ArtistProfileBlock(
            profile: visibleProfiles[index],
            showName: visibleProfiles.length > 1,
            links: _visibleLinks(visibleProfiles[index]),
            onLinkTap: onLinkTap,
          ),
        ],
      ],
    );
  }
}

class _ArtistProfileBlock extends StatelessWidget {
  final ArtistProfile profile;
  final bool showName;
  final List<ArtistLink> links;
  final ValueChanged<ArtistLink> onLinkTap;

  const _ArtistProfileBlock({
    required this.profile,
    required this.showName,
    required this.links,
    required this.onLinkTap,
  });

  @override
  Widget build(BuildContext context) {
    final metadata = [
      if (profile.country?.trim() case final country? when country.isNotEmpty)
        country,
      if (profile.artistType?.trim() case final type? when type.isNotEmpty)
        type,
    ];
    final description = profile.description?.trim();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _ArtistPlaceholder(name: profile.name),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (showName) ...[
                    Text(
                      profile.name.toUpperCase(),
                      style: _metaStyle.copyWith(color: colorFg),
                    ),
                    const SizedBox(height: 8),
                  ],
                  if (metadata.isNotEmpty) ...[
                    Text(metadata.join(' · ').toUpperCase(), style: _metaStyle),
                    const SizedBox(height: 8),
                  ] else if (!showName) ...[
                    const Text('OFFLINE PROFILE', style: _metaStyle),
                    const SizedBox(height: 8),
                  ],
                  if (description != null && description.isNotEmpty)
                    Text(
                      description,
                      style: const TextStyle(
                        fontFamily: 'Helvetica',
                        fontSize: 14,
                        height: 1.45,
                        color: colorFg2,
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        if (profile.genres.isNotEmpty) ...[
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final genre in profile.genres.take(5))
                _GenreLabel(label: genre),
            ],
          ),
          const SizedBox(height: 12),
        ],
        if (links.isNotEmpty)
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final link in links)
                _ArtistLinkAction(link: link, onTap: () => onLinkTap(link)),
            ],
          ),
      ],
    );
  }
}

class _ArtistPlaceholder extends StatelessWidget {
  final String name;

  const _ArtistPlaceholder({required this.name});

  @override
  Widget build(BuildContext context) {
    final trimmedName = name.trim();
    final initial = trimmedName.isEmpty
        ? '?'
        : String.fromCharCodes(trimmedName.runes.take(1)).toUpperCase();
    return Semantics(
      image: true,
      excludeSemantics: true,
      label: 'Artist image unavailable, placeholder for $name',
      child: Container(
        width: 56,
        height: 56,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: colorAccentWash,
          border: Border.all(color: colorDotted, width: bdDotWidth),
        ),
        child: Text(
          initial,
          style: const TextStyle(
            fontFamily: 'Helvetica',
            fontSize: 24,
            fontWeight: FontWeight.w700,
            color: colorAccent,
          ),
        ),
      ),
    );
  }
}

class _GenreLabel extends StatelessWidget {
  final String label;

  const _GenreLabel({required this.label});

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
    decoration: BoxDecoration(border: Border.all(color: colorDotted)),
    child: Text(
      label.toUpperCase(),
      style: _metaStyle.copyWith(color: colorFg2),
    ),
  );
}

class _ArtistLinkAction extends StatelessWidget {
  final ArtistLink link;
  final VoidCallback onTap;

  const _ArtistLinkAction({required this.link, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final kind = link.kind.toLowerCase();
    final icon = switch (kind) {
      'spotify' => Icons.play_circle_outline,
      'soundcloud' => Icons.cloud_outlined,
      _ => Icons.language,
    };
    final label = switch (kind) {
      'spotify' => 'SPOTIFY',
      'soundcloud' => 'SOUNDCLOUD',
      _ => 'WEBSITE',
    };

    return Semantics(
      button: true,
      label: 'Open $label externally',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: tapMin),
            padding: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(
              border: Border.all(color: colorDotted, width: bdDotWidth),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(icon, size: 16, color: colorAccent),
                const SizedBox(width: 8),
                Text(label, style: _metaStyle.copyWith(color: colorFg)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ArtistPath extends StatelessWidget {
  final List<FestSet> sets;
  final String selectedSetId;
  final Map<String, Stage> stageById;
  final Map<String, Day> dayById;
  final bool expanded;
  final ValueChanged<FestSet> onSetTap;

  const _ArtistPath({
    required this.sets,
    required this.selectedSetId,
    required this.stageById,
    required this.dayById,
    required this.expanded,
    required this.onSetTap,
  });

  List<FestSet> get _visibleSets {
    if (expanded || sets.length <= 3) return sets;
    final selected = sets.firstWhere((set) => set.id == selectedSetId);
    return {
      sets.first.id: sets.first,
      selected.id: selected,
      sets.last.id: sets.last,
    }.values.toList();
  }

  @override
  Widget build(BuildContext context) {
    final totalMinutes = sets.fold<int>(0, (total, set) => total + set.dur);
    final visible = _visibleSets;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            const Expanded(child: _SectionLabel(label: 'ARTIST PATH')),
            Text(
              sets.length == 1
                  ? 'ONLY APPEARANCE · $totalMinutes MIN'
                  : '${sets.length} APPEARANCES · $totalMinutes MIN',
              style: _metaStyle.copyWith(color: colorFg2),
            ),
          ],
        ),
        const SizedBox(height: 8),
        DottedBorder.top(
          child: Column(
            children: [
              for (var index = 0; index < visible.length; index++)
                _PathStop(
                  set: visible[index],
                  stage: stageById[visible[index].stage],
                  day: dayById[visible[index].day],
                  selected: visible[index].id == selectedSetId,
                  first: index == 0,
                  last: index == visible.length - 1,
                  gapBefore:
                      !expanded &&
                      index > 0 &&
                      sets.indexOf(visible[index]) -
                              sets.indexOf(visible[index - 1]) >
                          1,
                  onTap: () => onSetTap(visible[index]),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

class _PathStop extends StatelessWidget {
  final FestSet set;
  final Stage? stage;
  final Day? day;
  final bool selected;
  final bool first;
  final bool last;
  final bool gapBefore;
  final VoidCallback onTap;

  const _PathStop({
    required this.set,
    required this.stage,
    required this.day,
    required this.selected,
    required this.first,
    required this.last,
    required this.gapBefore,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final stageColor = Color(stage?.color ?? 0xFF777777);
    return DottedBorder.bottom(
      child: Material(
        color: selected ? colorAccentWash : Colors.transparent,
        child: InkWell(
          onTap: selected ? null : onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 58),
            child: Row(
              children: [
                SizedBox(
                  width: 30,
                  height: 58,
                  child: Stack(
                    alignment: Alignment.center,
                    children: [
                      if (!first || gapBefore)
                        Positioned(
                          top: 0,
                          bottom: 29,
                          child: Container(
                            width: 1,
                            color: gapBefore ? colorFg4 : stageColor,
                          ),
                        ),
                      if (!last)
                        Positioned(
                          top: 29,
                          bottom: 0,
                          child: Container(width: 1, color: stageColor),
                        ),
                      Container(
                        width: 11,
                        height: 11,
                        decoration: BoxDecoration(
                          color: selected ? colorAccent : colorSurface1,
                          border: Border.all(
                            color: selected ? colorAccent : stageColor,
                            width: 2,
                          ),
                        ),
                        child: set.cancelled
                            ? const Icon(Icons.close, size: 7, color: colorErr)
                            : null,
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 4),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        '${day?.label.toUpperCase() ?? set.day.toUpperCase()} '
                        '${day?.dayNum ?? ''} · ${fmtTime(set.t)}',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          color: colorFg,
                        ),
                      ),
                      const SizedBox(height: 3),
                      Text(
                        (stage?.name ?? set.stage).toUpperCase(),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          color: colorFg3,
                        ),
                      ),
                    ],
                  ),
                ),
                if (selected)
                  const Padding(
                    padding: EdgeInsets.only(right: 10),
                    child: Text('THIS SET', style: _accentMetaStyle),
                  )
                else
                  const Padding(
                    padding: EdgeInsets.only(right: 8),
                    child: Icon(Icons.chevron_right, size: 18, color: colorFg3),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Connections extends StatelessWidget {
  final List<({ScheduleSupporter supporter, int appearances})> friends;
  final List<FestSet> clashes;
  final Map<String, Stage> stageById;
  final Map<String, Day> dayById;
  final bool expanded;
  final ValueChanged<FestSet> onSetTap;

  const _Connections({
    required this.friends,
    required this.clashes,
    required this.stageById,
    required this.dayById,
    required this.expanded,
    required this.onSetTap,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionLabel(label: 'CONNECTIONS'),
        const SizedBox(height: 8),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            if (friends.isNotEmpty)
              _ConnectionCount(
                label:
                    '${friends.length} FRIEND${friends.length == 1 ? '' : 'S'}',
                color: colorCoAccent,
              ),
            if (clashes.isNotEmpty)
              _ConnectionCount(
                label:
                    '${clashes.length} CLASH${clashes.length == 1 ? '' : 'ES'}',
                color: colorWarn,
              ),
          ],
        ),
        if (expanded) ...[
          if (friends.isNotEmpty) ...[
            const SizedBox(height: 14),
            const _SectionLabel(label: 'FRIEND OVERLAP'),
            const SizedBox(height: 6),
            for (final friend in friends)
              DottedBorder.bottom(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 44),
                  child: Row(
                    children: [
                      Container(width: 7, height: 7, color: colorCoAccent),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text(
                          friend.supporter.displayName.toUpperCase(),
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                      ),
                      Text(
                        'LIKES ${friend.appearances} APPEARANCE${friend.appearances == 1 ? '' : 'S'}',
                        style: _metaStyle,
                      ),
                    ],
                  ),
                ),
              ),
          ],
          if (clashes.isNotEmpty) ...[
            const SizedBox(height: 14),
            const _SectionLabel(label: 'CLASH LINKS'),
            const SizedBox(height: 6),
            for (final clash in clashes)
              _ClashLink(
                set: clash,
                stage: stageById[clash.stage],
                day: dayById[clash.day],
                onTap: () => onSetTap(clash),
              ),
          ],
        ],
      ],
    );
  }
}

class _ClashLink extends StatelessWidget {
  final FestSet set;
  final Stage? stage;
  final Day? day;
  final VoidCallback onTap;

  const _ClashLink({
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
            constraints: const BoxConstraints(minHeight: 52),
            child: Row(
              children: [
                const SizedBox(
                  width: 30,
                  child: Icon(Icons.call_split, size: 16, color: colorWarn),
                ),
                const SizedBox(width: 4),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        set.artist,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontFamily: 'Helvetica',
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          color: colorFg,
                        ),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        '${day?.label.toUpperCase() ?? set.day.toUpperCase()} · '
                        '${fmtTime(set.t)} · ${(stage?.name ?? set.stage).toUpperCase()}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: _metaStyle,
                      ),
                    ],
                  ),
                ),
                const Icon(Icons.chevron_right, size: 18, color: colorFg3),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ConnectionCount extends StatelessWidget {
  final String label;
  final Color color;

  const _ConnectionCount({required this.label, required this.color});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: BoxDecoration(border: Border.all(color: color, width: 1)),
      child: Text(
        label,
        style: TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 9,
          fontWeight: FontWeight.w700,
          color: color,
        ),
      ),
    );
  }
}

class _ExpandConnections extends StatelessWidget {
  final bool expanded;
  final VoidCallback onTap;

  const _ExpandConnections({required this.expanded, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      expanded: expanded,
      child: InkWell(
        onTap: onTap,
        child: SizedBox(
          height: 48,
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                expanded ? 'COLLAPSE' : 'EXPAND CONNECTIONS',
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.08 * 10,
                  color: colorAccent,
                ),
              ),
              const SizedBox(width: 6),
              Icon(
                expanded ? Icons.expand_less : Icons.expand_more,
                size: 18,
                color: colorAccent,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Status extends StatelessWidget {
  final String label;
  final Color color;

  const _Status({required this.label, required this.color});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
      decoration: BoxDecoration(border: Border.all(color: color, width: 1)),
      child: Text(
        label,
        style: TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 9,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.08 * 9,
          color: color,
        ),
      ),
    );
  }
}

class _DetailRow extends StatelessWidget {
  final String label;
  final String value;
  final Color? swatch;

  const _DetailRow({required this.label, required this.value, this.swatch});

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 48),
        child: Row(
          children: [
            SizedBox(width: 72, child: Text(label, style: _metaStyle)),
            if (swatch != null) ...[
              Container(width: 10, height: 10, color: swatch),
              const SizedBox(width: 8),
            ],
            Expanded(
              child: Text(
                value,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: colorFg,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SectionLabel extends StatelessWidget {
  final String label;

  const _SectionLabel({required this.label});

  @override
  Widget build(BuildContext context) => Text(label, style: _metaStyle);
}

class _Action extends StatelessWidget {
  final String label;
  final IconData icon;
  final bool primary;
  final VoidCallback onTap;

  const _Action({
    required this.label,
    required this.icon,
    required this.onTap,
    this.primary = false,
  });

  @override
  Widget build(BuildContext context) {
    final foreground = primary ? colorAccentInk : colorFg;
    return Semantics(
      button: true,
      label: label,
      child: Material(
        color: primary ? colorAccent : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            height: 48,
            decoration: primary
                ? null
                : BoxDecoration(
                    border: Border.all(color: colorDotted, width: 1.5),
                  ),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(icon, size: 16, color: foreground),
                const SizedBox(width: 8),
                Text(
                  label,
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.08 * 10,
                    color: foreground,
                  ),
                ),
              ],
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
  letterSpacing: 0.08 * 9,
  color: colorFg3,
);

const _accentMetaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.08 * 9,
  color: colorAccent,
);
