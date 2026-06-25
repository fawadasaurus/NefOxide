# NefOxide Plan

This plan is color-first. The project prioritizes matching Nikon NX Studio output with a Rust public-SDK backend and a native macOS Swift UI.

## Phase 0: Local Environment

- Install Xcode 26.3 for macOS 15.7.7 development.
- Keep the macOS app shell under `app/NefOxideApp`.
- Keep downloaded Nikon Image SDK packages, SDK documentation, sample files, NEF files, and reference exports in local ignored storage outside project code paths.
- Copy `Nkfl_Interface.h` into `lib/NikonSDK/Include/` locally.
- Copy the SDK release runtime files into `lib/NikonSDK/Frameworks/` locally.
- Copy SDK resources into `lib/NikonSDK/Resources/` and profiles into `lib/NikonSDK/Profiles/` locally.
- Verify `lib/NikonSDK/Frameworks/libImgSDK.dylib`, `lib/NikonSDK/Frameworks/libRCSigProc.dylib`, and `lib/NikonSDK/Frameworks/libtbb.dylib` include `arm64` slices.

Validation:

```sh
lipo -info lib/NikonSDK/Frameworks/libImgSDK.dylib lib/NikonSDK/Frameworks/libRCSigProc.dylib lib/NikonSDK/Frameworks/libtbb.dylib
```

## Phase 1: Rust Public SDK Core

Restore and harden the Rust public Nikon SDK wrapper:

- `crates/nefoxide-nikon`: safe Rust wrapper around `Nkfl_Entry` with minimal unsafe.
- `crates/nefoxide-cli`: command-line renderer and comparison tool.
- `crates/nefoxide-core`: future catalog/import/edit-state model.
- `crates/nefoxide-ffi`: future Swift/Rust bridge surface.

Done when:

- `cargo run -p nefoxide-cli -- convert <input.nef> <output.jpg>` renders full-size JPEGs.
- The wrapper applies public SDK calls in the validated order.
- The unsafe SDK boundary remains localized inside `nefoxide-nikon`.

## Phase 2: Public SDK Conversion

Keep the proven NEF-to-JPEG path canonical in Rust.

- Open the Nikon library.
- Open a NEF session.
- Read image info and raw-development metadata.
- Render RGB data through the public SDK.
- Export JPEG with embedded Nikon sRGB profile and 4:4:4 chroma sampling for NX-like output.

Done when:

- The Rust CLI converts a developer-supplied NEF file to JPEG.
- Output dimensions match the source.
- The exported JPEG contains the intended ICC profile and useful metadata.
- A timing breakdown is printed for open, render, conversion, and export.

## Phase 3: NX Studio Color Comparison Harness

Build a repeatable comparison workflow against NX Studio exports.

Inputs:

- Source NEF.
- NefOxide export.
- NX Studio export.

Measurements:

- ICC/profile metadata.
- EXIF/MakerNote recipe metadata.
- Whole-image RGB deltas.
- Orange/red/yellow region deltas.
- Optional crop comparison images.

Done when:

- The command-line comparison workflow can quantify the color gap for developer-supplied validation files.
- The comparison confirms whether changes affect pixels, metadata, or both.

## Phase 4: Public Color Matching Experiments

Use public SDK output plus measured comparison against NX Studio exports.

Initial experiments:

- Render with the public SDK CLI.
- Export the same NEF from NX Studio.
- Compare RGB deltas in whole-image and targeted color regions.
- Inspect ExifTool MakerNotes and public SDK raw-development parameters.
- Try app-side correction only when backed by measured differences.

Current public SDK findings from local validation files:

- Public Picture Control mutation works: Vivid and Landscape produce different pixels.
- The file's as-shot Picture Control is Auto, and explicit Auto is byte-identical to the baseline public render.
- Direct `kNkfl_RawDevelopment_PictureControlAsShot` currently fails with `kNkfl_Code_Err_InvalidParam` when sent without a payload.
- `RawParameterSet=AsShot` resets the output profile to the display profile; `SetOutputProfile_UTF8` must be applied after raw-development settings.
- `RawParameterSet=AsShot` also resets earlier Picture Control and Active D-Lighting changes, so the public SDK bridge should apply raw parameter set first, then color process and raw-development edits, then output profile/device profile last.
- Correcting the output-profile order makes the public sRGB render materially closer to local NX Studio reference exports by the current downsampled RGB MAD check.
- The corrected public after-raw render measured about `0.992` downsampled RGB MAD to a local NX Studio reference export with the Objective-C++/ImageIO 4:2:0 check, versus about `1.990` for the old public baseline. The Rust CLI 4:4:4 export measures about `0.294` downsampled RGB MAD to the same reference export.
- `SetColorProcess(Latest)` only changes output when it is applied after `RawParameterSet=AsShot` or when the raw reset is skipped; otherwise it collapses back to the baseline public render.
- The `Latest + Auto` path changes pixels and exposes Picture Control version 3.1/list expansion, but does not currently match the developer-supplied NX Studio export better for the tested image.
- Active D-Lighting Off is accepted by the SDK but does not affect pixels for this file.

Done when:

- We know whether a public-SDK-only correction layer can close the visible color gap.
- We have a repeatable command-line comparison workflow for future images.

## Phase 5: Preview UI Integration

Wire the Swift app to the Rust backend through `nefoxide-ffi`.

Initial UI:

- Open file or folder.
- Compact image list.
- Main preview viewport.
- Basic metadata panel.
- Export command.

Done when:

- The app opens a NEF file.
- The app displays a backend-rendered preview.
- The app can export a JPEG through the native bridge.
- Long-running SDK work does not freeze the UI.

## Phase 6: Import and Library Workflow

Build the workflow that makes NefOxide useful day to day.

Import requirements:

- Choose source card/folder.
- Choose destination network share root.
- Read capture dates from image metadata.
- Create `YYYY/MM/DD` folders automatically.
- Copy NEF files and sidecars without overwriting existing files.
- Show import progress and a summary of imported/skipped/failed files.

Library requirements:

- Create a local SQLite catalog under `~/Library/Application Support/NefOxide/`.
- Track file path, capture date, import date, camera/lens metadata, favorite flag, rating, and album membership.
- Keep physical files in the date-based folder structure.
- Keep albums virtual.

Done when:

- A test import copies files into `/YYYY/MM/DD/` folders on a chosen destination.
- Imported images appear in the app library without manual folder refresh.
- Favorites and ratings persist across app launches.

## Phase 7: Albums, Favorites, and Filters

Add culling and organization tools.

Initial features:

- Favorite/unfavorite image.
- Set rating 0-5.
- Create albums.
- Add/remove images from albums.
- Filter by favorites, rating, folder, import date, and album.

Done when:

- Favorites view works instantly for the current catalog.
- Album membership does not duplicate files on disk.
- Filters can be combined for common review workflows.

## Phase 8: Color Controls and Editing

Add non-destructive editing only after the Nikon color baseline is understood.

Initial controls:

- Exposure.
- White balance temperature and tint.
- Contrast.
- Highlights.
- Shadows.
- Picture Control selection if supported.

Done when:

- Edits are represented as app-side state.
- Preview and export use the same edit state.
- The app can distinguish camera-baseline public SDK rendering from app-side user edits.

## Phase 9: Performance Pass

- Keep SDK contexts warm.
- Add preview caches.
- Add interactive and final-quality render modes.
- Add cancellation for obsolete renders.
- Profile full open-to-preview time.
- Move viewport rendering toward Metal if needed.

Done when:

- Slider movement stays responsive.
- Obsolete renders do not block newer renders.
- Preview generation timing is visible in logs.

## Immediate Next Step

Make the restored Rust CLI the canonical public renderer, then expose the same `nefoxide-nikon` render path through a narrow Swift-callable FFI.