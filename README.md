# NefOxide

An open-source, high-performance macOS RAW developer for Nikon NEF files, focused on Nikon-like color through Nikon's public Image SDK. The repository contains project code only; bring your own Nikon Image SDK, NEF files, and reference exports for local testing.

## Unofficial Project Notice

NefOxide is an independent project. It is not Nikon, and it is not affiliated with, endorsed by, sponsored by, or supplied by Nikon.

This repository does not provide Nikon SDK headers, Nikon runtime binaries, Nikon applications, Nikon sample files, NEF test files, or NX Studio exports. Anyone building or testing NefOxide must obtain any Nikon SDK files and test images separately and place them in local gitignored folders.

## Direction

NefOxide is a Rust-first Nikon RAW development project with a native macOS UI planned on top. Color fidelity matters more than backend novelty, so the public Nikon SDK call order is treated as part of the rendering contract.

Primary stack:

- Rust for the public Nikon SDK wrapper, CLI, import/catalog logic, and render orchestration.
- SwiftUI/AppKit for the future macOS app shell.
- Nikon Image SDK for the supported public rendering path.
- Native macOS APIs only where they are the best tool for UI or platform integration.

Primary workflow goals:

- Import NEF files from cards or folders into a network share using `YYYY/MM/DD/image.nef` folders.
- Create date folders automatically during import.
- Keep albums as virtual collections without duplicating files.
- Support favorites, ratings, and quick filters for culling.
- Provide a more intuitive adjustment UI while preserving Nikon-like color.

## Local Nikon SDK Files

NefOxide does not commit, vendor, mirror, or redistribute Nikon SDK headers, runtime binaries, resources, profiles, sample files, or applications. Each developer must supply their own Nikon Image SDK files locally, then create and populate `lib/NikonSDK/` before building.

The build reads Nikon SDK inputs from `lib/NikonSDK/` only. Keep downloaded SDK packages, SDK documentation, sample files, NEF files, and reference exports in your own local ignored storage; they are not part of this repository and should not be referenced by project code.

For the current macOS SDK layout, after you have obtained the SDK yourself, copy the release runtime files from your local SDK package:

```sh
<path-to-nikon-image-sdk>/Library/Mac/Sample/Lib/release/
```

into:

```sh
lib/NikonSDK/Frameworks/
```

The required local SDK layout is:

- `lib/NikonSDK/Include/Nkfl_Interface.h`
- `lib/NikonSDK/Frameworks/libImgSDK.dylib`
- `lib/NikonSDK/Frameworks/libRCSigProc.dylib`
- `lib/NikonSDK/Frameworks/libboost_atomic-clang-darwin150-mt-1_82.dylib`
- `lib/NikonSDK/Frameworks/libboost_filesystem-clang-darwin150-mt-1_82.dylib`
- `lib/NikonSDK/Frameworks/libboost_system-clang-darwin150-mt-1_82.dylib`
- `lib/NikonSDK/Frameworks/libboost_thread-clang-darwin150-mt-1_82.dylib`
- `lib/NikonSDK/Frameworks/libtbb.dylib`
- `lib/NikonSDK/Frameworks/libtbbmalloc.dylib`
- `lib/NikonSDK/Frameworks/Elm.framework/`
- `lib/NikonSDK/Resources/prm.bin`
- `lib/NikonSDK/Profiles/`

Convenience command from the repo root:

```sh
SDK_SOURCE="/path/to/Nikon Image SDK/Library/Mac"
mkdir -p lib/NikonSDK/Include lib/NikonSDK/Frameworks lib/NikonSDK/Resources lib/NikonSDK/Profiles
cp "$SDK_SOURCE/Include/Nkfl_Interface.h" lib/NikonSDK/Include/
cp -R "$SDK_SOURCE/Sample/Lib/release/"* lib/NikonSDK/Frameworks/
cp -R "$SDK_SOURCE/Sample/Resources/"* lib/NikonSDK/Resources/
cp -R "$SDK_SOURCE/Profiles/"* lib/NikonSDK/Profiles/
```

The copied Nikon SDK files are intentionally ignored by git. Anyone cloning this repository needs to provide their own local `lib/NikonSDK/` contents before building or running SDK-backed commands.

## Development Docs

- [docs/design.md](docs/design.md) describes the Rust-first public SDK architecture.
- [docs/plan.md](docs/plan.md) tracks the color-first implementation milestones.

## Current Rendering Check

Public Nikon SDK renderer:

```sh
cargo run -p nefoxide-cli -- convert /path/to/input.nef /tmp/nefoxide-output.jpg
```

The command above assumes you have supplied your own local test NEF. The corrected public SDK path gets very close to NX Studio exports for local validation files when called in the right order: `SetDevelopColorMode(AppliedInCamera)`, `RawParameterSet=AsShot`, `SetColorProcess(AppliedInCamera)`, and `SetOutputProfile_UTF8(NKsRGB.icm)` last.