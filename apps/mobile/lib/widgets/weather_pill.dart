// OFFBEAT WeatherPill — compact tappable pill for TopNav
// Shows current temp + WMO weather icon
// Taps open the full weather bottom sheet

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../src/rust/api/dto.dart';
import 'weather_sheet.dart';

/// Map WMO weather code to a unicode glyph.
///
/// WMO 4677 codes: https://www.nodc.noaa.gov/archive/arc0021/0002199/1.1/data/0-data/HTML/WMO-CODE/WMO4677.HTM
String wmoIcon(int code) {
  if (code == 0) return '☀'; // Clear sky
  if (code <= 3) return '⛅'; // Partly cloudy
  if (code <= 49) return '🌫'; // Fog / haze
  if (code <= 59) return '🌦'; // Drizzle
  if (code <= 69) return '🌧'; // Rain
  if (code <= 79) return '🌨'; // Snow
  if (code <= 84) return '🌧'; // Rain showers
  if (code <= 94) return '🌨'; // Snow showers
  return '⛈'; // Thunderstorm (95-99)
}

class WeatherPill extends StatelessWidget {
  final WeatherForecastDto forecast;

  const WeatherPill({super.key, required this.forecast});

  @override
  Widget build(BuildContext context) {
    // Find the current hour's data
    final now = DateTime.now();
    final hourly = forecast.hourly;
    int idx = _currentHourIndex(hourly.time, now);

    final temp = hourly.temperature2M[idx].round();
    final code = hourly.weatherCode[idx];
    final icon = wmoIcon(code);

    return GestureDetector(
      onTap: () => _openSheet(context),
      child: Container(
        height: 28,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: colorSurface2,
          border: Border.all(color: colorHairline, width: 1),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              icon,
              style: const TextStyle(fontSize: 13, height: 1),
            ),
            const SizedBox(width: 4),
            Text(
              '$temp°',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 11,
                fontWeight: FontWeight.w700,
                color: colorFg,
                height: 1,
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _openSheet(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: colorBg,
      isScrollControlled: true,
      builder: (_) => DraggableScrollableSheet(
        initialChildSize: 0.55,
        minChildSize: 0.3,
        maxChildSize: 0.85,
        expand: false,
        builder: (context, scrollController) => WeatherSheet(
          forecast: forecast,
          scrollController: scrollController,
        ),
      ),
    );
  }
}

/// Find the index of the current hour in the time array.
/// Falls back to 0 if no match found.
int _currentHourIndex(List<String> times, DateTime now) {
  // Times are like "2026-06-13T06:00"
  final nowHour = '${now.year}-${_p2(now.month)}-${_p2(now.day)}T${_p2(now.hour)}:00';

  for (int i = 0; i < times.length; i++) {
    if (times[i] == nowHour) return i;
  }

  // Fallback: find closest past hour
  for (int i = times.length - 1; i >= 0; i--) {
    if (times[i].compareTo(nowHour) <= 0) return i;
  }

  return 0;
}

String _p2(int n) => n.toString().padLeft(2, '0');
