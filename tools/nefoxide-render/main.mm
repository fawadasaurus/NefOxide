#include <CoreFoundation/CoreFoundation.h>
#include <CoreGraphics/CoreGraphics.h>
#include <ImageIO/ImageIO.h>

#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <string>
#include <vector>

#ifndef PATH_MAX
#define PATH_MAX 1024
#endif

#include "Nkfl_Interface.h"

static Nkfl_EntryProcPtr gEntry = nullptr;

struct RenderOptions {
    std::string outputProfileName = "NKsRGB.icm";
    std::string outputDeviceProfileName = "none";
    unsigned long outputRenderingIntent = kNkfl_RenderingIntent_Perceptual;
    long developColorMode = kNkfl_DevelopColorMode_AppliedInCamera;
    unsigned long colorProcess = kNkfl_ColorProcess_AppliedInCamera;
    bool applyRawParameterSetAsShot = true;
    std::string activeDLightingName = "none";
    bool hasTintOverride = false;
    double tintOverride = 0.0;
    std::string pictureControlName = "none";
    bool applyPictureControlAsShot = false;
    std::string orderName = "raw-color";
};

static bool sdkCall(unsigned long command, void *param, const char *operation) {
    unsigned long code = gEntry(command, param);
    if (code == kNkfl_Code_None) {
        return true;
    }

    fprintf(stderr, "Nikon SDK %s failed with code 0x%04lx\n", operation, code);
    return false;
}

static bool sdkCallAllowWarning(unsigned long command, void *param, const char *operation) {
    unsigned long code = gEntry(command, param);
    if (code == kNkfl_Code_None) {
        return true;
    }

    if ((code & 0x0100) == 0x0100) {
        fprintf(stderr, "Nikon SDK %s returned warning 0x%04lx\n", operation, code);
        return true;
    }

    fprintf(stderr, "Nikon SDK %s failed with code 0x%04lx\n", operation, code);
    return false;
}

static std::string repoRelativePath(const char *path) {
    return std::string(NEFOXIDE_REPO_ROOT) + "/" + path;
}

static bool loadSdk() {
    std::string sdkPath = repoRelativePath("lib/NikonSDK/Frameworks/libImgSDK.dylib");
    void *handle = dlopen(sdkPath.c_str(), RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        fprintf(stderr, "Failed to load %s: %s\n", sdkPath.c_str(), dlerror());
        return false;
    }

    gEntry = reinterpret_cast<Nkfl_EntryProcPtr>(dlsym(handle, "Nkfl_Entry"));
    if (!gEntry) {
        fprintf(stderr, "Failed to resolve Nkfl_Entry: %s\n", dlerror());
        return false;
    }

    return true;
}

static bool openLibrary(long developColorMode) {
    NkflPtr libraryHandle = nullptr;
    NkflLibraryParam param = {};
    param.ulSize = sizeof(param);
    param.ulVersion = 0x01000000;
    param.ulVMMemorySize = 1024;
    param.pNkflPtr = &libraryHandle;

    const char *vmPath = "/tmp/nefoxide-native-sdk-vm.tmp";
    strncpy(reinterpret_cast<char *>(param.VMFileInfo), vmPath, sizeof(param.VMFileInfo) - 1);

    if (!sdkCall(kNkfl_Cmd_OpenLibrary, &param, "OpenLibrary")) {
        return false;
    }

    NkflDevelopColorMode mode = {};
    mode.ulSize = sizeof(mode);
    mode.lDevelopColorMode = developColorMode;
    return sdkCall(kNkfl_Cmd_SetDevelopColorMode, &mode, "SetDevelopColorMode");
}

static void closeLibrary() {
    if (gEntry) {
        gEntry(kNkfl_Cmd_CloseLibrary, nullptr);
    }
}

static bool openSession(const char *inputPath, unsigned long *sessionId) {
    NkflSessionParam param = {};
    param.ulSize = sizeof(param);
    param.ulType = kNkfl_Source_FileName_UTF8;
    param.pFileInfo = const_cast<char *>(inputPath);
    param.bImageLoadSkip = false;

    if (!sdkCall(kNkfl_Cmd_OpenSession, &param, "OpenSession")) {
        return false;
    }

    *sessionId = param.ulSessionID;
    return true;
}

static void closeSession(unsigned long sessionId) {
    NkflSessionParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    gEntry(kNkfl_Cmd_CloseSession, &param);
}

static std::string profilePathForName(const std::string &profileName) {
    return repoRelativePath((std::string("lib/NikonSDK/Profiles/") + profileName).c_str());
}

static bool setOutputProfile(unsigned long sessionId, const std::string &profileName, unsigned long renderingIntent) {
    if (profileName == "none") {
        return true;
    }

    std::string profilePath = profilePathForName(profileName);
    NkflOutputProfileParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRenderingIntent = renderingIntent;
    strncpy(reinterpret_cast<char *>(param.OutputProfile), profilePath.c_str(), sizeof(param.OutputProfile) - 1);
    return sdkCall(kNkfl_Cmd_SetOutputProfile_UTF8, &param, "SetOutputProfile");
}

static bool setOutputDeviceProfile(unsigned long sessionId, const std::string &profileName) {
    if (profileName == "none") {
        return true;
    }

    std::string profilePath = profilePathForName(profileName);
    NkflOutputDeviceProfile param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    strncpy(reinterpret_cast<char *>(param.OutputDeviceProfile), profilePath.c_str(), sizeof(param.OutputDeviceProfile) - 1);
    return sdkCall(kNkfl_Cmd_SetOutputDeviceProfile, &param, "SetOutputDeviceProfile");
}

static bool setColorProcess(unsigned long sessionId, unsigned long colorProcess) {
    NkflColorProcess param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulColorProcess = colorProcess;
    return sdkCall(kNkfl_Cmd_SetColorProcess, &param, "SetColorProcess");
}

static bool getColorProcess(unsigned long sessionId, unsigned long *colorProcess) {
    NkflColorProcess param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;

    if (!sdkCall(kNkfl_Cmd_GetColorProcess, &param, "GetColorProcess")) {
        return false;
    }

    *colorProcess = param.ulColorProcess;
    return true;
}

static bool setRawParameterSetAsShot(unsigned long sessionId) {
    NkflRawDevelopment_RawParameterSet rawParameterSet = {};
    rawParameterSet.ulSize = sizeof(rawParameterSet);
    rawParameterSet.ulParamterSet = kNkfl_RawParameterSet_AsShot;

    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_RawParameterSet;
    param.pData = &rawParameterSet;
    return sdkCall(kNkfl_Cmd_RawDevelopment, &param, "RawDevelopment(RawParameterSet)");
}

static bool applyPictureControlAsShot(unsigned long sessionId) {
    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_PictureControlAsShot;
    param.pData = nullptr;
    return sdkCallAllowWarning(kNkfl_Cmd_RawDevelopment, &param, "RawDevelopment(PictureControlAsShot)");
}

static bool setActiveDLighting(unsigned long sessionId, const std::string &name) {
    if (name == "none") {
        return true;
    }

    unsigned long activeDLighting = 0;
    if (name == "as-shot") {
        activeDLighting = kNkfl_ActiveDLighting_AsShot;
    } else if (name == "off") {
        activeDLighting = kNkfl_ActiveDLighting_None;
    } else {
        fprintf(stderr, "invalid --active-d-lighting value: %s\n", name.c_str());
        return false;
    }

    NkflRawDevelopment_ActiveDLighting adl = {};
    adl.ulSize = sizeof(adl);
    adl.ulActiveDLighting = activeDLighting;

    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_ActiveDLighting;
    param.pData = &adl;
    return sdkCall(kNkfl_Cmd_RawDevelopment, &param, "RawDevelopment(ActiveDLighting)");
}

static bool setTint(unsigned long sessionId, double tint) {
    NkflRawDevelopment_Tint tintParam = {};
    tintParam.ulSize = sizeof(tintParam);
    tintParam.lfTint = tint;

    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_Tint;
    param.pData = &tintParam;
    return sdkCall(kNkfl_Cmd_RawDevelopment, &param, "RawDevelopment(Tint)");
}

static bool getRawDevelopmentInfo(unsigned long sessionId, unsigned long *rawDevelopmentInfo) {
    NkflRawDevelopmentInfo param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;

    if (!sdkCall(kNkfl_Cmd_GetRawDevelopmentInfo, &param, "GetRawDevelopmentInfo")) {
        return false;
    }

    *rawDevelopmentInfo = param.ulRawDevelopmentInfo;
    return true;
}

static bool getPictureControl(unsigned long sessionId, NkflRawDevelopment_PictureControl *pictureControl) {
    memset(pictureControl, 0, sizeof(*pictureControl));
    pictureControl->ulSize = sizeof(*pictureControl);

    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_PictureControl;
    param.pData = pictureControl;
    return sdkCall(kNkfl_Cmd_GetRawDevelopmentParam, &param, "GetRawDevelopmentParam(PictureControl)");
}

static bool getRawDevelopmentParam(unsigned long sessionId, unsigned long rawDevelopment, void *data, const char *operation) {
    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = rawDevelopment;
    param.pData = data;
    return sdkCall(kNkfl_Cmd_GetRawDevelopmentParam, &param, operation);
}

static void dumpPictureControlDiagnostics(unsigned long sessionId, const char *phase) {
    printf("diagnostics[%s]\n", phase);

    unsigned long rawDevelopmentInfo = 0;
    if (getRawDevelopmentInfo(sessionId, &rawDevelopmentInfo)) {
        printf("rawDevelopmentInfo=0x%08lx pictureControl=%s pictureControlAsShot=%s\n",
               rawDevelopmentInfo,
               (rawDevelopmentInfo & kNkfl_RawDevelopment_PictureControl) ? "yes" : "no",
               (rawDevelopmentInfo & kNkfl_RawDevelopment_PictureControlAsShot) ? "yes" : "no");
    }

    unsigned long colorProcess = 0;
    if (getColorProcess(sessionId, &colorProcess)) {
        printf("currentColorProcess=%lu\n", colorProcess);
    }

    NkflOutputProfileParam outputProfile = {};
    outputProfile.ulSize = sizeof(outputProfile);
    outputProfile.ulSessionID = sessionId;
    if (sdkCall(kNkfl_Cmd_GetOutputProfile_UTF8, &outputProfile, "GetOutputProfile")) {
        printf("outputProfile intent=%lu path=%s\n",
               outputProfile.ulRenderingIntent,
               reinterpret_cast<char *>(outputProfile.OutputProfile));
    }

    NkflOutputDeviceProfile outputDeviceProfile = {};
    outputDeviceProfile.ulSize = sizeof(outputDeviceProfile);
    outputDeviceProfile.ulSessionID = sessionId;
    if (sdkCall(kNkfl_Cmd_GetOutputDeviceProfile, &outputDeviceProfile, "GetOutputDeviceProfile")) {
        printf("outputDeviceProfile path=%s\n",
               reinterpret_cast<char *>(outputDeviceProfile.OutputDeviceProfile));
    }

    NkflPictureControlVersion version = {};
    version.ulSize = sizeof(version);
    version.ulSessionID = sessionId;
    if (sdkCall(kNkfl_Cmd_GetPictureControlVersion, &version, "GetPictureControlVersion")) {
        printf("pictureControlVersion latest=%lu modified=%lu recorded=%lu\n",
               version.ulLatestVersion,
               version.ulModifiedVersion,
               version.ulRecordedVersion);
    }

    NkflPictureControlList list = {};
    list.ulSize = sizeof(list);
    list.ulSessionID = sessionId;
    list.ulListCount = 0;
    list.pulListItems = nullptr;
    if (sdkCall(kNkfl_Cmd_GetPictureControlList, &list, "GetPictureControlList(count)")) {
        std::vector<NkflPicConListItem> items(list.ulListCount);
        list.pulListItems = items.data();
        if (sdkCall(kNkfl_Cmd_GetPictureControlList, &list, "GetPictureControlList(items)")) {
            printf("pictureControlList count=%lu ids=", list.ulListCount);
            for (unsigned long index = 0; index < list.ulListCount; ++index) {
                printf("%s0x%04lx", index == 0 ? "" : ",", items[index].ulID);
            }
            printf("\n");
        }
    }

    NkflRawDevelopment_PictureControl pictureControl = {};
    if (getPictureControl(sessionId, &pictureControl)) {
        printf("currentPictureControl id=0x%04lx quickAdjust=%g contrast=%g saturation=%g hue=%g applyLevel=%g\n",
               pictureControl.ulPictureControl,
               pictureControl.dbQuickAdjust,
               pictureControl.dbContrast,
               pictureControl.dbSaturation,
               pictureControl.dbHue,
               pictureControl.dbApplyLevel);
    }

    NkflRawDevelopment_WBAdj wb = {};
    wb.ulSize = sizeof(wb);
    if (getRawDevelopmentParam(sessionId, kNkfl_RawDevelopment_WBAdjustment, &wb, "GetRawDevelopmentParam(WBAdjustment)")) {
        printf("whiteBalance mwb=0x%04lx colorTemp=%ld rgb=(%lu,%lu,%lu)\n",
               wb.ulMWB,
               wb.lColorTemp,
               wb.rgb.ulR,
               wb.rgb.ulG,
               wb.rgb.ulB);
    }

    NkflRawDevelopment_Tint tint = {};
    tint.ulSize = sizeof(tint);
    if (getRawDevelopmentParam(sessionId, kNkfl_RawDevelopment_Tint, &tint, "GetRawDevelopmentParam(Tint)")) {
        printf("tint=%g\n", tint.lfTint);
    }

    NkflRawDevelopment_ActiveDLighting adl = {};
    adl.ulSize = sizeof(adl);
    if (getRawDevelopmentParam(sessionId, kNkfl_RawDevelopment_ActiveDLighting, &adl, "GetRawDevelopmentParam(ActiveDLighting)")) {
        printf("activeDLighting=0x%04lx\n", adl.ulActiveDLighting);
    }

    NkflRawDevelopment_ColorMode colorMode = {};
    colorMode.ulSize = sizeof(colorMode);
    if (getRawDevelopmentParam(sessionId, kNkfl_RawDevelopment_ColorMode, &colorMode, "GetRawDevelopmentParam(ColorMode)")) {
        printf("colorMode=0x%04lx\n", colorMode.ulColorMode);
    }
}

static bool pictureControlIdForName(const std::string &name, unsigned long *id) {
    if (name == "none") return false;
    if (name == "as-shot") { *id = kNkfl_PictureControl_AsShot; return true; }
    if (name == "standard") { *id = kNkfl_PictureControl_Standard; return true; }
    if (name == "neutral") { *id = kNkfl_PictureControl_Neutral; return true; }
    if (name == "vivid") { *id = kNkfl_PictureControl_Vivid; return true; }
    if (name == "monochrome") { *id = kNkfl_PictureControl_Monochrome; return true; }
    if (name == "flat") { *id = kNkfl_PictureControl_Flat; return true; }
    if (name == "auto") { *id = kNkfl_PictureControl_Auto; return true; }
    if (name == "portrait") { *id = kNkfl_OptionalPictureControl_Portrait; return true; }
    if (name == "landscape") { *id = kNkfl_OptionalPictureControl_Landscape; return true; }
    return false;
}

static bool setPictureControl(unsigned long sessionId, const std::string &name) {
    if (name == "none") {
        return true;
    }

    unsigned long pictureControlId = 0;
    if (!pictureControlIdForName(name, &pictureControlId)) {
        fprintf(stderr, "unknown --picture-control value: %s\n", name.c_str());
        return false;
    }

    NkflRawDevelopment_PictureControl pictureControl = {};
    if (!getPictureControl(sessionId, &pictureControl)) {
        return false;
    }

    pictureControl.ulPictureControl = pictureControlId;

    NkflRawDevelopmentParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.ulRawDevelopment = kNkfl_RawDevelopment_PictureControl;
    param.pData = &pictureControl;

    return sdkCallAllowWarning(kNkfl_Cmd_RawDevelopment, &param, "RawDevelopment(PictureControl)");
}

static bool getImageInfo(unsigned long sessionId, NkflImageInfoParam *info) {
    memset(info, 0, sizeof(*info));
    info->ulSize = sizeof(*info);
    info->ulSessionID = sessionId;
    return sdkCall(kNkfl_Cmd_GetImageInfo, info, "GetImageInfo");
}

static bool renderRgb8(unsigned long sessionId, const NkflImageInfoParam &info, std::vector<unsigned char> *rgb8) {
    if (info.ulWidth > SHRT_MAX || info.ulHeight > SHRT_MAX) {
        fprintf(stderr, "Image dimensions exceed SDK Rect limits: %lux%lu\n", info.ulWidth, info.ulHeight);
        return false;
    }

    const size_t channelCount = 3;
    const size_t byteCount = static_cast<size_t>(info.ulWidth) * static_cast<size_t>(info.ulHeight) * channelCount * info.ulByteDepth;
    std::vector<unsigned char> raw(byteCount);

    NkflImageParam param = {};
    param.ulSize = sizeof(param);
    param.ulSessionID = sessionId;
    param.rectArea.top = 0;
    param.rectArea.left = 0;
    param.rectArea.right = static_cast<short>(info.ulWidth);
    param.rectArea.bottom = static_cast<short>(info.ulHeight);
    param.ulDataSize = static_cast<unsigned long>(raw.size());
    param.pData = raw.data();

    if (!sdkCall(kNkfl_Cmd_GetImageData, &param, "GetImageData")) {
        return false;
    }

    if (info.ulByteDepth == 1) {
        *rgb8 = std::move(raw);
        return true;
    }

    if (info.ulByteDepth == 2) {
        rgb8->resize(raw.size() / 2);
        for (size_t source = 0, target = 0; source + 1 < raw.size(); source += 2, ++target) {
            unsigned short sample = 0;
            memcpy(&sample, raw.data() + source, sizeof(sample));
            (*rgb8)[target] = static_cast<unsigned char>(sample >> 8);
        }
        return true;
    }

    fprintf(stderr, "Unsupported byte depth: %lu\n", info.ulByteDepth);
    return false;
}

static CGColorSpaceRef createNikonSrgbColorSpace() {
    std::string profilePath = profilePathForName("NKsRGB.icm");
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(kCFAllocatorDefault, reinterpret_cast<const UInt8 *>(profilePath.c_str()), profilePath.size(), false);
    if (!url) {
        return CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    }

    CFDataRef data = nullptr;
    SInt32 errorCode = 0;
    Boolean ok = CFURLCreateDataAndPropertiesFromResource(kCFAllocatorDefault, url, &data, nullptr, nullptr, &errorCode);
    CFRelease(url);
    if (!ok || !data) {
        return CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    }

    CGColorSpaceRef colorSpace = CGColorSpaceCreateWithICCData(data);
    CFRelease(data);
    return colorSpace ?: CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
}

static bool parseLong(const char *value, long *out) {
    char *end = nullptr;
    long parsed = strtol(value, &end, 10);
    if (!end || *end != '\0') {
        return false;
    }
    *out = parsed;
    return true;
}

static bool parseUnsignedLong(const char *value, unsigned long *out) {
    char *end = nullptr;
    unsigned long parsed = strtoul(value, &end, 10);
    if (!end || *end != '\0') {
        return false;
    }
    *out = parsed;
    return true;
}

static bool parseDouble(const char *value, double *out) {
    char *end = nullptr;
    double parsed = strtod(value, &end);
    if (!end || *end != '\0') {
        return false;
    }
    *out = parsed;
    return true;
}

static bool parseOptions(int argc, char **argv, RenderOptions *options) {
    for (int index = 3; index < argc; ++index) {
        const char *arg = argv[index];
        if (strncmp(arg, "--output-profile=", 17) == 0) {
            options->outputProfileName = arg + 17;
        } else if (strncmp(arg, "--rendering-intent=", 19) == 0) {
            if (!parseUnsignedLong(arg + 19, &options->outputRenderingIntent)) {
                fprintf(stderr, "invalid --rendering-intent value: %s\n", arg + 19);
                return false;
            }
        } else if (strncmp(arg, "--output-device-profile=", 24) == 0) {
            options->outputDeviceProfileName = arg + 24;
        } else if (strncmp(arg, "--develop-color-mode=", 21) == 0) {
            if (!parseLong(arg + 21, &options->developColorMode)) {
                fprintf(stderr, "invalid --develop-color-mode value: %s\n", arg + 21);
                return false;
            }
        } else if (strncmp(arg, "--color-process=", 16) == 0) {
            if (!parseUnsignedLong(arg + 16, &options->colorProcess)) {
                fprintf(stderr, "invalid --color-process value: %s\n", arg + 16);
                return false;
            }
        } else if (strncmp(arg, "--raw-parameter-set=", 20) == 0) {
            if (strcmp(arg + 20, "as-shot") == 0) {
                options->applyRawParameterSetAsShot = true;
            } else if (strcmp(arg + 20, "none") == 0) {
                options->applyRawParameterSetAsShot = false;
            } else {
                fprintf(stderr, "invalid --raw-parameter-set value: %s\n", arg + 20);
                return false;
            }
        } else if (strncmp(arg, "--active-d-lighting=", 20) == 0) {
            options->activeDLightingName = arg + 20;
        } else if (strncmp(arg, "--tint=", 7) == 0) {
            if (!parseDouble(arg + 7, &options->tintOverride)) {
                fprintf(stderr, "invalid --tint value: %s\n", arg + 7);
                return false;
            }
            options->hasTintOverride = true;
        } else if (strncmp(arg, "--picture-control=", 18) == 0) {
            options->pictureControlName = arg + 18;
        } else if (strcmp(arg, "--apply-picture-control-as-shot") == 0) {
            options->applyPictureControlAsShot = true;
        } else if (strncmp(arg, "--order=", 8) == 0) {
            options->orderName = arg + 8;
        } else {
            fprintf(stderr, "unknown option: %s\n", arg);
            return false;
        }
    }

    return true;
}

static bool writeJpeg(const char *outputPath, const std::vector<unsigned char> &rgb, unsigned long width, unsigned long height, double quality) {
    CFURLRef outputURL = CFURLCreateFromFileSystemRepresentation(kCFAllocatorDefault, reinterpret_cast<const UInt8 *>(outputPath), strlen(outputPath), false);
    if (!outputURL) {
        fprintf(stderr, "Could not create output URL for %s\n", outputPath);
        return false;
    }

    CFDataRef imageData = CFDataCreate(kCFAllocatorDefault, rgb.data(), rgb.size());
    CGDataProviderRef provider = CGDataProviderCreateWithCFData(imageData);
    CGColorSpaceRef colorSpace = createNikonSrgbColorSpace();
    CGImageRef image = CGImageCreate(width, height, 8, 24, width * 3, colorSpace, kCGImageAlphaNone, provider, nullptr, false, kCGRenderingIntentPerceptual);
    CGImageDestinationRef destination = CGImageDestinationCreateWithURL(outputURL, CFSTR("public.jpeg"), 1, nullptr);

    bool success = false;
    if (image && destination) {
        CFNumberRef qualityNumber = CFNumberCreate(kCFAllocatorDefault, kCFNumberDoubleType, &quality);
        const void *keys[] = { kCGImageDestinationLossyCompressionQuality };
        const void *values[] = { qualityNumber };
        CFDictionaryRef properties = CFDictionaryCreate(kCFAllocatorDefault, keys, values, 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CGImageDestinationAddImage(destination, image, properties);
        success = CGImageDestinationFinalize(destination);
        CFRelease(properties);
        CFRelease(qualityNumber);
    }

    if (destination) CFRelease(destination);
    if (image) CGImageRelease(image);
    CGColorSpaceRelease(colorSpace);
    CGDataProviderRelease(provider);
    CFRelease(imageData);
    CFRelease(outputURL);

    if (!success) {
        fprintf(stderr, "Failed to write JPEG %s\n", outputPath);
    }
    return success;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s <input.nef> <output.jpg> [--output-profile=NKsRGB.icm|none] [--rendering-intent=0|1|2|3] [--output-device-profile=NKsRGB.icm|none] [--develop-color-mode=0|1|2] [--color-process=0|1] [--raw-parameter-set=as-shot|none] [--active-d-lighting=none|as-shot|off] [--tint=<value>] [--picture-control=none|as-shot|auto|standard|neutral|vivid|flat|portrait|landscape] [--apply-picture-control-as-shot] [--order=current|raw-color|adjustments-before-raw|profile-before-adjustments|profile-first]\n", argv[0]);
        return 2;
    }

    @autoreleasepool {
        RenderOptions options;
        if (!parseOptions(argc, argv, &options)) {
            return 2;
        }

         printf("options: outputProfile=%s renderingIntent=%lu outputDeviceProfile=%s developColorMode=%ld colorProcess=%lu rawParameterSet=%s activeDLighting=%s tint=%s pictureControl=%s applyPictureControlAsShot=%s order=%s\n",
               options.outputProfileName.c_str(),
             options.outputRenderingIntent,
               options.outputDeviceProfileName.c_str(),
               options.developColorMode,
               options.colorProcess,
               options.applyRawParameterSetAsShot ? "as-shot" : "none",
               options.activeDLightingName.c_str(),
               options.hasTintOverride ? std::to_string(options.tintOverride).c_str() : "as-shot",
               options.pictureControlName.c_str(),
             options.applyPictureControlAsShot ? "yes" : "no",
             options.orderName.c_str());

        if (!loadSdk() || !openLibrary(options.developColorMode)) {
            return 1;
        }

        unsigned long sessionId = 0;
        bool ok = openSession(argv[1], &sessionId);

        auto applyRawParameterSet = [&]() {
            if (!options.applyRawParameterSetAsShot) return true;
            return setRawParameterSetAsShot(sessionId);
        };
        auto applyAdjustments = [&]() {
            bool innerOk = true;
            if (innerOk) innerOk = setActiveDLighting(sessionId, options.activeDLightingName);
            if (innerOk && options.hasTintOverride) innerOk = setTint(sessionId, options.tintOverride);
            if (innerOk && options.applyPictureControlAsShot) innerOk = applyPictureControlAsShot(sessionId);
            if (innerOk) innerOk = setPictureControl(sessionId, options.pictureControlName);
            return innerOk;
        };
        auto applyProfiles = [&]() {
            bool innerOk = true;
            if (innerOk) innerOk = setOutputProfile(sessionId, options.outputProfileName, options.outputRenderingIntent);
            if (innerOk) innerOk = setOutputDeviceProfile(sessionId, options.outputDeviceProfileName);
            return innerOk;
        };

        if (options.orderName == "current") {
            if (ok) ok = setColorProcess(sessionId, options.colorProcess);
            if (ok) ok = applyRawParameterSet();
            if (ok) dumpPictureControlDiagnostics(sessionId, "after-color-raw");
            if (ok) ok = applyAdjustments();
            if (ok) ok = applyProfiles();
        } else if (options.orderName == "raw-color") {
            if (ok) ok = applyRawParameterSet();
            if (ok) ok = setColorProcess(sessionId, options.colorProcess);
            if (ok) dumpPictureControlDiagnostics(sessionId, "after-raw-color");
            if (ok) ok = applyAdjustments();
            if (ok) ok = applyProfiles();
        } else if (options.orderName == "profile-before-adjustments") {
            if (ok) ok = setColorProcess(sessionId, options.colorProcess);
            if (ok) ok = applyRawParameterSet();
            if (ok) ok = applyProfiles();
            if (ok) dumpPictureControlDiagnostics(sessionId, "after-color-raw-profile");
            if (ok) ok = applyAdjustments();
        } else if (options.orderName == "adjustments-before-raw") {
            if (ok) ok = setColorProcess(sessionId, options.colorProcess);
            if (ok) ok = applyAdjustments();
            if (ok) dumpPictureControlDiagnostics(sessionId, "after-color-adjustments");
            if (ok) ok = applyRawParameterSet();
            if (ok) ok = applyProfiles();
        } else if (options.orderName == "profile-first") {
            if (ok) ok = applyProfiles();
            if (ok) ok = setColorProcess(sessionId, options.colorProcess);
            if (ok) ok = applyRawParameterSet();
            if (ok) dumpPictureControlDiagnostics(sessionId, "after-profile-color-raw");
            if (ok) ok = applyAdjustments();
        } else {
            fprintf(stderr, "unknown --order value: %s\n", options.orderName.c_str());
            ok = false;
        }

        if (ok) dumpPictureControlDiagnostics(sessionId, "after-setters");

        NkflImageInfoParam info = {};
        if (ok) ok = getImageInfo(sessionId, &info);

        std::vector<unsigned char> rgb8;
        if (ok) ok = renderRgb8(sessionId, info, &rgb8);
        if (ok) ok = writeJpeg(argv[2], rgb8, info.ulWidth, info.ulHeight, 0.85);

        if (sessionId != 0) {
            closeSession(sessionId);
        }
        closeLibrary();

        if (!ok) {
            return 1;
        }

        printf("converted %s -> %s (%lux%lu, %lu byte/channel)\n", argv[1], argv[2], info.ulWidth, info.ulHeight, info.ulByteDepth);
        return 0;
    }
}