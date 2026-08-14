import 'package:flutter/material.dart';

import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/screens/festival_detail/festival_detail_screen.dart';
import 'package:offbeat_mobile/screens/festival_detail/now_strip_view.dart';
import 'package:offbeat_mobile/screens/festival_list/festival_list_screen.dart';
import 'package:offbeat_mobile/shell/bottom_tab_bar.dart';
import 'package:offbeat_mobile/shell/top_nav.dart';
import 'package:offbeat_mobile/theme/app_theme.dart';
import 'package:offbeat_mobile/theme/tokens.dart';

const storeScreenshotKey = Key('store-screenshot');
final storeScreenshotNow = DateTime(2026, 8, 14, 17, 35);

enum StoreScreenshotScene { festivals, schedule, now, clashes }

class StoreScreenshotApp extends StatelessWidget {
  final StoreScreenshotScene scene;

  const StoreScreenshotApp({super.key, required this.scene});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      locale: const Locale('en', 'GB'),
      theme: buildAppTheme(),
      home: MediaQuery(
        data: const MediaQueryData(textScaler: TextScaler.noScaling),
        child: TickerMode(
          enabled: false,
          child: RepaintBoundary(
            key: storeScreenshotKey,
            child: _ScreenshotFrame(scene: scene),
          ),
        ),
      ),
    );
  }
}

class _ScreenshotFrame extends StatelessWidget {
  final StoreScreenshotScene scene;

  const _ScreenshotFrame({required this.scene});

  @override
  Widget build(BuildContext context) {
    if (scene == StoreScreenshotScene.festivals) {
      return Scaffold(
        backgroundColor: colorBg,
        body: Column(
          children: [
            const TopNav(
              relayConnected: true,
              blePeerCount: 2,
              rightWidgets: [
                NavIconButton(icon: Icons.health_and_safety_outlined),
                NavIconButton(icon: Icons.settings),
              ],
            ),
            Expanded(
              child: FestivalListScreen(
                festivals: storeFestivals,
                importRegistered: true,
                onFestivalTap: (_) {},
                onRegister: () async {},
                onPreviewClashfinder: (_) => Future.error(
                  UnsupportedError('not available in screenshot mode'),
                ),
                onPublishClashfinder:
                    ({
                      required previewId,
                      required name,
                      required location,
                      required city,
                      required country,
                    }) => Future.error(
                      UnsupportedError('not available in screenshot mode'),
                    ),
                onFestivalPublished: (_) async {},
              ),
            ),
          ],
        ),
      );
    }

    final activeTab = switch (scene) {
      StoreScreenshotScene.schedule ||
      StoreScreenshotScene.clashes => AppTab.schedule,
      StoreScreenshotScene.now => AppTab.now,
      StoreScreenshotScene.festivals => AppTab.schedule,
    };

    return Scaffold(
      backgroundColor: colorBg,
      body: Column(
        children: [
          const TopNav(
            festivalName: 'Lost Village',
            showBack: true,
            relayConnected: true,
            blePeerCount: 4,
            rightWidgets: [NavIconButton(icon: Icons.search)],
          ),
          Expanded(child: _festivalBody()),
          OffbeatTabBar(
            activeTab: activeTab,
            currentSetCount: 3,
            onTabChanged: (_) {},
          ),
        ],
      ),
    );
  }

  Widget _festivalBody() {
    switch (scene) {
      case StoreScreenshotScene.schedule:
        return FestivalDetailScreen(
          festival: storeFestival,
          now: storeScreenshotNow,
          stages: storeStages,
          days: storeDays,
          sets: storeSets,
          onStar: (_) {},
        );
      case StoreScreenshotScene.now:
        return NowStripView(
          sets: storeSets,
          stages: storeStages,
          days: storeDays,
          now: storeScreenshotNow,
        );
      case StoreScreenshotScene.clashes:
        return FestivalDetailScreen(
          festival: storeFestival,
          now: storeScreenshotNow,
          initialView: FestDetailView.clashRadar,
          stages: storeStages,
          days: storeDays,
          sets: storeSets,
          onStar: (_) {},
        );
      case StoreScreenshotScene.festivals:
        throw StateError('festival list has its own frame');
    }
  }
}

final storeFestival = Festival(
  id: 'lost-village-26',
  name: 'Lost Village',
  year: '2026',
  where: 'Lincolnshire Woodland',
  city: 'Lincoln',
  cc: 'GB',
  dates: '14–17 AUG 2026',
  dateRange: ['2026-08-14', '2026-08-15', '2026-08-16', '2026-08-17'],
  daysAway: 0,
  stages: 4,
  sets: 18,
  saved: 7,
  hue: 327,
  genres: ['Electronic', 'House', 'Disco'],
  startDate: DateTime(2020),
  endDate: DateTime(2099),
  headliners: ['Kelly Lee Owens', 'Floating Points', 'Overmono'],
);

final storeFestivals = [
  Festival(
    id: 'fieldday26',
    name: 'Field Day',
    year: '2026',
    where: 'Brockwell Park',
    city: 'London',
    cc: 'GB',
    dates: '23 MAY 2026',
    dateRange: ['2026-05-23'],
    daysAway: 42,
    stages: 6,
    sets: 58,
    saved: 11,
    hue: 335,
    genres: ['Electronic', 'Dance'],
    startDate: DateTime(2099, 5, 23),
    endDate: DateTime(2099, 5, 23),
    headliners: ['Floating Points', 'Avalon Emerson'],
  ),
  storeFestival,
  Festival(
    id: 'ade25',
    name: 'Amsterdam Dance Event',
    year: '2026',
    where: 'Across Amsterdam',
    city: 'Amsterdam',
    cc: 'NL',
    dates: '21–25 OCT 2026',
    dateRange: ['2026-10-21', '2026-10-22'],
    daysAway: 68,
    stages: 12,
    sets: 124,
    saved: 5,
    hue: 28,
    genres: ['Techno', 'House'],
    startDate: DateTime(2099, 10, 21),
    endDate: DateTime(2099, 10, 25),
    headliners: ['Helena Hauff', 'Ben UFO'],
  ),
  Festival(
    id: 'green-man-26',
    name: 'Green Man',
    year: '2026',
    where: 'Bannau Brycheiniog',
    city: 'Crickhowell',
    cc: 'GB',
    dates: '20–23 AUG 2026',
    dateRange: ['2026-08-20', '2026-08-21'],
    daysAway: 6,
    stages: 8,
    sets: 84,
    saved: 3,
    hue: 142,
    genres: ['Indie', 'Folk'],
    startDate: DateTime(2099, 8, 20),
    endDate: DateTime(2099, 8, 23),
    headliners: ['Big Thief', 'Mogwai'],
  ),
];

const storeStages = [
  Stage(id: 'village', name: 'The Village', short: 'VIL', color: 0xFFFF2D8F),
  Stage(id: 'airbase', name: 'Airbase', short: 'AIR', color: 0xFF37D6C0),
  Stage(id: 'junkyard', name: 'Junkyard', short: 'JNK', color: 0xFFFFB020),
  Stage(
    id: 'forgotten',
    name: 'Forgotten Cabin',
    short: 'CAB',
    color: 0xFF8C7CFF,
  ),
];

const storeDays = [
  Day(id: 'fri', label: 'FRIDAY', dayNum: '14', month: 'Aug', year: 2026),
  Day(id: 'sat', label: 'SATURDAY', dayNum: '15', month: 'Aug', year: 2026),
  Day(id: 'sun', label: 'SUNDAY', dayNum: '16', month: 'Aug', year: 2026),
];

final storeSets = withScheduleClashes([
  FestSet(
    id: 'kelly-lee-owens',
    day: 'fri',
    stage: 'village',
    artist: 'Kelly Lee Owens',
    t: 1020,
    dur: 90,
    genre: 'Electronic',
    starred: true,
    live: true,
    likedByGroup: true,
  ),
  FestSet(
    id: 'caribou',
    day: 'fri',
    stage: 'airbase',
    artist: 'Caribou',
    t: 1005,
    dur: 90,
    genre: 'Electronic',
    live: true,
  ),
  FestSet(
    id: 'shanti-celeste',
    day: 'fri',
    stage: 'junkyard',
    artist: 'Shanti Celeste',
    t: 1030,
    dur: 75,
    genre: 'House',
    live: true,
  ),
  FestSet(
    id: 'floating-points',
    day: 'fri',
    stage: 'village',
    artist: 'Floating Points',
    t: 1140,
    dur: 90,
    genre: 'Electronic',
    starred: true,
  ),
  FestSet(
    id: 'ben-ufo',
    day: 'fri',
    stage: 'airbase',
    artist: 'Ben UFO',
    t: 1125,
    dur: 120,
    genre: 'Dance',
    starred: true,
  ),
  FestSet(
    id: 'avalon-emerson',
    day: 'fri',
    stage: 'forgotten',
    artist: 'Avalon Emerson',
    t: 1110,
    dur: 90,
    genre: 'House',
  ),
  FestSet(
    id: 'overmono',
    day: 'fri',
    stage: 'junkyard',
    artist: 'Overmono',
    t: 1245,
    dur: 75,
    genre: 'Electronic',
  ),
  FestSet(
    id: 'helena-hauff',
    day: 'fri',
    stage: 'village',
    artist: 'Helena Hauff',
    t: 1320,
    dur: 90,
    genre: 'Techno',
    starred: true,
  ),
  FestSet(
    id: 'maribou-state',
    day: 'sat',
    stage: 'village',
    artist: 'Maribou State',
    t: 960,
    dur: 75,
    genre: 'Electronic',
    starred: true,
  ),
  FestSet(
    id: 'dj-seinfeld',
    day: 'sat',
    stage: 'airbase',
    artist: 'DJ Seinfeld',
    t: 1050,
    dur: 90,
    genre: 'House',
  ),
  FestSet(
    id: 'honey-dijon',
    day: 'sun',
    stage: 'village',
    artist: 'Honey Dijon',
    t: 1080,
    dur: 90,
    genre: 'House',
    starred: true,
  ),
]);
