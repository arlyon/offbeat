// OFFBEAT — Create/Join group bottom sheet
// Two tabs: CREATE / JOIN W/ CODE
// Matches groups-screens.jsx CreateGroupSheet (lines 520–630)

import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class CreateGroupSheet extends StatefulWidget {
  final String festivalName;
  final void Function(String name) onCreate;
  final void Function(String code) onJoin;
  final VoidCallback? onScanQr;

  const CreateGroupSheet({
    super.key,
    required this.festivalName,
    required this.onCreate,
    required this.onJoin,
    this.onScanQr,
  });

  @override
  State<CreateGroupSheet> createState() => _CreateGroupSheetState();
}

class _CreateGroupSheetState extends State<CreateGroupSheet> {
  String _tab = 'new'; // 'new' | 'join'
  final _nameController = TextEditingController();
  final _codeController = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _nameController.dispose();
    _codeController.dispose();
    super.dispose();
  }

  void _submit() {
    setState(() => _error = null);
    if (_tab == 'new') {
      final name = _nameController.text.trim();
      if (name.isEmpty) {
        setState(() => _error = 'GROUP NEEDS A NAME');
        return;
      }
      widget.onCreate(name);
    } else {
      final code = _codeController.text.trim().toUpperCase();
      if (!RegExp(r'^[A-Z0-9]{3}-[A-Z0-9]{3}$').hasMatch(code)) {
        setState(
          () => _error = 'CODE FORMAT: 3 CHARS \u2014 3 CHARS (E.G. 7K3-X9P)',
        );
        return;
      }
      widget.onJoin(code);
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.85,
      minChildSize: 0.5,
      maxChildSize: 0.95,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: Column(
          children: [
            // Grip
            Center(
              child: Container(
                margin: const EdgeInsets.only(top: 8),
                width: 36,
                height: 3,
                color: colorFg4,
              ),
            ),
            // Sheet header
            _buildHeader(),
            // Tab toggle
            _buildTabToggle(),
            // Content
            Expanded(
              child: SingleChildScrollView(
                controller: scrollController,
                child: Column(
                  children: [
                    _tab == 'new' ? _buildCreateTab() : _buildJoinTab(),
                    _buildScheduleSharingNotice(),
                  ],
                ),
              ),
            ),
            // Footer
            _buildFooter(),
          ],
        ),
      ),
    );
  }

  Widget _buildHeader() {
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 12, 18, 10),
        child: Row(
          children: [
            Expanded(
              child: Text.rich(
                TextSpan(
                  children: [
                    const TextSpan(text: 'NEW GROUP'),
                    TextSpan(
                      text: '//',
                      style: TextStyle(color: colorAccent),
                    ),
                  ],
                ),
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.08 * 11,
                  color: colorFg,
                ),
              ),
            ),
            GestureDetector(
              onTap: () => Navigator.pop(context),
              child: const SizedBox(
                width: 28,
                height: 28,
                child: Center(
                  child: Icon(Icons.close, size: 16, color: colorFg2),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildTabToggle() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 12, 18, 0),
      child: DottedBorder(
        child: Row(
          children: [
            _toggleButton('CREATE', 'new'),
            _toggleButton('JOIN W/ CODE', 'join'),
          ],
        ),
      ),
    );
  }

  Widget _toggleButton(String label, String tabValue) {
    final isActive = _tab == tabValue;
    return Expanded(
      child: GestureDetector(
        onTap: () => setState(() {
          _tab = tabValue;
          _error = null;
        }),
        child: Container(
          padding: const EdgeInsets.symmetric(vertical: 10),
          color: isActive ? colorFg : Colors.transparent,
          child: Center(
            child: Text(
              label,
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                letterSpacing: 0.08 * 10,
                color: isActive ? colorBg : colorFg3,
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildCreateTab() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Group name field
        DottedBorder.bottom(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'GROUP NAME',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w500,
                    letterSpacing: 0.08 * 10,
                    color: colorFg3,
                  ),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _nameController,
                  maxLength: 32,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontWeight: FontWeight.w700,
                    fontSize: 22,
                    letterSpacing: -0.02 * 22,
                    color: colorFg,
                  ),
                  decoration: InputDecoration(
                    border: InputBorder.none,
                    hintText: 'e.g. TENT 3',
                    hintStyle: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontWeight: FontWeight.w700,
                      fontSize: 22,
                      color: colorFg4,
                    ),
                    counterText: '',
                    isDense: true,
                    contentPadding: const EdgeInsets.only(bottom: 8),
                    enabledBorder: const UnderlineInputBorder(
                      borderSide: BorderSide(color: colorFg3, width: 1.5),
                    ),
                    focusedBorder: const UnderlineInputBorder(
                      borderSide: BorderSide(color: colorAccent, width: 1.5),
                    ),
                  ),
                  cursorColor: colorAccent,
                  onChanged: (_) => setState(() => _error = null),
                ),
                const SizedBox(height: 8),
                Text(
                  _error != null && _tab == 'new'
                      ? _error!
                      : '${_nameController.text.length}/32 \u00B7 MAX 12 MEMBERS PER GROUP',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.08 * 10,
                    color: _error != null && _tab == 'new'
                        ? colorErr
                        : colorFg4,
                  ),
                ),
              ],
            ),
          ),
        ),
        // Festival label
        Padding(
          padding: const EdgeInsets.fromLTRB(18, 16, 18, 8),
          child: const Text(
            'FESTIVAL',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              fontWeight: FontWeight.w500,
              letterSpacing: 0.08 * 10,
              color: colorFg3,
            ),
          ),
        ),
        // Festival picker (single item — current festival)
        DottedBorder(
          sides: const {DottedBorderSide.top, DottedBorderSide.bottom},
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
            child: Row(
              children: [
                Container(width: 20, height: 20, color: colorAccent),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        widget.festivalName,
                        style: const TextStyle(
                          fontFamily: 'Helvetica',
                          fontWeight: FontWeight.w700,
                          fontSize: 14,
                          letterSpacing: -0.01 * 14,
                          color: colorFg,
                        ),
                      ),
                    ],
                  ),
                ),
                const Text(
                  '\u25CF',
                  style: TextStyle(fontSize: 14, color: colorAccent),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildJoinTab() {
    return Column(
      children: [
        // Code input
        DottedBorder.bottom(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'GROUP CODE',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w500,
                    letterSpacing: 0.08 * 10,
                    color: colorFg3,
                  ),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _codeController,
                  maxLength: 7,
                  textCapitalization: TextCapitalization.characters,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontWeight: FontWeight.w700,
                    fontSize: 22,
                    letterSpacing: 0.05 * 22,
                    color: colorFg,
                  ),
                  decoration: const InputDecoration(
                    border: InputBorder.none,
                    hintText: '000-000',
                    hintStyle: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontWeight: FontWeight.w700,
                      fontSize: 22,
                      color: colorFg4,
                    ),
                    counterText: '',
                    isDense: true,
                    contentPadding: EdgeInsets.only(bottom: 8),
                    enabledBorder: UnderlineInputBorder(
                      borderSide: BorderSide(color: colorFg3, width: 1.5),
                    ),
                    focusedBorder: UnderlineInputBorder(
                      borderSide: BorderSide(color: colorAccent, width: 1.5),
                    ),
                  ),
                  cursorColor: colorAccent,
                  onChanged: (v) {
                    setState(() => _error = null);
                    // Auto-insert dash after 3 chars
                    if (v.length == 3 && !v.contains('-')) {
                      _codeController.text = '$v-';
                      _codeController.selection = TextSelection.fromPosition(
                        TextPosition(offset: _codeController.text.length),
                      );
                    }
                  },
                ),
                const SizedBox(height: 8),
                Text(
                  _error != null && _tab == 'join'
                      ? _error!
                      : 'ASK A FRIEND TO SHARE THEIR GROUP CODE',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.08 * 10,
                    color: _error != null && _tab == 'join'
                        ? colorErr
                        : colorFg4,
                  ),
                ),
              ],
            ),
          ),
        ),
        // OR divider
        const Padding(
          padding: EdgeInsets.symmetric(vertical: 20),
          child: Text(
            '\u2014 OR \u2014',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              letterSpacing: 0.05 * 11,
              color: colorFg3,
            ),
            textAlign: TextAlign.center,
          ),
        ),
        // Scan QR button
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 18),
          child: DottedBorder(
            color: colorFg3,
            child: SizedBox(
              width: double.infinity,
              child: Material(
                color: Colors.transparent,
                child: InkWell(
                  onTap: () {
                    Navigator.pop(context);
                    widget.onScanQr?.call();
                  },
                  child: const Padding(
                    padding: EdgeInsets.symmetric(vertical: 12, horizontal: 16),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(Icons.qr_code, size: 14, color: colorFg),
                        SizedBox(width: 8),
                        Text(
                          'SCAN QR CODE',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            fontWeight: FontWeight.w500,
                            letterSpacing: 0.08 * 11,
                            color: colorFg,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildScheduleSharingNotice() {
    return DottedBorder.top(
      child: const Padding(
        padding: EdgeInsets.symmetric(horizontal: 18, vertical: 14),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.sync, size: 14, color: colorAccent),
            SizedBox(width: 10),
            Expanded(
              child: Text(
                'YOUR SAVED SETS SYNC AUTOMATICALLY WITH GROUP MEMBERS',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.06 * 9,
                  color: colorFg2,
                  height: 1.4,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildFooter() {
    return DottedBorder.top(
      child: Container(
        color: colorSurface1,
        padding: const EdgeInsets.fromLTRB(18, 14, 18, 22),
        child: SizedBox(
          width: double.infinity,
          height: 44,
          child: Material(
            color: colorAccent,
            child: InkWell(
              onTap: _submit,
              child: Center(
                child: Text(
                  _tab == 'new' ? 'CREATE GROUP' : 'JOIN',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w500,
                    letterSpacing: 0.08 * 11,
                    color: colorAccentInk,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
