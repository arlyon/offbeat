// OFFBEAT AppShell — Main scaffold
// Column: StatusBar → TopNav → Expanded(body) → TabBar

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import 'status_bar.dart';
import 'bottom_tab_bar.dart';

class AppShell extends StatefulWidget {
  const AppShell({super.key});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  AppTab _activeTab = AppTab.festivals;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: colorBg,
      body: SafeArea(
        top: false,
        bottom: false,
        child: Column(
          children: [
            const OffbeatStatusBar(),
            Expanded(
              child: Navigator(
                key: GlobalKey<NavigatorState>(),
                onGenerateRoute: (settings) {
                  return MaterialPageRoute(
                    builder: (_) => _tabBody(_activeTab),
                  );
                },
              ),
            ),
            OffbeatTabBar(
              activeTab: _activeTab,
              onTabChanged: (tab) {
                setState(() => _activeTab = tab);
              },
            ),
          ],
        ),
      ),
    );
  }

  Widget _tabBody(AppTab tab) {
    switch (tab) {
      case AppTab.festivals:
        // Imported dynamically to avoid circular deps
        return const _FestivalsTabPlaceholder();
      case AppTab.schedule:
        return const _PlaceholderBody(label: 'SCHEDULE');
      case AppTab.now:
        return const _PlaceholderBody(label: 'NOW');
      case AppTab.you:
        return const _PlaceholderBody(label: 'YOU');
    }
  }
}

class _FestivalsTabPlaceholder extends StatelessWidget {
  const _FestivalsTabPlaceholder();

  @override
  Widget build(BuildContext context) {
    // The actual FestivalListScreen is wired in main.dart via Navigator
    return const Center(
      child: Text(
        'FESTIVALS',
        style: TextStyle(
          fontFamily: 'JetBrainsMono',
          color: colorFg3,
          fontSize: 11,
        ),
      ),
    );
  }
}

class _PlaceholderBody extends StatelessWidget {
  final String label;
  const _PlaceholderBody({required this.label});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        label,
        style: const TextStyle(
          fontFamily: 'JetBrainsMono',
          color: colorFg3,
          fontSize: 11,
          letterSpacing: 0.08 * 11,
        ),
      ),
    );
  }
}
