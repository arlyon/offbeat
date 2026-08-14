# Mobile artifact size analysis

Measured on 14 August 2026 from a clean release build of the current worktree.
Binary sizes use MiB (1,048,576 bytes); store-facing decimal MB values are noted
where useful. The measurements include current uncommitted application work and
therefore describe the present build rather than the already uploaded 1.0.10
binary byte-for-byte.

## Summary

| Platform | Upload | Store download | Install payload |
| --- | ---: | ---: | ---: |
| Android, arm64 | 46.2 MiB AAB | 22.4 MiB | 51.7 MiB split APKs |
| iOS, arm64 | 45.4 MiB IPA | 27.9 MiB | 77.3 MiB app bundle |

The Android AAB contains 21.3 MiB of compressed native debug symbols. Google Play
retains those for crash symbolication but does not deliver them to users. The iOS
IPA similarly contains 17.5 MiB of compressed symbols; its compressed `Payload/`
is 27.9 MiB. Apple may further process and recompress the app, so the eventual
App Store download shown to users can differ.

## Android

### Android measurement

The app was rebuilt after deleting generated build output:

```sh
flutter build appbundle --release \
  --target-platform android-arm64 \
  --analyze-size
```

Bundletool 1.18.3 generated splits for an arm64, xxhdpi, Android 15 device with
en-GB. `bundletool get-size total` reported a 23,521,012-byte compressed download.
The delivered split APK files totalled 54,226,187 bytes.

### Main delivered components

| Component | Uncompressed size | Approximate share |
| --- | ---: | ---: |
| Rust `offbeat_bridge` | 24.6 MiB | 48% |
| Flutter engine | 11.0 MiB | 22% |
| Dart AOT `libapp.so` | 6.6 MiB | 13% |
| ML Kit barcode engine | 4.7 MiB | 9% |
| DEX, barcode models, resources, and other JNI libraries | 4.2 MiB | 8% |

Flutter's Dart size report attributes about 3 MiB to Flutter itself and about
0.7 MiB to Offbeat Dart code. The bundled wiki is below 0.4 MiB uncompressed.
Neither is a worthwhile first optimisation target.

The Rust bridge's symbol-attributed code is led by the iroh/QUIC networking stack,
HTTP/TLS/DNS support, the Rust runtime, cryptographic providers, SQLite, and Yrs.
A rough analysis of the unstripped Android bridge attributed:

| Rust/native area | Symbol-attributed size |
| --- | ---: |
| iroh, QUIC, and networking | 4.2 MiB |
| HTTP, TLS, and DNS | 3.5 MiB |
| Rust standard library | 2.1 MiB |
| AWS-LC and ring cryptography | 2.3 MiB |
| Offbeat core and bridge | 1.4 MiB |
| Tokio | 1.0 MiB |
| SQLite | 0.7 MiB |
| Yrs | 0.4 MiB |

These categories are approximate because they are derived from demangled symbol
names and do not attribute all binary sections.

### ABI correctness verification

The release build now excludes transitive JNI libraries for armeabi-v7a and
x86_64 in addition to filtering native builds to arm64-v8a. The validated AAB
contains only arm64 versions of Flutter, Dart AOT, the Rust bridge, and ML Kit.

Bundletool 1.18.3 validated the bundle and produced a complete arm64 install set.
For an armeabi-v7a-only device specification it now fails with:
`The app doesn't support ABI architectures of the device.` The release workflow
also rejects any bundle containing a native ABI other than arm64-v8a.

## iOS

### iOS measurement

A signed App Store archive and IPA were built locally with Flutter's size analyzer.
The IPA was not uploaded. The temporary signing keychain and SDK-path wrapper were
removed immediately after export.

The signed archive contains a 77.3 MiB `Runner.app`. Its IPA is 47,626,011 bytes
(45.4 MiB), split into a 27.9 MiB compressed application payload and 17.5 MiB of
compressed symbols.

### Main installed components

| Component | Uncompressed size | Approximate share |
| --- | ---: | ---: |
| Rust `offbeat_bridge.framework` | 44.0 MiB | 57% |
| Runner executable and statically linked pods | 13.5 MiB | 17% |
| Flutter engine | 8.8 MiB plus ICU data | 13% |
| Dart `App.framework` | 7.3 MiB plus assets | 10% |
| Other dynamic frameworks and resources | about 2.5 MiB | 3% |

The Runner executable includes statically linked barcode-scanning dependencies,
so its size cannot be assigned entirely to first-party Objective-C or Swift code.

The Rust framework retains more than 100,000 local symbols. Applying Apple's
`strip -x` to a copy reduced it from 44.0 MiB to 33.9 MiB. The same file compressed
from 14.2 MiB to 13.1 MiB, suggesting an attainable saving of about 10.1 MiB
installed and 1.1 MiB downloaded if the release framework is stripped safely
before final code signing.

## Is Flutter the limiting factor?

Not primarily. Flutter contributes a fairly fixed engine cost of roughly
9–11 MiB installed, while Offbeat's Dart AOT payload is 6–8 MiB installed and
about 3 MiB compressed. The size analyzer attributes only about 0.7 MiB to
Offbeat's own Dart code. Tree shaking is already reducing the Material icon font
by more than 99%.

The Rust bridge alone is 24.6 MiB on Android and 44.0 MiB before additional iOS
symbol stripping. Rewriting the Flutter UI natively would therefore carry major
product and maintenance cost while leaving the dominant Rust, networking,
cryptography, SQLite, CRDT, and barcode-scanning payloads intact.

With ABI correctness resolved, a realistic optimisation pass should target iOS
symbol stripping, duplicate cryptographic providers, and unused Rust dependency
features. These changes may save several megabytes of store download without
removing product capabilities. Larger savings would probably require replacing
ML Kit or removing networking functionality.

## Recommended measurement plan

Each optimisation should be applied independently and compared against the
baseline in this report:

1. Produce a clean baseline AAB and signed IPA.
2. Strip non-global symbols from the exported iOS Rust framework before final
   signing, then verify FFI symbol resolution and archive validation.
3. Remove the bridge's unused direct iroh dependency or disable its default
   features, rebuild, and run transport, relay, gossip, BLE, and offline tests.
4. Align rustls dependencies on one cryptographic provider where the iroh stack
   permits it, then repeat TLS and relay tests.
5. Compare the default Cargo release profile with thin LTO and one codegen unit.
   Record binary size, clean-build time, cold start, and sync performance.
6. Treat replacing ML Kit as a separate product/engineering decision rather than
   routine size optimisation.

Do not combine experiments until each one's contribution is measured. Keep crash
symbols: they affect upload artifacts, not store downloads.

## Prioritised opportunities

1. **Strip local symbols from the iOS Rust framework.** Verify the exported
   framework remains signed and all Flutter Rust Bridge entry points resolve.
   Expected saving: about 10.1 MiB installed and 1.1 MiB downloaded.
2. **Audit Rust dependency features.** The bridge directly enables iroh defaults
   while the core uses selected iroh features. The binary also includes both
   AWS-LC and ring cryptographic code. Removing unused direct dependencies and
   aligning feature/provider selection may reduce the largest component, but it
   requires transport, TLS, BLE, and gossip regression tests.
3. **Evaluate release-profile size optimisation.** Compare thin LTO and fewer
   codegen units against the present default Cargo release profile. Measure cold
   start and sync performance before adopting the result.
4. **Keep barcode scanning unless product scope changes.** ML Kit costs roughly
   4.7 MiB of Android native code plus models and contributes to the 13.5 MiB iOS
   Runner executable. QR joining is a product feature, so replacement should be
   considered only if a platform-native implementation provides a meaningful,
   measured saving.
5. **Do not remove debug symbols to optimise user downloads.** Android AAB and iOS
   IPA symbols enlarge upload artifacts but are not part of the user application
   payload and are valuable for production crash diagnosis.

The present store-download estimates are reasonable for a Flutter application
with a Rust P2P core. Further optimisation should focus on the Rust framework
rather than Dart code, wiki content, icons, or other small assets.
