// OFFBEAT Mobile App — Entry point

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'theme/app_theme.dart';
import 'theme/tokens.dart';
import 'shell/status_bar.dart';
import 'shell/bottom_tab_bar.dart';
import 'shell/top_nav.dart';
import 'data/mock_data.dart';
import 'screens/festival_list/festival_list_screen.dart';
import 'screens/festival_detail/festival_detail_screen.dart';
import 'screens/festival_detail/admin_panel.dart';
import 'screens/you/registration_screen.dart';
import 'screens/you/you_screen.dart';
import 'services/auth_service.dart';
import 'services/admin_service.dart';
import 'src/rust/api.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
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

  // Rust bridge node
  AppNode? _node;
  bool _nodeReady = false;

  // Auth state
  String _authState = 'unregistered';
  String? _authExpiresAt;
  String _userId = '';
  String _publicKeyHex = '';
  String? _displayName;

  // Admin state
  bool _isAdmin = false;
  List<String> _adminKeys = [];

  final _authService = AuthService();
  final _adminService = AdminService();

  @override
  void initState() {
    super.initState();
    _initNode();
  }

  Future<void> _initNode() async {
    final dir = await getApplicationDocumentsDirectory();
    final dbPath = '${dir.path}/offbeat.db';
    final node = await AppNode.create(dbPath: dbPath);

    // Load existing auth state
    final authState = await node.getAuthState();
    final identity = await node.getIdentity();
    String pubKeyHex = '';
    if (authState.state != 'unregistered') {
      pubKeyHex = await node.getPublicKeyHex();
    }

    setState(() {
      _node = node;
      _nodeReady = true;
      _authState = authState.state;
      _authExpiresAt = authState.expiresAt;
      _userId = identity.userId;
      _displayName = identity.displayName;
      _publicKeyHex = pubKeyHex;
    });
  }

  Future<void> _handleRegister() async {
    final node = _node;
    if (node == null) return;

    final result = await _authService.register(
      onPrfOutput: (prfBytes) async {
        return await node.deriveIdentityFromPrf(prfOutput: prfBytes.toList());
      },
    );

    // Store attestation in Rust DB
    final att = result.attestation;
    await node.storeAttestation(
      message: att['message'] as String,
      signature: att['signature'] as String,
      issuer: att['issuer'] as String,
    );

    // Reload state
    final authState = await node.getAuthState();
    final identity = await node.getIdentity();

    setState(() {
      _authState = authState.state;
      _authExpiresAt = authState.expiresAt;
      _publicKeyHex = result.ed25519PublicKeyHex;
      _userId = identity.userId;
    });
  }

  Future<void> _onFestivalTap(Festival fest) async {
    setState(() => _selectedFestival = fest);

    // Check admin status if authenticated
    if (_authState != 'unregistered' && _publicKeyHex.isNotEmpty) {
      try {
        final admins = await _adminService.listAdmins();
        final isAdmin = admins.contains(_publicKeyHex);
        setState(() {
          _adminKeys = admins;
          _isAdmin = isAdmin;
        });

        // Show bootstrap dialog if no admins exist
        if (admins.isEmpty && mounted) {
          showDialog(
            context: context,
            builder: (_) => AdminBootstrapDialog(
              festivalName: fest.name,
              onAccept: () {
                Navigator.pop(context);
                _handleBecomeAdmin(fest.id);
              },
              onDecline: () => Navigator.pop(context),
            ),
          );
        }
      } catch (_) {
        // Server unreachable — continue without admin state
      }
    }
  }

  Future<void> _handleBecomeAdmin(String festivalId) async {
    if (_publicKeyHex.isEmpty) return;

    await _adminService.registerFestivalAdmin(
      festivalId: festivalId,
      publicKeyHex: _publicKeyHex,
    );

    // Also register as global admin
    await _adminService.registerAdmin(publicKeyHex: _publicKeyHex);

    // Refresh admin list
    final admins = await _adminService.listAdmins();
    setState(() {
      _isAdmin = admins.contains(_publicKeyHex);
      _adminKeys = admins;
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
            const OffbeatStatusBar(),
            Expanded(child: _buildBody()),
            OffbeatTabBar(
              activeTab: _activeTab,
              onTabChanged: (tab) {
                setState(() {
                  _activeTab = tab;
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
    if (!_nodeReady) {
      return const Center(
        child: CircularProgressIndicator(color: colorAccent, strokeWidth: 1.5),
      );
    }

    switch (_activeTab) {
      case AppTab.festivals:
        if (_selectedFestival != null) {
          return FestivalDetailScreen(
            festival: _selectedFestival!,
            onBack: () => setState(() => _selectedFestival = null),
            isAdmin: _isAdmin,
            adminKeys: _adminKeys,
            userPublicKeyHex: _publicKeyHex,
          );
        }
        return FestivalListScreen(
          onFestivalTap: (fest) => _onFestivalTap(fest),
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
          onDisplayNameChanged: (name) async {
            await _node?.setDisplayName(name: name);
            setState(() => _displayName = name);
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
