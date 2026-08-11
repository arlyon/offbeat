import 'package:flutter/foundation.dart';

@immutable
class WikiSource {
  final String title;
  final String publisher;
  final String url;
  final String? revision;
  final String? license;

  const WikiSource({
    required this.title,
    required this.publisher,
    required this.url,
    this.revision,
    this.license,
  });

  factory WikiSource.fromJson(Map<String, dynamic> json) => WikiSource(
    title: json['title'] as String,
    publisher: json['publisher'] as String,
    url: json['url'] as String,
    revision: json['revision'] as String?,
    license: json['license'] as String?,
  );
}

@immutable
class WikiRange {
  final double? min;
  final double? max;
  final String? units;

  const WikiRange({this.min, this.max, this.units});

  factory WikiRange.fromJson(Map<String, dynamic>? json) => WikiRange(
    min: (json?['min'] as num?)?.toDouble(),
    max: (json?['max'] as num?)?.toDouble(),
    units: json?['units'] as String?,
  );

  bool get isEmpty => min == null && max == null;
}

@immutable
class WikiDoseReference {
  final String? units;
  final double? threshold;
  final WikiRange light;
  final WikiRange common;
  final WikiRange strong;
  final double? heavy;

  const WikiDoseReference({
    this.units,
    this.threshold,
    required this.light,
    required this.common,
    required this.strong,
    this.heavy,
  });

  factory WikiDoseReference.fromJson(Map<String, dynamic>? json) =>
      WikiDoseReference(
        units: json?['units'] as String?,
        threshold: (json?['threshold'] as num?)?.toDouble(),
        light: WikiRange.fromJson(json?['light'] as Map<String, dynamic>?),
        common: WikiRange.fromJson(json?['common'] as Map<String, dynamic>?),
        strong: WikiRange.fromJson(json?['strong'] as Map<String, dynamic>?),
        heavy: (json?['heavy'] as num?)?.toDouble(),
      );

  bool get isEmpty =>
      units == null &&
      threshold == null &&
      light.isEmpty &&
      common.isEmpty &&
      strong.isEmpty &&
      heavy == null;
}

@immutable
class WikiRouteReference {
  final String name;
  final WikiDoseReference dose;
  final WikiRange onset;
  final WikiRange duration;
  final WikiRange total;

  const WikiRouteReference({
    required this.name,
    required this.dose,
    required this.onset,
    required this.duration,
    required this.total,
  });

  factory WikiRouteReference.fromJson(Map<String, dynamic> json) {
    final duration = json['duration'] as Map<String, dynamic>?;
    return WikiRouteReference(
      name: json['name'] as String,
      dose: WikiDoseReference.fromJson(json['dose'] as Map<String, dynamic>?),
      onset: WikiRange.fromJson(duration?['onset'] as Map<String, dynamic>?),
      duration: WikiRange.fromJson(
        duration?['duration'] as Map<String, dynamic>?,
      ),
      total: WikiRange.fromJson(duration?['total'] as Map<String, dynamic>?),
    );
  }
}

@immutable
class WikiGeneratedRecord {
  final String id;
  final String sourceName;
  final String sourceUrl;
  final String? sourceRevision;
  final DateTime? sourceRevisionTimestamp;
  final DateTime retrievedAt;
  final String contentLicense;
  final String contentLicenseUrl;
  final String sourcePayloadSha256;
  final List<String> commonNames;
  final List<String> psychoactiveClasses;
  final List<WikiRouteReference> routes;

  const WikiGeneratedRecord({
    required this.id,
    required this.sourceName,
    required this.sourceUrl,
    this.sourceRevision,
    this.sourceRevisionTimestamp,
    required this.retrievedAt,
    required this.contentLicense,
    required this.contentLicenseUrl,
    required this.sourcePayloadSha256,
    required this.commonNames,
    required this.psychoactiveClasses,
    required this.routes,
  });

  factory WikiGeneratedRecord.fromJson(Map<String, dynamic> json) =>
      WikiGeneratedRecord(
        id: json['id'] as String,
        sourceName: json['sourceName'] as String,
        sourceUrl: json['sourceUrl'] as String,
        sourceRevision: json['sourceRevision'] as String?,
        sourceRevisionTimestamp: switch (json['sourceRevisionTimestamp']) {
          final String value => DateTime.parse(value),
          _ => null,
        },
        retrievedAt: DateTime.parse(json['retrievedAt'] as String),
        contentLicense: json['contentLicense'] as String,
        contentLicenseUrl: json['contentLicenseUrl'] as String,
        sourcePayloadSha256: json['sourcePayloadSha256'] as String,
        commonNames: List<String>.from(json['commonNames'] as List<dynamic>),
        psychoactiveClasses: List<String>.from(
          json['psychoactiveClasses'] as List<dynamic>,
        ),
        routes: (json['routes'] as List<dynamic>)
            .map(
              (value) =>
                  WikiRouteReference.fromJson(value as Map<String, dynamic>),
            )
            .toList(growable: false),
      );
}

@immutable
class WikiPage {
  final String id;
  final String locale;
  final String title;
  final String summary;
  final String category;
  final List<String> countryCodes;
  final List<String> aliases;
  final List<String> tags;
  final List<String> generatedRefs;
  final String priority;
  final int order;
  final DateTime lastVerified;
  final String contentStatus;
  final List<WikiSource> sources;
  final String markdown;

  const WikiPage({
    required this.id,
    required this.locale,
    required this.title,
    required this.summary,
    required this.category,
    required this.countryCodes,
    required this.aliases,
    required this.tags,
    required this.generatedRefs,
    required this.priority,
    required this.order,
    required this.lastVerified,
    required this.contentStatus,
    required this.sources,
    required this.markdown,
  });

  factory WikiPage.fromJson(Map<String, dynamic> json) => WikiPage(
    id: json['id'] as String,
    locale: json['locale'] as String,
    title: json['title'] as String,
    summary: json['summary'] as String,
    category: json['category'] as String,
    countryCodes: List<String>.from(json['countryCodes'] as List<dynamic>),
    aliases: List<String>.from(json['aliases'] as List<dynamic>),
    tags: List<String>.from(json['tags'] as List<dynamic>),
    generatedRefs: List<String>.from(json['generatedRefs'] as List<dynamic>),
    priority: json['priority'] as String,
    order: json['order'] as int,
    lastVerified: DateTime.parse(json['lastVerified'] as String),
    contentStatus: json['contentStatus'] as String,
    sources: (json['sources'] as List<dynamic>)
        .map((value) => WikiSource.fromJson(value as Map<String, dynamic>))
        .toList(growable: false),
    markdown: json['markdown'] as String,
  );

  bool appliesToCountry(String countryCode) =>
      countryCodes.isEmpty || countryCodes.contains(countryCode.toUpperCase());

  bool get isEmergency => priority == 'critical';
}

@immutable
class WikiCatalog {
  final List<WikiPage> pages;
  final Map<String, WikiGeneratedRecord> generatedRecords;
  final String countryCode;
  final bool countrySupported;

  const WikiCatalog({
    required this.pages,
    required this.generatedRecords,
    required this.countryCode,
    required this.countrySupported,
  });

  WikiPage? pageById(String id) {
    for (final page in pages) {
      if (page.id == id) return page;
    }
    return null;
  }
}
