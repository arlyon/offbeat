import 'package:flutter/material.dart';

import '../../data/models.dart';
import '../../services/festival_import_service.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class ClashfinderImportPanel extends StatefulWidget {
  final bool registered;
  final Future<void> Function() onRegister;
  final Future<ClashfinderPreviewResult> Function(String source) onPreview;
  final Future<Festival> Function({
    required String previewId,
    required String name,
    required String location,
    required String city,
    required String country,
  })
  onPublish;
  final Future<void> Function(Festival festival) onPublished;
  final VoidCallback onClose;

  const ClashfinderImportPanel({
    super.key,
    required this.registered,
    required this.onRegister,
    required this.onPreview,
    required this.onPublish,
    required this.onPublished,
    required this.onClose,
  });

  @override
  State<ClashfinderImportPanel> createState() => _ClashfinderImportPanelState();
}

class _ClashfinderImportPanelState extends State<ClashfinderImportPanel> {
  final _sourceController = TextEditingController();
  final _nameController = TextEditingController();
  final _locationController = TextEditingController();
  final _cityController = TextEditingController();
  final _countryController = TextEditingController();

  ClashfinderPreview? _preview;
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _sourceController.dispose();
    _nameController.dispose();
    _locationController.dispose();
    _cityController.dispose();
    _countryController.dispose();
    super.dispose();
  }

  Future<void> _register() async {
    await _run(() async => widget.onRegister());
  }

  Future<void> _previewSource() async {
    final source = _sourceController.text.trim();
    if (source.isEmpty) {
      setState(() => _error = 'Paste a Clashfinder URL or event ID.');
      return;
    }
    await _run(() async {
      final result = await widget.onPreview(source);
      if (result.existingFestival case final existing?) {
        await widget.onPublished(existing);
        return;
      }
      final preview = result.preview;
      if (preview == null || !mounted) return;
      setState(() {
        _preview = preview;
        _nameController.text = preview.name;
      });
    });
  }

  Future<void> _publish() async {
    final preview = _preview;
    if (preview == null) return;
    final name = _nameController.text.trim();
    final location = _locationController.text.trim();
    final city = _cityController.text.trim();
    final country = _countryController.text.trim().toUpperCase();
    if (name.isEmpty ||
        location.isEmpty ||
        city.isEmpty ||
        country.length != 2) {
      setState(() {
        _error =
            'Name, venue, city, and a two-letter country code are required.';
      });
      return;
    }
    await _run(() async {
      final festival = await widget.onPublish(
        previewId: preview.id,
        name: name,
        location: location,
        city: city,
        country: country,
      );
      await widget.onPublished(festival);
    });
  }

  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (!mounted) return;
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 4, 18, 10),
      child: DottedBorder(
        color: colorAccent,
        child: Container(
          color: colorSurface1,
          padding: const EdgeInsets.fromLTRB(14, 14, 14, 16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  const Expanded(
                    child: Text('ADD FROM CLASHFINDER', style: _labelStyle),
                  ),
                  Semantics(
                    button: true,
                    label: 'Close Clashfinder import',
                    child: InkWell(
                      onTap: _busy ? null : widget.onClose,
                      child: const SizedBox(
                        width: 44,
                        height: 44,
                        child: Icon(Icons.close, size: 16, color: colorFg3),
                      ),
                    ),
                  ),
                ],
              ),
              if (_busy)
                const LinearProgressIndicator(
                  minHeight: 2,
                  color: colorAccent,
                  backgroundColor: colorSurface2,
                  semanticsLabel: 'Import in progress',
                ),
              if (_error != null) ...[
                const SizedBox(height: 10),
                _InlineError(message: _error!),
              ],
              const SizedBox(height: 10),
              if (!widget.registered)
                _RegistrationGate(onRegister: _busy ? null : _register)
              else if (_preview == null)
                _SourceStep(
                  controller: _sourceController,
                  enabled: !_busy,
                  onPreview: _busy ? null : _previewSource,
                )
              else
                _ConfirmStep(
                  preview: _preview!,
                  nameController: _nameController,
                  locationController: _locationController,
                  cityController: _cityController,
                  countryController: _countryController,
                  enabled: !_busy,
                  onBack: _busy
                      ? null
                      : () => setState(() {
                          _preview = null;
                          _error = null;
                        }),
                  onPublish: _busy ? null : _publish,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _RegistrationGate extends StatelessWidget {
  final VoidCallback? onRegister;
  const _RegistrationGate({required this.onRegister});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Register your device to add a public event. Registration prevents anonymous spam.',
          style: _bodyStyle,
        ),
        const SizedBox(height: 14),
        _ActionButton(label: 'REGISTER', onTap: onRegister),
      ],
    );
  }
}

class _SourceStep extends StatelessWidget {
  final TextEditingController controller;
  final bool enabled;
  final VoidCallback? onPreview;

  const _SourceStep({
    required this.controller,
    required this.enabled,
    required this.onPreview,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Paste a public Clashfinder link. We will validate the schedule before anything is published.',
          style: _bodyStyle,
        ),
        const SizedBox(height: 14),
        _LabeledField(
          label: 'CLASHFINDER URL OR ID',
          controller: controller,
          enabled: enabled,
          hint: 'clashfinder.com/s/event-name/',
          keyboardType: TextInputType.url,
          autofillHints: const [AutofillHints.url],
          onSubmitted: (_) => onPreview?.call(),
        ),
        const SizedBox(height: 14),
        _ActionButton(label: 'PREVIEW EVENT', onTap: onPreview),
      ],
    );
  }
}

class _ConfirmStep extends StatelessWidget {
  final ClashfinderPreview preview;
  final TextEditingController nameController;
  final TextEditingController locationController;
  final TextEditingController cityController;
  final TextEditingController countryController;
  final bool enabled;
  final VoidCallback? onBack;
  final VoidCallback? onPublish;

  const _ConfirmStep({
    required this.preview,
    required this.nameController,
    required this.locationController,
    required this.cityController,
    required this.countryController,
    required this.enabled,
    required this.onBack,
    required this.onPublish,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '${_date(preview.startDate)} → ${_date(preview.endDate)}  //  ${preview.stageCount} STAGES  //  ${preview.setCount} SETS',
          style: _metaStyle,
        ),
        const SizedBox(height: 14),
        _LabeledField(
          label: 'EVENT NAME',
          controller: nameController,
          enabled: enabled,
          maxLength: 200,
        ),
        const SizedBox(height: 10),
        _LabeledField(
          label: 'VENUE',
          controller: locationController,
          enabled: enabled,
          maxLength: 200,
        ),
        const SizedBox(height: 10),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: _LabeledField(
                label: 'CITY',
                controller: cityController,
                enabled: enabled,
                maxLength: 120,
              ),
            ),
            const SizedBox(width: 10),
            SizedBox(
              width: 92,
              child: _LabeledField(
                label: 'COUNTRY',
                controller: countryController,
                enabled: enabled,
                hint: 'GB',
                maxLength: 2,
                textCapitalization: TextCapitalization.characters,
              ),
            ),
          ],
        ),
        const SizedBox(height: 14),
        Row(
          children: [
            Expanded(
              child: _ActionButton(
                label: 'BACK',
                onTap: onBack,
                secondary: true,
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              flex: 2,
              child: _ActionButton(label: 'PUBLISH EVENT', onTap: onPublish),
            ),
          ],
        ),
      ],
    );
  }

  static String _date(DateTime date) {
    final month = date.month.toString().padLeft(2, '0');
    final day = date.day.toString().padLeft(2, '0');
    return '${date.year}-$month-$day';
  }
}

class _LabeledField extends StatelessWidget {
  final String label;
  final TextEditingController controller;
  final bool enabled;
  final String? hint;
  final int? maxLength;
  final TextInputType? keyboardType;
  final List<String>? autofillHints;
  final TextCapitalization textCapitalization;
  final ValueChanged<String>? onSubmitted;

  const _LabeledField({
    required this.label,
    required this.controller,
    required this.enabled,
    this.hint,
    this.maxLength,
    this.keyboardType,
    this.autofillHints,
    this.textCapitalization = TextCapitalization.none,
    this.onSubmitted,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: _fieldLabelStyle),
        const SizedBox(height: 6),
        Semantics(
          label: label,
          textField: true,
          child: TextField(
            controller: controller,
            enabled: enabled,
            keyboardType: keyboardType,
            autofillHints: autofillHints,
            textCapitalization: textCapitalization,
            maxLength: maxLength,
            onSubmitted: onSubmitted,
            cursorColor: colorAccent,
            style: const TextStyle(
              fontFamily: 'Helvetica',
              fontSize: 15,
              color: colorFg,
            ),
            decoration: InputDecoration(
              isDense: true,
              counterText: '',
              hintText: hint,
              hintStyle: const TextStyle(color: colorFg4),
              filled: true,
              fillColor: colorBg,
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 12,
                vertical: 13,
              ),
              enabledBorder: const OutlineInputBorder(
                borderRadius: BorderRadius.zero,
                borderSide: BorderSide(color: colorHairline),
              ),
              focusedBorder: const OutlineInputBorder(
                borderRadius: BorderRadius.zero,
                borderSide: BorderSide(color: colorAccent, width: 1.5),
              ),
              disabledBorder: const OutlineInputBorder(
                borderRadius: BorderRadius.zero,
                borderSide: BorderSide(color: colorHairline),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _ActionButton extends StatelessWidget {
  final String label;
  final VoidCallback? onTap;
  final bool secondary;

  const _ActionButton({
    required this.label,
    required this.onTap,
    this.secondary = false,
  });

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      enabled: onTap != null,
      child: Material(
        color: onTap == null
            ? colorSurface2
            : secondary
            ? colorBg
            : colorAccent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 44),
            alignment: Alignment.center,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: secondary
                ? BoxDecoration(border: Border.all(color: colorHairline))
                : null,
            child: Text(
              label,
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 10,
                color: onTap == null
                    ? colorFg4
                    : secondary
                    ? colorFg2
                    : colorAccentInk,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _InlineError extends StatelessWidget {
  final String message;
  const _InlineError({required this.message});

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      liveRegion: true,
      label: 'Error: $message',
      child: ExcludeSemantics(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Icon(Icons.error_outline, size: 15, color: colorAccent),
            const SizedBox(width: 8),
            Expanded(child: Text(message, style: _errorStyle)),
          ],
        ),
      ),
    );
  }
}

const _labelStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 11,
  fontWeight: FontWeight.w700,
  color: colorFg,
  letterSpacing: 0.08 * 11,
);

const _fieldLabelStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w600,
  color: colorFg3,
  letterSpacing: 0.08 * 9,
);

const _bodyStyle = TextStyle(
  fontFamily: 'Helvetica',
  fontSize: 14,
  color: colorFg2,
  height: 1.4,
);

const _metaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  color: colorFg3,
  letterSpacing: 0.05 * 9,
  height: 1.4,
);

const _errorStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 10,
  color: colorAccent,
  height: 1.4,
);
