#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

device_id="${1:?usage: $0 DEVICE_ID DEVICE_KIND}"
device_kind="${2:?usage: $0 DEVICE_ID DEVICE_KIND}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/offbeat-screenshots.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

case "$device_kind" in
iphone_69 | ipad_13)
	destinations=(
		"fastlane/screenshots/en-GB"
		"fastlane/screenshots/en-US"
	)
	;;
android_phone)
	destinations=(
		"fastlane/metadata/android/en-GB/images/phoneScreenshots"
		"fastlane/metadata/android/en-US/images/phoneScreenshots"
	)
	;;
android_tablet_7)
	destinations=(
		"fastlane/metadata/android/en-GB/images/sevenInchScreenshots"
		"fastlane/metadata/android/en-US/images/sevenInchScreenshots"
	)
	;;
android_tablet_10)
	destinations=(
		"fastlane/metadata/android/en-GB/images/tenInchScreenshots"
		"fastlane/metadata/android/en-US/images/tenInchScreenshots"
	)
	;;
*)
	printf 'Unsupported device kind: %s\n' "$device_kind" >&2
	exit 2
	;;
esac

STORE_SCREENSHOT_OUTPUT="$tmp_dir" \
	STORE_SCREENSHOT_SUFFIX="$device_kind" \
	flutter drive \
	--device-id "$device_id" \
	--driver test_driver/store_screenshots_driver.dart \
	--target integration_test/store_screenshots_test.dart

for destination in "${destinations[@]}"; do
	mkdir -p "$destination"
	rm -f "$destination"/*_"$device_kind".png
	cp "$tmp_dir"/*.png "$destination"/
done

printf 'Generated %s screenshots for %s\n' "$device_kind" "$device_id"
