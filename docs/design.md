# NefOxide Design

NefOxide is a macOS RAW developer for Nikon NEF files. The app targets Apple Silicon first, uses Rust for the public Nikon SDK backend and Swift for the native UI.

Color fidelity is the primary requirement. Matching Nikon NX Studio output is more important than backend portability.

## Goals

- Open Nikon NEF files through a local Nikon Image SDK installation.
- Match Nikon NX Studio color as closely as possible, especially Picture Control and camera color behavior.
- Keep editing non-destructive and fast enough for interactive use.
- Render responsive previews while preserving a path to high-quality full-resolution export.
- Import photos into date-based folders on a network share automatically.
- Support albums, favorites, ratings, and quick filtering for culling and review.
- Provide a clearer adjustment experience than Nikon NX Studio while preserving Nikon-like color.
- Build a native macOS app that feels like a professional RAW developer, not a simple file converter.

## Non-Goals for the First Version

- Full DAM/library management.
- Cross-platform UI.
- Complete feature parity with legacy Nikon editing tools.
- A from-scratch NEF decoder before Nikon color matching is proven.
- A portable backend abstraction before the native color path is understood.
- A full cloud sync or multi-user catalog system.

## Repository Shape

```text
NefOxide/
	app/
		NefOxideApp/              SwiftUI/AppKit macOS app
	crates/
		nefoxide-nikon/           Rust wrapper around public Nikon Image SDK
		nefoxide-cli/             Rust command-line renderer and comparison tools
		nefoxide-core/            Rust catalog/import/edit-state core
		nefoxide-ffi/             Future Swift/Rust FFI boundary
	tools/
		nefoxide-render/          Objective-C++ public SDK renderer kept for parity checks
	docs/
		design.md
		plan.md
	lib/NikonSDK/               Local SDK headers, runtime files, resources, and profiles, ignored by git
```

## Runtime Dependency Model

The Nikon SDK is a bring-your-own dependency. The repository does not commit vendor headers, runtime binaries, or SDK payloads.

For local macOS development, `lib/NikonSDK/` must contain the public SDK header, release runtime files, resources, and profiles copied from the Nikon SDK:

```text
lib/NikonSDK/Include/Nkfl_Interface.h
lib/NikonSDK/Frameworks/Elm.framework/
lib/NikonSDK/Frameworks/libImgSDK.dylib
lib/NikonSDK/Frameworks/libRCSigProc.dylib
lib/NikonSDK/Frameworks/libboost_atomic-clang-darwin150-mt-1_82.dylib
lib/NikonSDK/Frameworks/libboost_filesystem-clang-darwin150-mt-1_82.dylib
lib/NikonSDK/Frameworks/libboost_system-clang-darwin150-mt-1_82.dylib
lib/NikonSDK/Frameworks/libboost_thread-clang-darwin150-mt-1_82.dylib
lib/NikonSDK/Frameworks/libtbb.dylib
lib/NikonSDK/Frameworks/libtbbmalloc.dylib
lib/NikonSDK/Resources/prm.bin
lib/NikonSDK/Profiles/
```

Build scripts and Xcode settings should read Nikon SDK inputs from `lib/NikonSDK/` only. Downloaded SDK packages, SDK documentation, sample files, NEF files, and reference exports are local developer data and must not be referenced by project code.

The current checked SDK binaries include both `x86_64` and `arm64` slices, so the app can run natively on Apple Silicon when linked correctly.

The app bundle must include the required SDK runtime files at build or packaging time. Xcode should copy frameworks from `lib/NikonSDK/Frameworks/` into `NefOxideApp.app/Contents/Frameworks/`, and copy SDK resources/profiles from `lib/NikonSDK/Resources/` and `lib/NikonSDK/Profiles/` into the app resources layout the SDK expects.

## System Architecture

```text
Swift macOS App
	- Library/sidebar shell
	- Image viewport
	- Edit controls
	- Export UI
	- Async calls into Rust backend/FFI

Rust Backend
	- Public Nikon Image SDK wrapper with a small unsafe boundary around Nkfl_Entry
	- CLI renderer and comparison tooling
	- Future import/catalog/cache/job orchestration
	- Future Swift-facing FFI surface

Public Nikon SDK Path
	- Nkfl_Entry calls
	- Session lifetime management
	- Metadata and image extraction
	- Baseline supported rendering path
	- Picture Control and ColorProcess validation

Color-Matching Validation Path
	- Compare public SDK output against NX Studio exports
	- Inspect public SDK metadata and ExifTool MakerNotes
	- Implement app-side adjustments only when backed by measured differences
```

## Public SDK Color Findings

The canonical public SDK renderer is the Rust CLI in `crates/nefoxide-cli` using `crates/nefoxide-nikon`. The Objective-C++ `tools/nefoxide-render` renderer remains useful for parity checks.

Current findings from local validation files:

- ExifTool reports `PictureControlName=Auto`, `PictureControlVersion=0200`, `WhiteBalance=Auto0`, `ActiveD-Lighting=Off`, and `ColorSpace=sRGB`.
- After `SetColorProcess(AppliedInCamera)` plus `RawParameterSet=AsShot`, the public SDK reports Picture Control support, current Picture Control `0x0001` (`AsShot`), modified/recorded Picture Control version `3`, and a 9-item Picture Control list.
- Explicit `--picture-control=auto` can be byte-identical to the baseline public render when the file's as-shot Picture Control is already Auto.
- Explicit `--picture-control=vivid` and `--picture-control=landscape` change pixels, confirming the public Picture Control setter is wired.
- Calling `RawDevelopment(PictureControlAsShot)` directly with no payload returns SDK error `0x0004` (`InvalidParam`) in the current validation path.
- `RawParameterSet=AsShot` resets the public SDK output profile back to the display profile. Apply `SetOutputProfile_UTF8` after raw-development resets/settings, not before them.
- `RawParameterSet=AsShot` also resets stateful development settings applied before it. In ordering checks, Picture Control Vivid reverted to AsShot and Active D-Lighting Off reverted to AsShot when `RawParameterSet=AsShot` was called afterward.
- Public SDK calls should therefore be ordered as: open session, apply raw parameter set, set the desired color process, apply persistent raw-development edits such as Picture Control or Active D-Lighting, then apply output profile/device profile last.
- Reapplying `NKsRGB.icm` after raw-development settings changes pixels and fixes the flatter/wrong output caused by the display profile reset.
- Against local NX Studio reference exports, the corrected public after-raw render originally measured about `0.992` downsampled RGB MAD with the Objective-C++/ImageIO 4:2:0 check. The Rust CLI now embeds Nikon sRGB and writes 4:4:4 JPEGs like NX Studio; local validation exports measure about `0.294` downsampled RGB MAD to the NX export.
- `SetColorProcess(Latest)` followed by `RawParameterSet=AsShot` collapses back to a byte-identical baseline public render.
- `SetColorProcess(Latest)` only persists if it is applied after `RawParameterSet=AsShot` or if the raw parameter reset is skipped. In that state, the modified Picture Control version moves to `7` (`3.1`), the Picture Control list expands to 33 items, and pixels change.
- The `Latest + Auto` public render is not closer to the NX Studio export by the current downsampled whole-image RGB mean absolute difference metric.
- Explicit Active D-Lighting Off is accepted by the SDK (`activeDLighting` changes from `0x0001` to `0x0002`) but can remain byte-identical to the matching baseline render.

Generated comparison sheets should be written to local ignored output paths.

```text
<local-comparison-output>.jpg
```

The rust-vs-nx sheet columns are Rust public SDK output and NX Studio export.

Rust CLI export notes:

```text
cargo run -p nefoxide-cli -- convert /path/to/input.nef /path/to/output.jpg
```

The Rust CLI uses the validated public SDK call order, embeds `NKsRGB.icm`, and writes 4:4:4 JPEG output for NX-like export sampling.

## Swift UI Design

Swift owns the application experience, not the image processing rules.

Initial UI surfaces:

- Source browser for opening a folder or individual NEF file.
- Filmstrip or compact grid of imported images.
- Main image viewport with fit, fill, and 1:1 zoom modes.
- Inspector panel for exposure, white balance, contrast, highlights, shadows, Picture Control, and export.
- Progress and error surfaces for slow SDK work.

Swift should call the Rust backend asynchronously through a narrow FFI surface. Long-running render work must not block the main thread.

## Import and Storage Workflow

NefOxide should treat network-share import as a first-class workflow.

Source locations may be camera cards, local folders, or temporary ingest folders. The destination is usually a mounted network share such as:

```text
/Volumes/PhotoStore/
```

Imported files should be copied into date-based folders derived from capture date metadata:

```text
/Volumes/PhotoStore/YYYY/MM/DD/image.nef
```

Example:

```text
/Volumes/PhotoStore/2026/06/25/image.nef
```

Import behavior:

- Read capture date from EXIF/MakerNote metadata.
- Create `YYYY/MM/DD` destination folders automatically.
- Copy NEF files and supported sidecar files together when present.
- Avoid overwriting existing files; use deterministic duplicate handling.
- Preserve original filenames by default.
- Show import progress, failures, and skipped duplicates.
- Work reliably when the destination is a mounted network volume.

The app should not require users to manually create folder trees before import.

## Library Model

The physical file layout remains simple and user-readable. The app database stores organization and fast lookup state.

Physical storage:

```text
/Volumes/PhotoStore/2026/06/25/image.nef
/Volumes/PhotoStore/2026/06/25/image.nefoxide.json
```

Local app database:

```text
~/Library/Application Support/NefOxide/NefOxide.sqlite
```

Database responsibilities:

- File path and stable image id.
- Capture date and import date.
- Camera, lens, exposure, and Picture Control metadata.
- Favorite flag, rating, reject flag, and optional color label.
- Album membership.
- Thumbnail and preview cache keys.
- Last known sidecar state.

Sidecar responsibilities:

- Durable per-image edit state.
- Optional favorite/rating state for portability.
- App version and schema version.

## Albums, Favorites, and Filtering

Albums are virtual collections. Adding an image to an album must not duplicate the NEF on disk.

Required organization features:

- Create, rename, and delete albums.
- Add/remove images from albums.
- Mark images as favorites.
- Assign ratings from 0 to 5.
- Filter by favorite, rating, import date, folder, album, and camera metadata.
- Provide a fast “Favorites” view for quick review.

This keeps the filesystem clean while making culling and review faster than folder-only browsing.

## Adjustment UX

NefOxide should preserve Nikon-like color while exposing controls in a more direct way.

Initial adjustment controls:

- Exposure.
- White balance temperature and tint.
- Highlights.
- Shadows.
- Contrast.
- Saturation.
- Picture Control display/selection when supported.
- Reset to camera/NX-like default.

Adjustment behavior:

- Edits are non-destructive.
- Preview updates should use cached or lower-resolution renders.
- Full-quality render is deferred until interaction settles or export begins.
- The app should clearly separate camera/NX baseline rendering from user edits.

## Rust FFI Design

`nefoxide-ffi` is the planned ownership boundary between Swift and the Rust backend.

The Swift-facing API should stay small and C-compatible:

```text
NefOxideContext
NefOxideImage

open_image(path)
render_preview(image, max_dimension)
export_jpeg(image, output_path, quality)
read_metadata(image)
read_raw_development_params(image)
```

Implementation crates:

- `nefoxide-nikon`: supported public SDK calls through `Nkfl_Entry`.
- `nefoxide-core`: catalog/import/edit-state model.
- `nefoxide-ffi`: stable Swift-facing bridge surface.

## NX Color Matching

NX Studio color matching is the highest-risk and highest-value part of the project.

Known findings:

- The test files use Picture Control `Auto`, which appears scene-dependent.
- Public SDK conversion gets very close to NX Studio when the SDK calls are ordered correctly.
- `SetDevelopColorMode(AppliedInCamera)` must be set after `OpenLibrary`.
- `RawParameterSet=AsShot` should be applied before color process/raw edits.
- `SetOutputProfile_UTF8(NKsRGB.icm)` must be applied after raw-development settings.
- Rust JPEG exports should embed `NKsRGB.icm` and use 4:4:4 chroma sampling for NX-like output.

Validation should stay grounded in reproducible comparisons against developer-supplied NX Studio exports.

## Performance Strategy

- Keep the SDK/library context warm in the app instead of opening/closing for each image.
- Generate previews at multiple sizes instead of rendering full resolution during every slider drag.
- Use cached render keys based on image id, edit stack, viewport size, quality level, and rendering path.
- Use low-quality interactive renders while controls are moving, then schedule a high-quality render after interaction settles.
- Use ImageIO/CoreGraphics/ColorSync correctly for export metadata and ICC profiles.
- Move viewport display toward Metal once the color path is correct.

## First Render Pipeline

```text
NEF path
	-> Rust backend open image
	-> public Nikon SDK render path
	-> metadata and preview/full image extraction or rendering
	-> Rust image buffer
	-> JPEG export or Swift viewport display
```

The first version should prefer correctness and measurable color comparisons over complex GPU architecture. Once a color-matching baseline exists, optimize the slowest stage with real numbers.

## Error Handling

The bridge should expose Objective-C `NSError` values to Swift and keep detailed Nikon/NX diagnostic information in logs.

Typical error categories:

- Missing SDK runtime files in `lib/NikonSDK/`.
- Unsupported or corrupt NEF file.
- SDK session open failure.
- Public SDK rendering mismatch.
- Render cancellation.
- Export failure.

## Build Notes

- `lib/NikonSDK/` is local-only and ignored by git.
- `lib/NikonSDK/` is the build input for Nikon headers, runtime files, resources, and profiles.
- Xcode should compile Objective-C++ bridge files as part of the macOS app target or a local framework target.
- The app bundle needs a copy phase that places Nikon SDK runtime files in `Contents/Frameworks/` and SDK resources/profiles in the expected resources layout.
- Xcode 26.3 is the newest practical choice for macOS 15.7.7 development.