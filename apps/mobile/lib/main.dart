// OFFBEAT Mobile App — Entry point
// MaterialApp with custom theme, AppShell wrapping FestivalListScreen
// Route to FestivalDetailScreen on festival tap

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'theme/app_theme.dart';
import 'theme/tokens.dart';
import 'shell/status_bar.dart';
import 'shell/bottom_tab_bar.dart';
import 'shell/top_nav.dart';
import 'data/mock_data.dart';
import 'screens/festival_list/festival_list_screen.dart';
import 'screens/festival_detail/festival_detail_screen.dart';
import 'screens/you/registration_screen.dart';
import 'screens/you/you_screen.dart';
import 'services/auth_service.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  SystemChrome.setSystemUIOverlayStyle(const SystemUiOverlayStyle(
    statusBarColor: Colors.transparent,
    statusBarBrightness: Brightness.dark,
    statusBarIconBrightness: Brightness.light,
  ));
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  runApp(const OffbeatApp());
}

class OffbeatApp extends StatelessWidget {
  const OffbeatApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'OFFBEAT',
      theme: buildAppTheme(),
      debugShowCheckedModeBanner: false,
      home: const _OffbeatShell(),
    );
  }
}

class _OffbeatShell extends StatefulWidget {
  const _OffbeatShell();

  @override
  State<_OffbeatShell> createState() => _OffbeatShellState();
}

class _OffbeatShellState extends State<_OffbeatShell> {
  AppTab _activeTab = AppTab.festivals;
  Festival? _selectedFestival;

  // Auth state — in a real app this would come from the Rust bridge.
  // For now, we track it in Dart state until FRB codegen is wired up.
  String _authState = 'unregistered'; // unregistered, valid, expiring, expired
  String? _authExpiresAt;
  String _userId = '';
  String _publicKeyHex = '';
  String? _displayName;

  final _authService = AuthService();

  Future<void> _handleRegister() async {
    // TODO: PRF derivation via Rust bridge once FRB is wired
    // For now, generate a random identity and register it
    _publicKeyHex = 'a' * 64; // placeholder until bridge is connected

    final attestation = await _authService.register(
      ed25519PublicKeyHex: _publicKeyHex,
    );

    // TODO: Store attestation via Rust bridge
    // AppNode.storeAttestation(attestation['message'], attestation['signature'], attestation['issuer']);

    setState(() {
      _authState = 'valid';
      _userId = _publicKeyHex.substring(0, 16);
    });
  }

  @override
  Widget build(BuildContext context) {
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: const SystemUiOverlayStyle(
        statusBarColor: Colors.transparent,
        statusBarIconBrightness: Brightness.light,
        systemNavigationBarColor: colorBg,
        systemNavigationBarIconBrightness: Brightness.light,
      ),
      child: Scaffold(
        backgroundColor: colorBg,
        body: Column(
          children: [
            // Custom status bar (sits above SafeArea)
            const OffbeatStatusBar(),
            // Body with SafeArea for notch/bottom insets
            Expanded(
              child: _buildBody(),
            ),
            // Bottom tab bar
            OffbeatTabBar(
              activeTab: _activeTab,
              onTabChanged: (tab) {
                setState(() {
                  _activeTab = tab;
                  // Reset detail when navigating to other tabs
                  if (tab != AppTab.festivals) _selectedFestival = null;
                });
              },
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBody() {
    switch (_activeTab) {
      case AppTab.festivals:
        if (_selectedFestival != null) {
          return FestivalDetailScreen(
            festival: _selectedFestival!,
            onBack: () => setState(() => _selectedFestival = null),
          );
        }
        return FestivalListScreen(
          onFestivalTap: (fest) => setState(() => _selectedFestival = fest),
        );
      case AppTab.schedule:
        return _PlaceholderTab(
          label: 'SCHEDULE',
          sublabel: 'Your saved sets across all festivals',
        );
      case AppTab.now:
        return NowTabPlaceholder();
      case AppTab.you:
        if (_authState == 'unregistered') {
          return RegistrationScreen(onRegister: _handleRegister);
        }
        return YouScreen(
          userId: _userId,
          publicKeyHex: _publicKeyHex,
          displayName: _displayName,
          authState: _authState,
          expiresAt: _authExpiresAt,
          onDisplayNameChanged: (name) {
            setState(() => _displayName = name);
            // TODO: AppNode.setDisplayName(name) via bridge
          },
        );
    }
  }
}

class _PlaceholderTab extends StatelessWidget {
  final String label;
  final String sublabel;

  const _PlaceholderTab({required this.label, required this.sublabel});

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        TopNav(),
        Expanded(
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  label,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.1 * 11,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 8),
                Text(
                  sublabel.toUpperCase(),
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.08 * 9,
                    color: colorFg4,
                    height: 1,
                  ),
                  textAlign: TextAlign.center,
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class NowTabPlaceholder extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    // Pre-wire the NowStrip view for the NOW tab
    return Column(
      children: [
        TopNav(
          rightWidgets: [NavIconButton(icon: Icons.search)],
        ),
        Expanded(
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 8,
                  height: 8,
                  decoration: const BoxDecoration(
                    shape: BoxShape.circle,
                    color: colorAccent,
                  ),
                ),
                const SizedBox(height: 8),
                const Text(
                  'NOW',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.1 * 11,
                    color: colorAccent,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 8),
                const Text(
                  'LIVE AT FIELD DAY',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.08 * 9,
                    color: colorFg4,
                    height: 1,
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
