#!/bin/sh
set -eu

# Flutter's asset build reads assets/wiki/index.json from the project tree.
# Generate the restricted iOS corpus immediately before Flutter bundles assets,
# then restore the Android corpus even when the build fails.
app_path="${FLUTTER_APPLICATION_PATH:?FLUTTER_APPLICATION_PATH is required}"
asset_path="$app_path/assets/wiki/index.json"
backup_path="$(mktemp "${TMPDIR:-/tmp}/offbeat-wiki.XXXXXX")"

cp "$asset_path" "$backup_path"
restore() {
  cp "$backup_path" "$asset_path"
  rm -f "$backup_path"
}
trap restore EXIT HUP INT TERM

cd "$app_path"
"${FLUTTER_ROOT:?FLUTTER_ROOT is required}/bin/cache/dart-sdk/bin/dart" \
  run tool/build_wiki.dart --platform ios --output "$asset_path"
"$FLUTTER_ROOT/packages/flutter_tools/bin/xcode_backend.sh" build
