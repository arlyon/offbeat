// OFFBEAT WeatherSheet — bottom sheet with hourly weather detail
// Hero: current conditions (temp, wind, precip, icon)
// Scrollable hourly strip with temp + icon + precip bars
// Updated-at timestamp

import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../src/rust/api/dto.dart';
import '../widgets/dotted_border.dart';
import 'weather_pill.dart' show wmoIcon;

class WeatherSheet extends StatelessWidget {
  final WeatherForecastDto forecast;
  final ScrollController scrollController;

  const WeatherSheet({
    super.key,
    required this.forecast,
    required this.scrollController,
  });

  @override
  Widget build(BuildContext context) {
    final hourly = forecast.hourly;
    final now = DateTime.now();
    final startIdx = _findStartIndex(hourly.time, now);

    return Column(
      children: [
        // Drag handle
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
        // Header
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 18),
          child: Row(
            children: [
              const Text(
                'WEATHER',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.1 * 11,
                  color: colorFg,
                  height: 1,
                ),
              ),
              const Spacer(),
              Text(
                forecast.timezone.toUpperCase(),
                style: const TextStyle(
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
        const SizedBox(height: 12),
        // Current conditions hero
        if (startIdx < hourly.time.length)
          _HeroCard(hourly: hourly, index: startIdx),
        const SizedBox(height: 4),
        // Hourly strip
        Expanded(
          child: ListView.builder(
            controller: scrollController,
            padding: const EdgeInsets.symmetric(horizontal: 18),
            itemCount: math.min(hourly.time.length - startIdx, 72), // ~3 days
            itemBuilder: (context, i) {
              final idx = startIdx + i;
              return _HourRow(hourly: hourly, index: idx, isFirst: i == 0);
            },
          ),
        ),
        // Updated-at footer
        DottedBorder.top(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
            child: Row(
              children: [
                Text(
                  'UPDATED ${_formatTimestamp(forecast.updatedAt)}',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.08 * 9,
                    color: colorFg4,
                    height: 1,
                  ),
                ),
                const Spacer(),
                const Text(
                  'OPEN-METEO',
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

class _HeroCard extends StatelessWidget {
  final HourlyWeatherDto hourly;
  final int index;

  const _HeroCard({required this.hourly, required this.index});

  @override
  Widget build(BuildContext context) {
    final temp = hourly.temperature2M[index];
    final precip = hourly.precipitationProbability[index];
    final wind = hourly.windSpeed10M[index];
    final code = hourly.weatherCode[index];

    return DottedBorder.bottom(
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
        decoration: BoxDecoration(
          gradient: RadialGradient(
            center: const Alignment(-0.6, -0.3),
            radius: 1.4,
            colors: [
              _tempColor(temp).withValues(alpha: 0.12),
              Colors.transparent,
            ],
          ),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Big temp + icon
            Text(
              wmoIcon(code),
              style: const TextStyle(fontSize: 36, height: 1),
            ),
            const SizedBox(width: 12),
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${temp.round()}°C',
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontWeight: FontWeight.w700,
                    fontSize: 32,
                    letterSpacing: -0.02 * 32,
                    height: 1,
                    color: colorFg,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  _weatherLabel(code),
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.08 * 10,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
              ],
            ),
            const Spacer(),
            // Stats column
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                _StatRow(label: 'PRECIP', value: '${precip.round()}%'),
                const SizedBox(height: 6),
                _StatRow(label: 'WIND', value: '${wind.round()} km/h'),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _StatRow extends StatelessWidget {
  final String label;
  final String value;

  const _StatRow({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          label,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 9,
            letterSpacing: 0.08 * 9,
            color: colorFg4,
            height: 1,
          ),
        ),
        const SizedBox(width: 6),
        Text(
          value,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 11,
            fontWeight: FontWeight.w600,
            color: colorFg2,
            height: 1,
          ),
        ),
      ],
    );
  }
}

class _HourRow extends StatelessWidget {
  final HourlyWeatherDto hourly;
  final int index;
  final bool isFirst;

  const _HourRow({
    required this.hourly,
    required this.index,
    required this.isFirst,
  });

  @override
  Widget build(BuildContext context) {
    final time = hourly.time[index]; // "2026-06-13T06:00"
    final temp = hourly.temperature2M[index];
    final precip = hourly.precipitationProbability[index];
    final code = hourly.weatherCode[index];
    final wind = hourly.windSpeed10M[index];

    // Parse hour and detect day boundaries
    final hour = time.length >= 13 ? time.substring(11, 13) : '??';
    final date = time.length >= 10 ? time.substring(0, 10) : '';
    final isDayBoundary = hour == '00' && !isFirst;

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        // Day separator
        if (isDayBoundary)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 8),
            decoration: const BoxDecoration(
              border: Border(top: BorderSide(color: colorHairline, width: 1)),
            ),
            child: Text(
              _formatDate(date),
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.1 * 9,
                color: colorFg3,
                height: 1,
              ),
            ),
          ),
        // Hour row
        SizedBox(
          height: 36,
          child: Row(
            children: [
              // Time
              SizedBox(
                width: 36,
                child: Text(
                  '$hour:00',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    color: isFirst ? colorAccent : colorFg3,
                    height: 1,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              // Icon
              SizedBox(
                width: 20,
                child: Text(
                  wmoIcon(code),
                  style: const TextStyle(fontSize: 14, height: 1),
                ),
              ),
              const SizedBox(width: 8),
              // Temp
              SizedBox(
                width: 40,
                child: Text(
                  '${temp.round()}°',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: colorFg,
                    height: 1,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              // Precip bar
              Expanded(child: _PrecipBar(percent: precip)),
              const SizedBox(width: 8),
              // Precip %
              SizedBox(
                width: 32,
                child: Text(
                  '${precip.round()}%',
                  textAlign: TextAlign.right,
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    color: precip > 50 ? colorCoAccent : colorFg4,
                    height: 1,
                  ),
                ),
              ),
              const SizedBox(width: 10),
              // Wind
              SizedBox(
                width: 28,
                child: Text(
                  '${wind.round()}',
                  textAlign: TextAlign.right,
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    color: wind > 30 ? colorWarn : colorFg4,
                    height: 1,
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _PrecipBar extends StatelessWidget {
  final double percent;

  const _PrecipBar({required this.percent});

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.maxWidth * (percent / 100).clamp(0.0, 1.0);
        return Align(
          alignment: Alignment.centerLeft,
          child: Container(
            height: 4,
            width: width,
            color: percent > 70
                ? colorCoAccent
                : percent > 40
                ? colorCoAccent.withValues(alpha: 0.6)
                : colorFg4.withValues(alpha: 0.4),
          ),
        );
      },
    );
  }
}

// ── Helpers ──────────────────────────────────────────────────

Color _tempColor(double temp) {
  if (temp > 30) return colorAccent; // Hot → magenta
  if (temp > 20) return colorWarn; // Warm → amber
  if (temp > 10) return colorCoAccent; // Mild → teal
  return const Color(0xFF6CA0DC); // Cool → blue
}

String _weatherLabel(int code) {
  if (code == 0) return 'CLEAR SKY';
  if (code == 1) return 'MAINLY CLEAR';
  if (code == 2) return 'PARTLY CLOUDY';
  if (code == 3) return 'OVERCAST';
  if (code <= 49) return 'FOG';
  if (code <= 55) return 'DRIZZLE';
  if (code <= 59) return 'FREEZING DRIZZLE';
  if (code <= 65) return 'RAIN';
  if (code <= 69) return 'FREEZING RAIN';
  if (code <= 75) return 'SNOW';
  if (code <= 77) return 'SNOW GRAINS';
  if (code <= 82) return 'RAIN SHOWERS';
  if (code <= 86) return 'SNOW SHOWERS';
  if (code <= 99) return 'THUNDERSTORM';
  return 'UNKNOWN';
}

String _formatTimestamp(String iso) {
  // "2026-06-13T12:00:00Z" → "13 JUN 12:00"
  try {
    final dt = DateTime.parse(iso);
    const months = [
      'JAN',
      'FEB',
      'MAR',
      'APR',
      'MAY',
      'JUN',
      'JUL',
      'AUG',
      'SEP',
      'OCT',
      'NOV',
      'DEC',
    ];
    return '${dt.day} ${months[dt.month - 1]} ${dt.hour.toString().padLeft(2, '0')}:${dt.minute.toString().padLeft(2, '0')}';
  } catch (_) {
    return iso;
  }
}

String _formatDate(String date) {
  // "2026-06-14" → "SAT 14 JUN"
  try {
    final dt = DateTime.parse(date);
    const days = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
    const months = [
      'JAN',
      'FEB',
      'MAR',
      'APR',
      'MAY',
      'JUN',
      'JUL',
      'AUG',
      'SEP',
      'OCT',
      'NOV',
      'DEC',
    ];
    return '${days[dt.weekday - 1]} ${dt.day} ${months[dt.month - 1]}';
  } catch (_) {
    return date;
  }
}

/// Find the index of the first hour at or after now.
int _findStartIndex(List<String> times, DateTime now) {
  final nowHour =
      '${now.year}-${now.month.toString().padLeft(2, '0')}-${now.day.toString().padLeft(2, '0')}T${now.hour.toString().padLeft(2, '0')}:00';

  for (int i = 0; i < times.length; i++) {
    if (times[i].compareTo(nowHour) >= 0) return i;
  }

  // All times are in the past — show from the end
  return times.isEmpty ? 0 : times.length - 1;
}
