import 'dart:convert';

import 'package:flutter/services.dart';

import 'wiki_page.dart';

class WikiRepository {
  static const indexAsset = 'assets/wiki/index.json';
  static const defaultCountryCode = 'GB';

  final AssetBundle bundle;

  WikiRepository({AssetBundle? bundle}) : bundle = bundle ?? rootBundle;

  Future<WikiCatalog> load({String? countryCode}) async {
    final normalizedCountry = _normalizeCountry(countryCode);
    final raw = await bundle.loadString(indexAsset);
    final decoded = jsonDecode(raw) as Map<String, dynamic>;
    final digest = decoded['corpusDigest'];
    if (decoded['schemaVersion'] != 1 ||
        digest is! String ||
        !RegExp(r'^[a-f0-9]{64}$').hasMatch(digest)) {
      throw const FormatException('Unsupported or malformed wiki corpus');
    }
    final pages =
        (decoded['pages'] as List<dynamic>)
            .map((value) => WikiPage.fromJson(value as Map<String, dynamic>))
            .where((page) => page.appliesToCountry(normalizedCountry))
            .toList(growable: false)
          ..sort(_defaultSort);
    final supportedCountries = List<String>.from(
      decoded['supportedCountries'] as List<dynamic>,
    );
    final generatedRecords = {
      for (final value in decoded['generatedRecords'] as List<dynamic>)
        (value as Map<String, dynamic>)['id'] as String:
            WikiGeneratedRecord.fromJson(value),
    };

    return WikiCatalog(
      pages: pages,
      generatedRecords: generatedRecords,
      countryCode: normalizedCountry,
      countrySupported: supportedCountries.contains(normalizedCountry),
    );
  }

  List<WikiPage> search(
    List<WikiPage> pages,
    String query, {
    String? category,
  }) {
    final terms = _searchTerms(query);
    final matches = <({WikiPage page, int score})>[];

    for (final page in pages) {
      if (category != null && page.category != category) continue;
      if (terms.isEmpty) {
        matches.add((page: page, score: 0));
        continue;
      }

      final title = _normalize(page.title);
      final aliases = _normalize(page.aliases.join(' '));
      final tags = _normalize(page.tags.join(' '));
      final summary = _normalize(page.summary);
      final body = _normalize(page.markdown);
      final searchable = '$title $aliases $tags $summary $body';
      if (!terms.every(searchable.contains)) continue;

      var score = 0;
      for (final term in terms) {
        if (title == term) score += 120;
        if (page.aliases.any((alias) => _normalize(alias) == term)) {
          score += 100;
        }
        if (title.contains(term)) score += 40;
        if (aliases.contains(term)) score += 30;
        if (tags.contains(term)) score += 20;
        if (summary.contains(term)) score += 10;
        if (body.contains(term)) score += 1;
      }
      matches.add((page: page, score: score));
    }

    matches.sort((left, right) {
      final score = right.score.compareTo(left.score);
      if (score != 0) return score;
      return _defaultSort(left.page, right.page);
    });
    return matches.map((match) => match.page).toList(growable: false);
  }

  static int _defaultSort(WikiPage left, WikiPage right) {
    final priority = _priorityRank(
      left.priority,
    ).compareTo(_priorityRank(right.priority));
    if (priority != 0) return priority;
    final order = left.order.compareTo(right.order);
    if (order != 0) return order;
    return left.title.compareTo(right.title);
  }

  static int _priorityRank(String priority) => switch (priority) {
    'critical' => 0,
    'high' => 1,
    _ => 2,
  };

  static String _normalizeCountry(String? value) {
    final country = value?.trim().toUpperCase() ?? '';
    return country.isEmpty ? defaultCountryCode : country;
  }

  static List<String> _searchTerms(String query) => _normalize(
    query,
  ).split(' ').where((term) => term.isNotEmpty).toList(growable: false);

  static String _normalize(String value) =>
      value.toLowerCase().replaceAll(RegExp(r'[^a-z0-9]+'), ' ').trim();
}
