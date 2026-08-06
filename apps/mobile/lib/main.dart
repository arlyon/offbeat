// OFFBEAT Mobile App — Entry point

import 'dart:async';
import 'dart:io';
import 'package:app_links/app_links.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'config.dart';
import 'theme/app_theme.dart';
import 'theme/tokens.dart';
import 'shell/bottom_tab_bar.dart';
import 'shell/top_nav.dart';
import 'data/check_in_controller.dart';
import 'data/group_schedule_overlay.dart';
import 'data/models.dart';
import 'data/serial_keyed_queue.dart';
import 'screens/festival_list/festival_list_screen.dart';
import 'screens/festival_detail/festival_detail_screen.dart';
import 'screens/festival_detail/admin_panel.dart';
import 'screens/festival_detail/lineup_search_screen.dart';
import 'screens/festival_detail/now_strip_view.dart';
import 'screens/festival_detail/set_details_sheet.dart';
import 'screens/chat/public_chat_screen.dart';
import 'screens/you/registration_screen.dart';
import 'screens/you/you_screen.dart';
import 'screens/social/social_screen.dart';
import 'services/auth_service.dart';
import 'services/admin_service.dart';
import 'services/festival_admin_service.dart';
import 'services/festival_import_service.dart';
import 'services/festival_service.dart';
import 'services/bluetooth_service.dart';
import 'widgets/connection_drawer.dart';
import 'widgets/weather_pill.dart';
import 'src/rust/api.dart';
import 'src/rust/api/dto.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  SystemChrome.setSystemUIOverlayStyle(
    const SystemUiOverlayStyle(
      statusBarColor: Colors.transparent,
      statusBarBrightness: Brightness.dark,
      statusBarIconBrightness: Brightness.light,
    ),
  );
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

class _OffbeatShellState extends State<_OffbeatShell>
    with SingleTickerProviderStateMixin {
  // Navigation: null = lobby (festival list), non-null = inside festival
  Festival? _selectedFestival;
  AppTab _activeTab = AppTab.schedule;
  final _festivalContentKey = GlobalKey();
  final _socialActionsController = SocialActionsController();
  final _scheduleViewsByFestival = <String, FestDetailView>{};

  // Navigation animation
  late final AnimationController _navController;
  late final Animation<Offset> _slideIn;
  late final Animation<Offset> _slideOut;

  // Rust bridge node
  AppNode? _node;
  final ValueNotifier<TransportStatusDto?> _transportStatusNotifier =
      ValueNotifier(null);
  final ValueNotifier<SyncStatusDto?> _syncStatusNotifier = ValueNotifier(null);

  bool _nodeReady = false;

  // Auth state
  String _authState = 'unregistered';
  String? _authExpiresAt;
  String _userId = '';
  String _publicKeyHex = '';
  String? _displayName;

  // Festival state
  List<Festival> _festivals = [];
  bool _festivalsLoading = false;
  String? _festivalsError;
  DateTime? _festivalCacheFetchedAt;
  FestivalRegistryStore? _festivalRegistryStore;
  int _festivalLoadGeneration = 0;

  // Lineup state (subscription-based)
  StreamSubscription<LineupDto?>? _lineupSub;
  LineupDto? _lineup;
  bool _lineupLoading = true;

  // Starred set IDs (loaded from Rust SQLite)
  Set<String> _starredSetIds = {};
  final SerialKeyedQueue _starToggleQueue = SerialKeyedQueue();
  GroupScheduleOverlayController? _groupScheduleController;
  GroupScheduleOverlay _groupScheduleOverlay = GroupScheduleOverlay.empty;
  CheckInController? _checkInController;

  // Weather state
  StreamSubscription<WeatherForecastDto?>? _weatherSub;
  WeatherForecastDto? _weather;

  // Live clock (ticks every 60s for timeline)
  Timer? _clockTimer;
  DateTime _now = DateTime.now();

  // Sync status
  bool _isSyncing = false;

  // Transport status
  StreamSubscription<TransportStatusDto>? _transportSub;
  bool _relayConnected = false;
  String? _relayFestivalId;
  Timer? _relayRetryTimer;
  Duration _relayRetryDelay = const Duration(seconds: 1);
  int _relayConnectionGeneration = 0;
  int _blePeerCount = -1; // -1 = unavailable

  // Admin state
  bool _isAdmin = false;
  List<String> _adminKeys = [];
  List<AdminRequest> _pendingRequests = [];
  String _adminRequestStatus = ''; // '', 'pending', 'already_admin'

  // Deep link handling
  StreamSubscription<Uri>? _deepLinkSub;
  final _appLinks = AppLinks();

  final _authService = AuthService();
  final _adminService = AdminService();
  final _festivalAdminService = FestivalAdminService();
  final _festivalImportService = FestivalImportService();
  final _festivalService = FestivalService();

  // Brutalist motion curve: cubic-bezier(0.2, 0.7, 0.2, 1.0)
  static const _curve = Cubic(0.2, 0.7, 0.2, 1.0);

  @override
  void initState() {
    super.initState();
    _navController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 280),
    );
    _slideIn = Tween<Offset>(
      begin: const Offset(1.0, 0.0),
      end: Offset.zero,
    ).animate(CurvedAnimation(parent: _navController, curve: _curve));
    _slideOut = Tween<Offset>(
      begin: Offset.zero,
      end: const Offset(-0.3, 0.0),
    ).animate(CurvedAnimation(parent: _navController, curve: _curve));
    _clockTimer = Timer.periodic(const Duration(seconds: 60), (_) {
      if (mounted) setState(() => _now = DateTime.now());
    });
    unawaited(_initializeNodeAndFestivals());
    _initDeepLinks();
  }

  @override
  void dispose() {
    _clockTimer?.cancel();
    _lineupSub?.cancel();
    _weatherSub?.cancel();
    _groupScheduleController?.dispose();
    _checkInController?.dispose();
    _transportSub?.cancel();
    _relayRetryTimer?.cancel();
    _deepLinkSub?.cancel();
    _festivalService.dispose();
    _navController.dispose();
    super.dispose();
  }

  void _initDeepLinks() {
    // Handle links that opened the app
    _appLinks.getInitialLink().then((uri) {
      if (uri != null) _handleDeepLink(uri);
    });
    // Handle links while app is running
    _deepLinkSub = _appLinks.uriLinkStream.listen((uri) {
      _handleDeepLink(uri);
    });
  }

  void _handleDeepLink(Uri uri) {
    // Expected: offbeat://group/{festival_id}/{group_id}/{key}
    if (uri.scheme != 'offbeat') return;
    if (uri.host != 'group' && !uri.path.startsWith('/group/')) return;

    final node = _node;
    final festivalId = _selectedFestival?.id;
    if (node == null || festivalId == null || _authState == 'unregistered') {
      return;
    }

    final invitePayload = uri.toString();
    final displayName = _displayName ?? 'anon';

    node
        .joinGroup(
          invitePayload: invitePayload,
          festivalId: festivalId,
          displayName: displayName,
        )
        .then((result) {
          if (!mounted) return;
          // If we know the festival_id, navigate to that festival
          if (result.festivalId.isNotEmpty) {
            final fest = _festivals.firstWhere(
              (f) => f.id == result.festivalId,
              orElse: () => _festivals.first,
            );
            _onFestivalTap(fest);
          }
        })
        .catchError((e) {
          debugPrint('deep link join failed: $e');
        });
  }

  Future<void> _initializeNodeAndFestivals() async {
    String? dbPath;
    try {
      final dir = await getApplicationDocumentsDirectory();
      dbPath = '${dir.path}/offbeat.db';
      final localStore = await FestivalRegistryCacheStore.open(dbPath: dbPath);
      if (!mounted) return;
      _festivalRegistryStore = LocalFestivalRegistryStore(localStore);
      await _hydrateFestivalCache();
    } catch (error) {
      debugPrint('Failed to initialize local festival cache: $error');
    }

    if (!mounted) return;
    setState(() => _nodeReady = true);
    unawaited(_loadFestivals(loadCache: false));

    try {
      await BluetoothService.requestPermissions();
      await BluetoothService.initBle();
      await _initNode(dbPath: dbPath);
    } catch (error) {
      debugPrint('Failed to initialize network node: $error');
    }
  }

  Future<void> _initNode({String? dbPath}) async {
    if (dbPath == null) {
      final dir = await getApplicationDocumentsDirectory();
      dbPath = '${dir.path}/offbeat.db';
    }
    final node = await AppNode.create(dbPath: dbPath);
    if (!mounted) return;

    setState(() {
      _node = node;
      _nodeReady = true;
    });

    try {
      await node.startBleSync();
    } catch (error) {
      debugPrint('BLE sync startup failed: $error');
    }

    final authState = await node.getAuthState();
    final identity = await node.getIdentity();
    String pubKeyHex = '';
    if (authState.state != 'unregistered') {
      pubKeyHex = await node.getPublicKeyHex();
    }

    List<String> admins = [];
    bool isAdmin = false;
    if (pubKeyHex.isNotEmpty) {
      try {
        admins = await _adminService.listAdmins().timeout(
          const Duration(seconds: 10),
        );
        isAdmin = admins.contains(pubKeyHex);
        debugPrint('Loaded ${admins.length} admins, isAdmin=$isAdmin');
      } catch (error) {
        debugPrint('Failed to load admins: $error');
      }
    }

    if (!mounted || _node != node) return;
    setState(() {
      _authState = authState.state;
      _authExpiresAt = authState.expiresAt;
      _userId = identity.userId;
      _displayName = identity.displayName;
      _publicKeyHex = pubKeyHex;
      _adminKeys = admins;
      _isAdmin = isAdmin;
      if (isAdmin) _adminRequestStatus = 'already_admin';
    });

    try {
      final syncStream = await node.watchSyncStatus();
      syncStream.listen((status) {
        if (mounted && _node == node) {
          setState(() {
            _isSyncing = status.syncing;
            _syncStatusNotifier.value = status;
          });
        }
      });
    } catch (error) {
      debugPrint('Sync status watcher failed: $error');
    }

    try {
      final transportStream = await node.watchTransportStatus();
      _transportSub = transportStream.listen((status) {
        if (mounted && _node == node) {
          setState(() {
            _transportStatusNotifier.value = status;
            _relayConnected = status.relay.connected;
            _blePeerCount = status.ble.active ? status.ble.peerCount : -1;
          });
        }
      });
    } catch (error) {
      debugPrint('Transport status watcher failed: $error');
    }
  }

  /// Restart the node to re-attempt BLE transport initialization.
  Future<void> _restartNode() async {
    _relayConnectionGeneration++;
    await _node?.disconnectRelay();
    _lineupSub?.cancel();
    _lineupSub = null;
    _weatherSub?.cancel();
    _weatherSub = null;
    _groupScheduleController?.dispose();
    _groupScheduleController = null;
    _groupScheduleOverlay = GroupScheduleOverlay.empty;
    _transportSub?.cancel();
    _transportSub = null;
    _relayRetryTimer?.cancel();
    _relayRetryTimer = null;
    _relayRetryDelay = const Duration(seconds: 1);
    _relayFestivalId = null;
    setState(() {
      _nodeReady = false;
      _relayConnected = false;
      _blePeerCount = -1;
      _selectedFestival = null;
      _lineup = null;
      _lineupLoading = true;
      _weather = null;
      _starredSetIds = {};
    });
    await _initNode();
  }

  Future<void> _hydrateFestivalCache() async {
    final store = _festivalRegistryStore;
    if (store == null) return;
    try {
      final cached = await _festivalService.loadCachedFestivals(store);
      if (cached != null && mounted) {
        setState(() {
          _festivals = cached.festivals;
          _festivalCacheFetchedAt = cached.fetchedAt;
          _festivalsError =
              'cached copy // updated ${_formatCacheTime(cached.fetchedAt)}';
        });
      }
    } catch (error) {
      debugPrint('Failed to load cached festival registry: $error');
    }
  }

  Future<void> _loadFestivals({bool loadCache = true}) async {
    final generation = ++_festivalLoadGeneration;
    bool isCurrent() => mounted && generation == _festivalLoadGeneration;
    setState(() {
      _festivalsLoading = true;
      final cachedAt = _festivalCacheFetchedAt;
      _festivalsError = cachedAt == null
          ? null
          : 'cached copy // updated ${_formatCacheTime(cachedAt)} // checking for updates';
    });

    final store = _festivalRegistryStore;
    if (loadCache && _festivals.isEmpty && store != null) {
      try {
        final cached = await _festivalService.loadCachedFestivals(store);
        if (cached != null && isCurrent()) {
          setState(() {
            _festivals = cached.festivals;
            _festivalCacheFetchedAt = cached.fetchedAt;
            _festivalsError =
                'cached copy // updated ${_formatCacheTime(cached.fetchedAt)} // checking for updates';
          });
        }
      } catch (error) {
        debugPrint('Failed to load cached festival registry: $error');
      }
    }

    try {
      final refreshed = await _festivalService.refreshFestivals(store: store);
      if (!isCurrent()) return;
      setState(() {
        _festivals = refreshed.festivals;
        _festivalsLoading = false;
        if (refreshed.persisted) {
          _festivalCacheFetchedAt = refreshed.fetchedAt;
          _festivalsError = null;
        } else {
          _festivalsError = 'online // offline cache unavailable';
        }
      });
    } catch (error) {
      if (!isCurrent()) return;
      final cachedAt = _festivalCacheFetchedAt;
      setState(() {
        _festivalsLoading = false;
        _festivalsError = cachedAt != null
            ? 'offline cache // updated ${_formatCacheTime(cachedAt)}'
            : _festivals.isEmpty
            ? 'offline — no cached data'
            : 'could not refresh';
      });
    }
  }

  String _formatCacheTime(DateTime value) {
    final local = value.toLocal();
    String twoDigits(int number) => number.toString().padLeft(2, '0');
    return '${local.year}-${twoDigits(local.month)}-${twoDigits(local.day)} '
        '${twoDigits(local.hour)}:${twoDigits(local.minute)}';
  }

  Future<ClashfinderPreviewResult> _previewClashfinder(String source) async {
    final node = _node;
    if (node == null) {
      throw const FestivalImportException('The local node is not ready.');
    }
    return _festivalImportService.preview(node: node, clashfinder: source);
  }

  Future<Festival> _publishClashfinder({
    required String previewId,
    required String name,
    required String location,
    required String city,
    required String country,
  }) async {
    final node = _node;
    if (node == null) {
      throw const FestivalImportException('The local node is not ready.');
    }
    return _festivalImportService.publish(
      node: node,
      previewId: previewId,
      name: name,
      location: location,
      city: city,
      country: country,
    );
  }

  Future<void> _openPublishedFestival(Festival festival) async {
    await _loadFestivals();
    final canonical = _festivals
        .where((entry) => entry.id == festival.id)
        .firstOrNull;
    await _onFestivalTap(canonical ?? festival);
  }

  Future<void> _handleRegister() async {
    final node = _node;
    if (node == null) return;

    final result = await _authService.register(
      onPrfOutput: (prfBytes) async {
        return await node.deriveIdentityFromPrf(prfOutput: prfBytes.toList());
      },
    );

    // Pin the server trust root before accepting its portable attestation.
    final rootKey = mainDoPublicKeyHex.isNotEmpty
        ? mainDoPublicKeyHex
        : await _authService.fetchServerPublicKey();
    await node.pinMainDoPublicKey(publicKeyHex: rootKey);

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
    final festivalId = _selectedFestival?.id;
    if (festivalId != null) {
      _startGroupScheduleOverlay(node, festivalId);
    }
  }

  Future<void> _handleLogout() async {
    _relayConnectionGeneration++;
    await _node?.disconnectRelay();
    // Delete the database and recreate the node
    final dir = await getApplicationDocumentsDirectory();
    final dbPath = '${dir.path}/offbeat.db';
    final dbFile = File(dbPath);
    if (await dbFile.exists()) {
      await dbFile.delete();
    }

    // Recreate a fresh node
    final node = await AppNode.create(dbPath: dbPath);

    _lineupSub?.cancel();
    _lineupSub = null;
    _weatherSub?.cancel();
    _weatherSub = null;
    _relayRetryTimer?.cancel();
    _relayRetryTimer = null;
    _relayRetryDelay = const Duration(seconds: 1);
    _relayFestivalId = null;
    setState(() {
      _node = node;
      _authState = 'unregistered';
      _authExpiresAt = null;
      _userId = '';
      _publicKeyHex = '';
      _displayName = null;
      _isAdmin = false;
      _adminKeys = [];
      _pendingRequests = [];
      _adminRequestStatus = '';
      _selectedFestival = null;
      _activeTab = AppTab.schedule;
      _lineup = null;
      _lineupLoading = true;
      _weather = null;
      _starredSetIds = {};
    });
  }

  Future<void> _onFestivalTap(Festival fest) async {
    final node = _node;
    if (node == null) return;
    final relayGeneration = ++_relayConnectionGeneration;

    // Cancel and await the previous festival relay before setup can continue.
    _lineupSub?.cancel();
    _weatherSub?.cancel();
    _relayRetryTimer?.cancel();
    _relayRetryTimer = null;
    _relayRetryDelay = const Duration(seconds: 1);
    await node.disconnectRelay();
    if (!mounted || relayGeneration != _relayConnectionGeneration) return;

    // Start animation and set state in the same frame so the lobby is
    // never removed before the slide transition begins.
    _navController.forward(from: 0.0);

    setState(() {
      _selectedFestival = fest;
      _lineup = null;
      _lineupLoading = true;
      _weather = null;
      _starredSetIds = {};
      _groupScheduleOverlay = GroupScheduleOverlay.empty;
    });
    _startGroupScheduleOverlay(node, fest.id);
    _checkInController?.dispose();
    _checkInController = CheckInController(node: node, festivalId: fest.id);
    unawaited(_checkInController!.load());

    // Load persisted stars from Rust SQLite
    try {
      final stars = await node.getStars(festivalId: fest.id);
      if (mounted && relayGeneration == _relayConnectionGeneration) {
        setState(() => _starredSetIds = stars.toSet());
      }
    } catch (_) {}

    // Start watching the lineup stream
    final stream = await node.watchLineup(festivalId: fest.id);
    if (!mounted || relayGeneration != _relayConnectionGeneration) {
      await stream.listen((_) {}).cancel();
      return;
    }
    _lineupSub = stream.listen((lineup) {
      if (mounted && relayGeneration == _relayConnectionGeneration) {
        setState(() {
          _lineup = lineup;
          _lineupLoading = false;
        });
      }
    });

    // Start watching weather
    final weatherStream = await node.watchWeather(festivalId: fest.id);
    if (!mounted || relayGeneration != _relayConnectionGeneration) {
      await weatherStream.listen((_) {}).cancel();
      return;
    }
    _weatherSub = weatherStream.listen((weather) {
      if (mounted && relayGeneration == _relayConnectionGeneration) {
        setState(() => _weather = weather);
      }
    });

    // Connect to the Festival DO WebSocket relay in the background.
    unawaited(_connectToRelay(fest.id));

    // Check admin status if authenticated
    if (_authState != 'unregistered' && _publicKeyHex.isNotEmpty) {
      try {
        final admins = await _adminService.listAdmins();
        final isAdmin = admins.contains(_publicKeyHex);

        List<AdminRequest> requests = [];
        if (isAdmin) {
          try {
            requests = await _adminService.listAdminRequests();
          } catch (_) {}
        }

        if (!mounted || relayGeneration != _relayConnectionGeneration) return;
        setState(() {
          _adminKeys = admins;
          _isAdmin = isAdmin;
          _pendingRequests = requests;
          if (isAdmin) _adminRequestStatus = 'already_admin';
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

  /// Connect to the Festival DO WebSocket relay and subscribe to updates.
  /// The lineup stream will automatically emit updates as they arrive.
  Future<void> _connectToRelay(String festivalId) async {
    final node = _node;
    final generation = _relayConnectionGeneration;
    bool isCurrent() =>
        mounted &&
        generation == _relayConnectionGeneration &&
        _selectedFestival?.id == festivalId;
    if (node == null || !isCurrent()) return;
    if (_relayFestivalId == festivalId) return;
    _relayFestivalId = festivalId;

    try {
      // Fetch the festival DO's public key BEFORE connecting so the
      // receive loop can verify signed updates in the catchup.
      final pubKeyHex = await _festivalService.fetchFestivalPublicKey(
        festivalId,
      );
      if (!isCurrent()) return;
      if (pubKeyHex != null) {
        await node.setFestivalPublicKey(
          festivalId: festivalId,
          hexKey: pubKeyHex,
        );
        if (!isCurrent()) return;
      }

      // Connect WS relay to the Festival DO
      final wsScheme = mainDoBaseUrl.startsWith('https') ? 'wss' : 'ws';
      final authority = mainDoBaseUrl.replaceFirst(RegExp(r'^https?://'), '');
      final wsUrl = '$wsScheme://$authority/festivals/$festivalId/ws';
      await node.connectRelay(url: wsUrl, festivalId: festivalId);
      if (!isCurrent()) return;

      // Subscribe to the state topic and request catchup from seq 0
      await node.subscribeFestival(festivalId: festivalId);
      if (!isCurrent()) return;

      // Subscribe to all group topics for this festival
      await node.subscribeGroups(festivalId: festivalId);
      if (!isCurrent()) return;
      _relayRetryTimer?.cancel();
      _relayRetryTimer = null;
      _relayRetryDelay = const Duration(seconds: 1);
    } catch (e, st) {
      if (!isCurrent()) return;
      if (_relayFestivalId == festivalId) {
        _relayFestivalId = null;
      }
      debugPrint('relay error: $e\n$st');
      _scheduleRelayRetry(festivalId);
    }
  }

  void _scheduleRelayRetry(String festivalId) {
    final generation = _relayConnectionGeneration;
    if (!mounted ||
        _selectedFestival?.id != festivalId ||
        _relayRetryTimer?.isActive == true) {
      return;
    }
    final delay = _relayRetryDelay;
    _relayRetryDelay = Duration(
      seconds: (_relayRetryDelay.inSeconds * 2).clamp(1, 30),
    );
    _relayRetryTimer = Timer(delay, () {
      if (mounted &&
          generation == _relayConnectionGeneration &&
          _selectedFestival?.id == festivalId) {
        unawaited(_connectToRelay(festivalId));
      }
    });
  }

  Future<void> _handleBecomeAdmin(String festivalId) async {
    if (_publicKeyHex.isEmpty) return;

    await _festivalAdminService.registerFestivalAdmin(
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
      _adminRequestStatus = 'already_admin';
    });
  }

  Future<void> _handleRequestAdmin() async {
    if (_publicKeyHex.isEmpty) return;
    try {
      // If no admins exist, bootstrap directly (first admin is auto-accepted)
      if (_adminKeys.isEmpty) {
        await _adminService.registerAdmin(publicKeyHex: _publicKeyHex);
        final admins = await _adminService.listAdmins();
        setState(() {
          _adminKeys = admins;
          _isAdmin = admins.contains(_publicKeyHex);
          _adminRequestStatus = 'already_admin';
        });
        return;
      }
      final status = await _adminService.requestAdmin(
        publicKeyHex: _publicKeyHex,
        displayName: _displayName,
      );
      setState(() => _adminRequestStatus = status);
    } catch (_) {
      // Server unreachable
    }
  }

  Future<void> _handleApproveRequest(String key) async {
    if (_publicKeyHex.isEmpty || _node == null) return;
    try {
      // Sign the approve path
      final path = '/admins/requests/$key/approve';
      final sig = await _node!.signMessage(message: path);
      await _adminService.approveAdminRequest(
        publicKeyHex: key,
        adminKeyHex: _publicKeyHex,
        adminSigHex: sig,
      );
      // Refresh
      final admins = await _adminService.listAdmins();
      final requests = await _adminService.listAdminRequests();
      setState(() {
        _adminKeys = admins;
        _pendingRequests = requests;
      });
    } catch (_) {}
  }

  Future<void> _handleDenyRequest(String key) async {
    if (_publicKeyHex.isEmpty || _node == null) return;
    try {
      final path = '/admins/requests/$key/deny';
      final sig = await _node!.signMessage(message: path);
      await _adminService.denyAdminRequest(
        publicKeyHex: key,
        adminKeyHex: _publicKeyHex,
        adminSigHex: sig,
      );
      // Refresh
      final requests = await _adminService.listAdminRequests();
      setState(() => _pendingRequests = requests);
    } catch (_) {}
  }

  Future<void> _refreshAdminStatus() async {
    if (_publicKeyHex.isEmpty) return;
    try {
      final admins = await _adminService.listAdmins();
      if (!mounted) return;
      setState(() {
        _adminKeys = admins;
        _isAdmin = admins.contains(_publicKeyHex);
        if (_isAdmin) _adminRequestStatus = 'already_admin';
      });
    } catch (e) {
      debugPrint('Failed to refresh admins: $e');
    }
  }

  void _showSettingsSheet() {
    _refreshAdminStatus();
    showModalBottomSheet(
      context: context,
      backgroundColor: colorBg,
      isScrollControlled: true,
      builder: (_) => DraggableScrollableSheet(
        initialChildSize: 0.85,
        minChildSize: 0.5,
        maxChildSize: 0.95,
        expand: false,
        builder: (context, scrollController) => Column(
          children: [
            // Handle bar
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: Container(
                width: 32,
                height: 3,
                decoration: BoxDecoration(
                  color: colorFg4,
                  borderRadius: BorderRadius.circular(1.5),
                ),
              ),
            ),
            Expanded(child: _buildYouContent()),
          ],
        ),
      ),
    );
  }

  Widget _buildYouContent() {
    if (_authState == 'unregistered') {
      return RegistrationScreen(onRegister: _handleRegister);
    }
    return YouScreen(
      userId: _userId,
      publicKeyHex: _publicKeyHex,
      displayName: _displayName,
      authState: _authState,
      expiresAt: _authExpiresAt,
      isAdmin: _isAdmin,
      adminRequestStatus: _adminRequestStatus,
      adminKeys: _adminKeys,
      node: _node,
      currentFestivalId: _selectedFestival?.id,
      onDisplayNameChanged: (name) async {
        await _node?.setDisplayName(name: name);
        setState(() => _displayName = name);
      },
      onRequestAdmin: _handleRequestAdmin,
      onLogout: () async {
        // Close the settings sheet if open, then log out
        if (Navigator.of(context).canPop()) {
          Navigator.of(context).pop();
        }
        await _handleLogout();
      },
    );
  }

  Future<void> _navigateBack() async {
    final generation = ++_relayConnectionGeneration;
    final node = _node;
    if (node != null) {
      await node.disconnectRelay();
    }
    if (!mounted || generation != _relayConnectionGeneration) return;
    _lineupSub?.cancel();
    _lineupSub = null;
    _weatherSub?.cancel();
    _weatherSub = null;
    _groupScheduleController?.dispose();
    _groupScheduleController = null;
    _groupScheduleOverlay = GroupScheduleOverlay.empty;
    _relayRetryTimer?.cancel();
    _relayRetryTimer = null;
    _relayRetryDelay = const Duration(seconds: 1);
    _relayFestivalId = null;
    _navController.reverse().then((_) {
      if (!mounted) return;
      setState(() {
        _selectedFestival = null;
        _activeTab = AppTab.schedule;
        _lineup = null;
        _lineupLoading = true;
        _weather = null;
        _starredSetIds = {};
      });
    });
  }

  @override
  Widget build(BuildContext context) {
    final inFestival = _selectedFestival != null;

    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: const SystemUiOverlayStyle(
        statusBarColor: Colors.transparent,
        statusBarIconBrightness: Brightness.light,
        systemNavigationBarColor: colorBg,
        systemNavigationBarIconBrightness: Brightness.light,
      ),
      child: PopScope(
        canPop: !inFestival,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop && inFestival) {
            unawaited(_navigateBack());
          }
        },
        child: Scaffold(
          backgroundColor: colorBg,
          body: Column(
            children: [
              // Shell-level TopNav with animation
              TopNav(
                festivalName: _selectedFestival?.name,
                showBack: inFestival,
                onBack: () => unawaited(_navigateBack()),
                animation: _navController,
                syncing: _isSyncing,
                relayConnected: _relayConnected,
                blePeerCount: _blePeerCount,
                onConnectionTap: () => showConnectionDrawer(
                  context,
                  transportStatus: _transportStatusNotifier,
                  syncStatus: _syncStatusNotifier,
                  onStartBle: _restartNode,
                  onConnectPeer: (deviceId) =>
                      _node?.connectPeer(deviceId: deviceId),
                  onNudgeGossip: () => _node?.nudgeGossip(),
                  onRestartBle: () => _node?.restartBle(),
                ),
                rightWidgets: [
                  // Crossfade between settings (lobby) and search+admin (festival)
                  AnimatedBuilder(
                    animation: _navController,
                    builder: (context, _) {
                      final t = _navController.value;
                      return Stack(
                        children: [
                          // Settings button (lobby)
                          IgnorePointer(
                            ignoring: t >= 0.5,
                            child: Opacity(
                              opacity: 1.0 - t,
                              child: NavIconButton(
                                icon: Icons.settings,
                                onTap: _showSettingsSheet,
                              ),
                            ),
                          ),
                          // Search + Admin (festival)
                          IgnorePointer(
                            ignoring: t < 0.5,
                            child: Opacity(
                              opacity: t,
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (_activeTab == AppTab.social &&
                                      _authState != 'unregistered')
                                    NavIconButton(
                                      icon: Icons.more_vert,
                                      onTap: _socialActionsController
                                          .openGroupActions,
                                    )
                                  else if (_activeTab != AppTab.social)
                                    NavIconButton(
                                      icon: Icons.search,
                                      onTap: _openLineupSearch,
                                    ),
                                  if (_isAdmin)
                                    NavIconButton(
                                      icon: Icons.shield,
                                      color: colorAccent,
                                      onTap: () => _showAdminPanel(context),
                                    ),
                                ],
                              ),
                            ),
                          ),
                        ],
                      );
                    },
                  ),
                  // Weather pill (only visible inside festival when forecast available)
                  if (_weather != null && _weather!.hourly.time.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(left: 2),
                      child: WeatherPill(forecast: _weather!),
                    ),
                ],
              ),
              // Body with slide animation
              Expanded(child: _buildAnimatedBody()),
              // Bottom tab bar — slides up when entering festival
              AnimatedBuilder(
                animation: _navController,
                builder: (context, child) {
                  final showBar = inFestival || _navController.isAnimating;
                  if (!showBar) return const SizedBox.shrink();
                  return SlideTransition(
                    position:
                        Tween<Offset>(
                          begin: const Offset(0.0, 1.0),
                          end: Offset.zero,
                        ).animate(
                          CurvedAnimation(
                            parent: _navController,
                            curve: _curve,
                          ),
                        ),
                    child: child,
                  );
                },
                child: OffbeatTabBar(
                  activeTab: _activeTab,
                  currentSetCount: _currentSetCount,
                  onTabChanged: (tab) => setState(() => _activeTab = tab),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  void _openPublicChat({required String channelId, required String title}) {
    final node = _node;
    final festival = _selectedFestival;
    if (node == null || festival == null) return;
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => PublicChatScreen(
          node: node,
          festivalId: festival.id,
          channelId: channelId,
          title: title,
          userId: _userId,
          checkInController: _checkInController,
        ),
      ),
    );
  }

  void _showAdminPanel(BuildContext context) {
    final fest = _selectedFestival;
    if (fest == null) return;
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      builder: (_) => AdminPanel(
        festivalId: fest.id,
        festivalName: fest.name,
        adminKeys: _adminKeys,
        userPublicKeyHex: _publicKeyHex,
        pendingRequests: _pendingRequests,
        onRefreshLineup: () => Navigator.pop(context),
        onExportSigningKey: () => Navigator.pop(context),
        onApproveRequest: (key) {
          Navigator.pop(context);
          _handleApproveRequest(key);
        },
        onDenyRequest: (key) {
          Navigator.pop(context);
          _handleDenyRequest(key);
        },
      ),
    );
  }

  Widget _buildAnimatedBody() {
    if (!_nodeReady) {
      return const Center(
        child: CircularProgressIndicator(color: colorAccent, strokeWidth: 1.5),
      );
    }

    final inFestival = _selectedFestival != null;

    // Always use the same widget tree structure to avoid remounts.
    // When not animating, the SlideTransitions are at rest positions
    // (Offset.zero for the visible one, off-screen for the hidden one).
    return AnimatedBuilder(
      animation: _navController,
      builder: (context, _) {
        return Stack(
          children: [
            // Lobby (slides out to left)
            if (!inFestival || _navController.isAnimating)
              SlideTransition(
                position: _slideOut,
                child: FestivalListScreen(
                  festivals: _festivals,
                  loading: _festivalsLoading,
                  error: _festivalsError,
                  onRefresh: _loadFestivals,
                  onFestivalTap: (fest) => _onFestivalTap(fest),
                  importRegistered:
                      _authState == 'valid' || _authState == 'expiring',
                  onRegister: _handleRegister,
                  onPreviewClashfinder: _previewClashfinder,
                  onPublishClashfinder: _publishClashfinder,
                  onFestivalPublished: _openPublishedFestival,
                ),
              ),
            // Festival (slides in from right)
            if (inFestival)
              SlideTransition(
                position: _slideIn,
                child: Container(
                  color: colorBg,
                  child: _buildFestivalContent(),
                ),
              ),
          ],
        );
      },
    );
  }

  Widget _buildFestivalContent() {
    switch (_activeTab) {
      case AppTab.schedule:
        return _buildScheduleTab();
      case AppTab.now:
        return _buildNowTab();
      case AppTab.social:
        if (_authState == 'unregistered') {
          return RegistrationScreen(onRegister: _handleRegister);
        }
        return SocialScreen(
          node: _node!,
          festivalId: _selectedFestival!.id,
          festivalName: _selectedFestival!.name,
          stages: {
            if (_lineup != null)
              for (final stage in _lineup!.stages) stage.id: stage.name,
          },
          userId: _userId,
          displayName: _displayName,
          lineup: _lineup,
          actionsController: _socialActionsController,
          checkInController: _checkInController,
          onGroupsChanged: () => _groupScheduleController?.refresh(),
        );
    }
  }

  void _startGroupScheduleOverlay(AppNode node, String festivalId) {
    _groupScheduleController?.dispose();
    final controller = GroupScheduleOverlayController(
      node: node,
      festivalId: festivalId,
      localUserId: _userId,
    );
    controller.addListener(() {
      if (!mounted || _selectedFestival?.id != festivalId) return;
      setState(() => _groupScheduleOverlay = controller.overlay);
    });
    _groupScheduleController = controller;
    unawaited(
      controller.refresh().catchError((Object error) {
        debugPrint('group schedule overlay failed: $error');
      }),
    );
  }

  void _handleStarToggle(String setId) {
    final node = _node;
    final festivalId = _selectedFestival?.id;
    if (node == null || festivalId == null) return;

    _starToggleQueue.enqueue('$festivalId/$setId', () async {
      try {
        await node.toggleStar(festivalId: festivalId, setId: setId);
        final persisted = await node.getStars(festivalId: festivalId);
        if (!mounted || _selectedFestival?.id != festivalId) return;
        setState(() => _starredSetIds = persisted.toSet());
      } catch (error) {
        debugPrint('star toggle failed: $error');
        if (!mounted || _selectedFestival?.id != festivalId) return;
        try {
          final persisted = await node.getStars(festivalId: festivalId);
          if (mounted && _selectedFestival?.id == festivalId) {
            setState(() => _starredSetIds = persisted.toSet());
          }
        } catch (_) {}
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('COULD NOT UPDATE MY SCHEDULE')),
          );
        }
      }
    });
  }

  ({List<Stage> stages, List<Day> days, List<FestSet> sets})? _lineupModels() {
    final lineup = _lineup;
    if (lineup == null) return null;

    final stages = lineup.stages
        .map(
          (stage) => Stage.fromJson({
            'id': stage.id,
            'name': stage.name,
            'short': stage.short,
            'color': stage.color,
            'order': stage.order,
          }),
        )
        .toList();
    final days = lineup.days
        .map(
          (day) => Day.fromJson({
            'id': day.id,
            'label': day.label,
            'num': day.num,
            'month': day.month,
            'year': day.year,
          }),
        )
        .toList();
    final dayById = {for (final day in days) day.id: day};
    final today = DateTime(_now.year, _now.month, _now.day);
    final currentMinute = _now.hour * 60 + _now.minute;
    final builtSets = lineup.sets.map((lineupSet) {
      final set = FestSet.fromJson({
        'id': lineupSet.id,
        'day': lineupSet.day,
        'stage': lineupSet.stage,
        'artist': lineupSet.artist,
        'startMin': lineupSet.startMin,
        'durationMin': lineupSet.durationMin,
        'genre': lineupSet.genre,
        'cancelled': lineupSet.cancelled,
      });
      final day = dayById[lineupSet.day];
      final live =
          day?.date == today &&
          !lineupSet.cancelled &&
          lineupSet.startMin <= currentMinute &&
          lineupSet.startMin + lineupSet.durationMin > currentMinute;
      return set.copyWith(
        live: live,
        starred: _starredSetIds.contains(lineupSet.id),
        likedByGroup: _groupScheduleOverlay.groupLikedSetIds.contains(
          lineupSet.id,
        ),
        supporters: _groupScheduleOverlay.supportersBySetId[lineupSet.id],
      );
    }).toList();

    return (stages: stages, days: days, sets: withScheduleClashes(builtSets));
  }

  int get _currentSetCount {
    final data = _lineupModels();
    if (data == null) return 0;
    final today = DateTime(_now.year, _now.month, _now.day);
    final day = data.days.where((day) => day.date == today).firstOrNull;
    if (day == null) return 0;
    final minute = _now.hour * 60 + _now.minute;
    return data.sets.where((set) {
      return set.day == day.id &&
          !set.cancelled &&
          set.t <= minute &&
          set.t + set.dur > minute;
    }).length;
  }

  void _showSetDetails(
    FestSet set,
    ({List<Stage> stages, List<Day> days, List<FestSet> sets}) data,
  ) {
    showSetDetailsSheet(
      context,
      set: set,
      stages: data.stages,
      days: data.days,
      allSets: data.sets,
      onStar: _handleStarToggle,
      onStageChat: (stage) =>
          _openPublicChat(channelId: stage.id, title: '${stage.name} chat'),
    );
  }

  void _openLineupSearch() {
    final data = _lineupModels();
    if (data == null) return;
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => LineupSearchScreen(
          sets: data.sets,
          stages: data.stages,
          days: data.days,
          onSetTap: (set) => _showSetDetails(set, data),
        ),
      ),
    );
  }

  Widget _buildScheduleTab() {
    final data = _lineupModels();
    return FestivalDetailScreen(
      key: _festivalContentKey,
      festival: _selectedFestival!,
      now: _now,
      initialView:
          _scheduleViewsByFestival[_selectedFestival!.id] ??
          FestDetailView.gantt,
      onViewChanged: (view) =>
          _scheduleViewsByFestival[_selectedFestival!.id] = view,
      stages: data?.stages,
      days: data?.days,
      sets: data?.sets,
      loading: _lineupLoading,
      onStar: _handleStarToggle,
      onStageChat: (stage) =>
          _openPublicChat(channelId: stage.id, title: '${stage.name} chat'),
    );
  }

  Widget _buildNowTab() {
    if (_lineupLoading) {
      return const Center(
        child: CircularProgressIndicator(color: colorAccent, strokeWidth: 1.5),
      );
    }
    final data = _lineupModels();
    if (data == null) {
      return const Center(
        child: Text(
          'NO LINEUP DATA',
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 11,
            fontWeight: FontWeight.w700,
            color: colorFg4,
          ),
        ),
      );
    }
    return NowStripView(
      sets: data.sets,
      stages: data.stages,
      days: data.days,
      now: _now,
      checkInController: _checkInController,
      onSetTap: (set) => _showSetDetails(set, data),
      onStageChat: (stage) =>
          _openPublicChat(channelId: stage.id, title: '${stage.name} chat'),
      onGlobalChat: () =>
          _openPublicChat(channelId: 'campsite', title: 'Camp chat'),
    );
  }
}
