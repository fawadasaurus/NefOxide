use std::error::Error;
use std::ffi::{CString, c_void};
use std::fmt;
use std::mem::size_of;
use std::os::raw::c_ulong;
use std::path::{Path, PathBuf};
use std::ptr;

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/nkfl_bindings.rs"));
}

#[derive(Debug)]
pub enum NikonError {
    PathEncoding,
    ImageTooLarge {
        width: u64,
        height: u64,
    },
    UnsupportedByteDepth(c_ulong),
    Sdk {
        operation: &'static str,
        code: c_ulong,
    },
}

impl fmt::Display for NikonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathEncoding => write!(
                formatter,
                "path cannot be passed to Nikon SDK as UTF-8 C string"
            ),
            Self::ImageTooLarge { width, height } => write!(
                formatter,
                "image is too large for the Nikon SDK Rect: {width}x{height}"
            ),
            Self::UnsupportedByteDepth(byte_depth) => {
                write!(formatter, "unsupported Nikon SDK byte depth: {byte_depth}")
            }
            Self::Sdk { operation, code } => write!(
                formatter,
                "Nikon SDK {operation} failed with code 0x{code:04x}"
            ),
        }
    }
}

impl Error for NikonError {}

pub type Result<T> = std::result::Result<T, NikonError>;

#[derive(Debug, Clone, Copy)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub byte_depth: u32,
    pub color: u64,
    pub orientation: u64,
    pub resolution: f64,
}

#[derive(Debug)]
pub struct RgbImage {
    pub info: ImageInfo,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct RawDevelopmentParams {
    pub supported_mask: u64,
    pub raw_parameter_set: Option<u64>,
    pub raw_quality: Option<u64>,
    pub exposure_compensation: Option<f64>,
    pub white_balance: Option<WhiteBalanceParams>,
    pub tint: Option<f64>,
    pub noise_reduction: Option<u64>,
    pub picture_control: Option<PictureControlParams>,
    pub color_mode: Option<u64>,
    pub sharpness: Option<u64>,
    pub tone_compensation: Option<u64>,
    pub saturation: Option<u64>,
    pub hue_adjustment: Option<i64>,
    pub filter_effect: Option<u64>,
    pub active_d_lighting: Option<u64>,
    pub dehaze: Option<u64>,
    pub film_grain: Option<FilmGrainParams>,
}

#[derive(Debug)]
pub struct WhiteBalanceParams {
    pub mode: u64,
    pub color_temperature: i64,
    pub red: u64,
    pub green: u64,
    pub blue: u64,
}

#[derive(Debug)]
pub struct PictureControlParams {
    pub id: u64,
    pub apply_quick_adjust: bool,
    pub quick_adjust: f64,
    pub sharpness_auto: bool,
    pub sharpness: f64,
    pub clarity_auto: bool,
    pub clarity: f64,
    pub user_defined_curve: bool,
    pub contrast_auto: bool,
    pub contrast: f64,
    pub brightness: f64,
    pub highlight: f64,
    pub shadow: f64,
    pub saturation_auto: bool,
    pub saturation: f64,
    pub hue: f64,
    pub filter: i64,
    pub toning: u64,
    pub toning_intensity: f64,
    pub apply_level: f64,
    pub apply_quick_sharp: bool,
    pub quick_sharp_auto: bool,
    pub quick_sharp: f64,
    pub middle_range_sharp: f64,
    pub flexible_contrast: f64,
    pub flexible_highlight: f64,
    pub flexible_shadow: f64,
    pub flexible_saturation: f64,
}

#[derive(Debug)]
pub struct FilmGrainParams {
    pub intensity: u64,
    pub size: u64,
}

pub struct NikonLibrary {
    _private: (),
}

impl NikonLibrary {
    pub fn open() -> Result<Self> {
        let mut library_handle: sys::NkflPtr = ptr::null_mut();
        let mut param = sys::NkflLibraryParam {
            ulSize: size_of::<sys::NkflLibraryParam>() as c_ulong,
            ulVersion: 0x01000000,
            ulVMMemorySize: 1024,
            pNkflPtr: &mut library_handle,
            VMFileInfo: [0; 1024],
        };

        let vm_path = b"/tmp/nefoxide-nikon-sdk-vm.tmp\0";
        param.VMFileInfo[..vm_path.len()].copy_from_slice(vm_path);

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_OpenLibrary as c_ulong,
            &mut param,
            "OpenLibrary",
        )?;
        set_develop_color_mode_applied_in_camera()?;
        Ok(Self { _private: () })
    }

    pub fn open_session<'library, P: AsRef<Path>>(
        &'library self,
        path: P,
    ) -> Result<NikonSession<'library>> {
        let path = path.as_ref().to_string_lossy();
        let path = CString::new(path.as_bytes()).map_err(|_| NikonError::PathEncoding)?;
        let mut param = sys::NkflSessionParam {
            ulSize: size_of::<sys::NkflSessionParam>() as c_ulong,
            ulSessionID: 0,
            ulType: sys::eNkflSource_kNkfl_Source_FileName_UTF8 as c_ulong,
            pFileInfo: path.as_ptr() as *mut c_void,
            ulFileSize: 0,
            bImageLoadSkip: false,
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_OpenSession as c_ulong,
            &mut param,
            "OpenSession",
        )?;
        let session = NikonSession {
            _library: self,
            session_id: param.ulSessionID,
        };
        session.apply_as_shot_raw_parameters()?;
        session.apply_in_camera_color_process()?;
        session.set_output_profile_srgb()?;
        Ok(session)
    }
}

impl Drop for NikonLibrary {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::Nkfl_Entry(
                sys::eNkflCommand_kNkfl_Cmd_CloseLibrary as c_ulong,
                ptr::null_mut(),
            );
        }
    }
}

pub struct NikonSession<'library> {
    _library: &'library NikonLibrary,
    session_id: c_ulong,
}

impl NikonSession<'_> {
    pub fn raw_params(&self) -> Result<RawDevelopmentParams> {
        let supported_mask = self.raw_development_mask()?;

        Ok(RawDevelopmentParams {
            supported_mask,
            raw_parameter_set: self.raw_parameter_set(supported_mask)?,
            raw_quality: self.raw_quality(supported_mask)?,
            exposure_compensation: self.exposure_compensation(supported_mask)?,
            white_balance: self.white_balance(supported_mask)?,
            tint: self.tint(supported_mask)?,
            noise_reduction: self.noise_reduction(supported_mask)?,
            picture_control: self.picture_control(supported_mask)?,
            color_mode: self.color_mode(supported_mask)?,
            sharpness: self.sharpness(supported_mask)?,
            tone_compensation: self.tone_compensation(supported_mask)?,
            saturation: self.saturation(supported_mask)?,
            hue_adjustment: self.hue_adjustment(supported_mask)?,
            filter_effect: self.filter_effect(supported_mask)?,
            active_d_lighting: self.active_d_lighting(supported_mask)?,
            dehaze: self.dehaze(supported_mask)?,
            film_grain: self.film_grain(supported_mask)?,
        })
    }

    fn raw_development_mask(&self) -> Result<u64> {
        let mut info = sys::NkflRawDevelopmentInfo {
            ulSize: size_of::<sys::NkflRawDevelopmentInfo>() as c_ulong,
            ulSessionID: self.session_id,
            ulRawDevelopmentInfo: 0,
        };
        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_GetRawDevelopmentInfo as c_ulong,
            &mut info,
            "GetRawDevelopmentInfo",
        )?;
        Ok(info.ulRawDevelopmentInfo as u64)
    }

    fn raw_parameter_set(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_RawParameterSet,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_RawParameterSet {
            ulSize: size_of::<sys::NkflRawDevelopment_RawParameterSet>() as c_ulong,
            ulParamterSet: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_RawParameterSet,
            &mut param,
            "GetRawDevelopmentParam(RawParameterSet)",
        )?;
        Ok(Some(param.ulParamterSet as u64))
    }

    fn raw_quality(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_RawQuality,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_RawQuality {
            ulSize: size_of::<sys::NkflRawDevelopment_RawQuality>() as c_ulong,
            ulQuality: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_RawQuality,
            &mut param,
            "GetRawDevelopmentParam(RawQuality)",
        )?;
        Ok(Some(param.ulQuality as u64))
    }

    fn exposure_compensation(&self, supported_mask: u64) -> Result<Option<f64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ExpComp,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_ExpComp {
            ulSize: size_of::<sys::NkflRawDevelopment_ExpComp>() as c_ulong,
            dbExpComp: 0.0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ExpComp,
            &mut param,
            "GetRawDevelopmentParam(ExpComp)",
        )?;
        Ok(Some(param.dbExpComp))
    }

    fn white_balance(&self, supported_mask: u64) -> Result<Option<WhiteBalanceParams>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_WBAdjustment,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_WBAdj {
            ulSize: size_of::<sys::NkflRawDevelopment_WBAdj>() as c_ulong,
            ..Default::default()
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_WBAdjustment,
            &mut param,
            "GetRawDevelopmentParam(WBAdjustment)",
        )?;
        Ok(Some(WhiteBalanceParams {
            mode: param.ulMWB as u64,
            color_temperature: param.lColorTemp as i64,
            red: param.rgb.ulR as u64,
            green: param.rgb.ulG as u64,
            blue: param.rgb.ulB as u64,
        }))
    }

    fn tint(&self, supported_mask: u64) -> Result<Option<f64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Tint,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_Tint {
            ulSize: size_of::<sys::NkflRawDevelopment_Tint>() as c_ulong,
            lfTint: 0.0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Tint,
            &mut param,
            "GetRawDevelopmentParam(Tint)",
        )?;
        Ok(Some(param.lfTint))
    }

    fn noise_reduction(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_NR,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_NR {
            ulSize: size_of::<sys::NkflRawDevelopment_NR>() as c_ulong,
            ulNRType: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_NR,
            &mut param,
            "GetRawDevelopmentParam(NR)",
        )?;
        Ok(Some(param.ulNRType as u64))
    }

    fn picture_control(&self, supported_mask: u64) -> Result<Option<PictureControlParams>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_PictureControl,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_PictureControl {
            ulSize: size_of::<sys::NkflRawDevelopment_PictureControl>() as c_ulong,
            ..Default::default()
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_PictureControl,
            &mut param,
            "GetRawDevelopmentParam(PictureControl)",
        )?;
        Ok(Some(PictureControlParams {
            id: param.ulPictureControl as u64,
            apply_quick_adjust: param.bApplyQuickAdjust,
            quick_adjust: param.dbQuickAdjust,
            sharpness_auto: param.bSharpessAuto,
            sharpness: param.dbSharpness,
            clarity_auto: param.bClarityAuto,
            clarity: param.dbClarity,
            user_defined_curve: param.bUserDefinedCurve,
            contrast_auto: param.bContrastAuto,
            contrast: param.dbContrast,
            brightness: param.dbBrightness,
            highlight: param.dbHighlight,
            shadow: param.dbShadow,
            saturation_auto: param.bSaturationAuto,
            saturation: param.dbSaturation,
            hue: param.dbHue,
            filter: param.lFilter as i64,
            toning: param.lToning as u64,
            toning_intensity: param.dbToningIntensity,
            apply_level: param.dbApplyLevel,
            apply_quick_sharp: param.bApplyQuickSharp,
            quick_sharp_auto: param.bQuickSharpAuto,
            quick_sharp: param.dbQuickSharp,
            middle_range_sharp: param.dbMiddleRangeSharp,
            flexible_contrast: param.flc_dbParams.dbContrast,
            flexible_highlight: param.flc_dbParams.dbHighlight,
            flexible_shadow: param.flc_dbParams.dbShadow,
            flexible_saturation: param.flc_dbParams.dbSaturation,
        }))
    }

    fn color_mode(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ColorMode,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_ColorMode {
            ulSize: size_of::<sys::NkflRawDevelopment_ColorMode>() as c_ulong,
            ulColorMode: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ColorMode,
            &mut param,
            "GetRawDevelopmentParam(ColorMode)",
        )?;
        Ok(Some(param.ulColorMode as u64))
    }

    fn sharpness(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Sharpness,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_Sharpness {
            ulSize: size_of::<sys::NkflRawDevelopment_Sharpness>() as c_ulong,
            ulSharpness: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Sharpness,
            &mut param,
            "GetRawDevelopmentParam(Sharpness)",
        )?;
        Ok(Some(param.ulSharpness as u64))
    }

    fn tone_compensation(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ToneComp,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_ToneComp {
            ulSize: size_of::<sys::NkflRawDevelopment_ToneComp>() as c_ulong,
            ulToneComp: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ToneComp,
            &mut param,
            "GetRawDevelopmentParam(ToneComp)",
        )?;
        Ok(Some(param.ulToneComp as u64))
    }

    fn saturation(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Saturation,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_Saturation {
            ulSize: size_of::<sys::NkflRawDevelopment_Saturation>() as c_ulong,
            ulSaturation: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Saturation,
            &mut param,
            "GetRawDevelopmentParam(Saturation)",
        )?;
        Ok(Some(param.ulSaturation as u64))
    }

    fn hue_adjustment(&self, supported_mask: u64) -> Result<Option<i64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_HueAdjustment,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_HueAdj {
            ulSize: size_of::<sys::NkflRawDevelopment_HueAdj>() as c_ulong,
            ulHueAdj: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_HueAdjustment,
            &mut param,
            "GetRawDevelopmentParam(HueAdjustment)",
        )?;
        Ok(Some(param.ulHueAdj as i64))
    }

    fn filter_effect(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_FilterEffect,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_FilterEffect {
            ulSize: size_of::<sys::NkflRawDevelopment_FilterEffect>() as c_ulong,
            ulFilterEffect: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_FilterEffect,
            &mut param,
            "GetRawDevelopmentParam(FilterEffect)",
        )?;
        Ok(Some(param.ulFilterEffect as u64))
    }

    fn active_d_lighting(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ActiveDLighting,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_ActiveDLighting {
            ulSize: size_of::<sys::NkflRawDevelopment_ActiveDLighting>() as c_ulong,
            ulActiveDLighting: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_ActiveDLighting,
            &mut param,
            "GetRawDevelopmentParam(ActiveDLighting)",
        )?;
        Ok(Some(param.ulActiveDLighting as u64))
    }

    fn dehaze(&self, supported_mask: u64) -> Result<Option<u64>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Dehaze,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_Dehaze {
            ulSize: size_of::<sys::NkflRawDevelopment_Dehaze>() as c_ulong,
            ulDehaze: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_Dehaze,
            &mut param,
            "GetRawDevelopmentParam(Dehaze)",
        )?;
        Ok(Some(param.ulDehaze as u64))
    }

    fn film_grain(&self, supported_mask: u64) -> Result<Option<FilmGrainParams>> {
        if !is_supported(
            supported_mask,
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_FilmGrain,
        ) {
            return Ok(None);
        }
        let mut param = sys::NkflRawDevelopment_FilmGrain {
            ulSize: size_of::<sys::NkflRawDevelopment_FilmGrain>() as c_ulong,
            ulFilmGrainIntensity: 0,
            ulFilmGrainSize: 0,
        };
        self.get_raw_development_param(
            sys::eNkflRawDevelopment_kNkfl_RawDevelopment_FilmGrain,
            &mut param,
            "GetRawDevelopmentParam(FilmGrain)",
        )?;
        Ok(Some(FilmGrainParams {
            intensity: param.ulFilmGrainIntensity as u64,
            size: param.ulFilmGrainSize as u64,
        }))
    }

    fn get_raw_development_param<T>(
        &self,
        flag: sys::eNkflRawDevelopment,
        data: &mut T,
        operation: &'static str,
    ) -> Result<()> {
        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_GetRawDevelopmentParam as c_ulong,
            &mut sys::NkflRawDevelopmentParam {
                ulSize: size_of::<sys::NkflRawDevelopmentParam>() as c_ulong,
                ulSessionID: self.session_id,
                ulRawDevelopment: flag as c_ulong,
                pData: data as *mut T as *mut c_void,
            },
            operation,
        )
    }

    fn set_output_profile_srgb(&self) -> Result<()> {
        let profile_path = repo_root()
            .join("lib")
            .join("NikonSDK")
            .join("Profiles")
            .join("NKsRGB.icm");
        let profile_path = CString::new(profile_path.to_string_lossy().as_bytes())
            .map_err(|_| NikonError::PathEncoding)?;
        let profile_bytes = profile_path.as_bytes_with_nul();
        let mut output_profile = sys::NkflOutputProfileParam {
            ulSize: size_of::<sys::NkflOutputProfileParam>() as c_ulong,
            ulSessionID: self.session_id,
            ulRenderingIntent: sys::eNkflRenderingIntent_kNkfl_RenderingIntent_Perceptual
                as c_ulong,
            OutputProfile: [0; 1024],
        };
        output_profile.OutputProfile[..profile_bytes.len()].copy_from_slice(profile_bytes);

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_SetOutputProfile_UTF8 as c_ulong,
            &mut output_profile,
            "SetOutputProfile",
        )
    }

    fn apply_in_camera_color_process(&self) -> Result<()> {
        let mut color_process = sys::NkflColorProcess {
            ulSize: size_of::<sys::NkflColorProcess>() as c_ulong,
            ulSessionID: self.session_id,
            ulColorProcess: sys::eNkflColorProcess_kNkfl_ColorProcess_AppliedInCamera as c_ulong,
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_SetColorProcess as c_ulong,
            &mut color_process,
            "SetColorProcess",
        )
    }

    fn apply_as_shot_raw_parameters(&self) -> Result<()> {
        let mut raw_parameter_set = sys::NkflRawDevelopment_RawParameterSet {
            ulSize: size_of::<sys::NkflRawDevelopment_RawParameterSet>() as c_ulong,
            ulParamterSet: sys::eNkflRawParameterSet_kNkfl_RawParameterSet_AsShot as c_ulong,
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_RawDevelopment as c_ulong,
            &mut sys::NkflRawDevelopmentParam {
                ulSize: size_of::<sys::NkflRawDevelopmentParam>() as c_ulong,
                ulSessionID: self.session_id,
                ulRawDevelopment: sys::eNkflRawDevelopment_kNkfl_RawDevelopment_RawParameterSet
                    as c_ulong,
                pData: &mut raw_parameter_set as *mut _ as *mut c_void,
            },
            "RawDevelopment(RawParameterSet)",
        )
    }

    #[allow(dead_code)]
    fn apply_current_picture_control(&self) -> Result<()> {
        let mut picture_control = sys::NkflRawDevelopment_PictureControl {
            ulSize: size_of::<sys::NkflRawDevelopment_PictureControl>() as c_ulong,
            ..Default::default()
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_GetRawDevelopmentParam as c_ulong,
            &mut sys::NkflRawDevelopmentParam {
                ulSize: size_of::<sys::NkflRawDevelopmentParam>() as c_ulong,
                ulSessionID: self.session_id,
                ulRawDevelopment: sys::eNkflRawDevelopment_kNkfl_RawDevelopment_PictureControl
                    as c_ulong,
                pData: &mut picture_control as *mut _ as *mut c_void,
            },
            "GetRawDevelopmentParam(PictureControl)",
        )?;

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_RawDevelopment as c_ulong,
            &mut sys::NkflRawDevelopmentParam {
                ulSize: size_of::<sys::NkflRawDevelopmentParam>() as c_ulong,
                ulSessionID: self.session_id,
                ulRawDevelopment: sys::eNkflRawDevelopment_kNkfl_RawDevelopment_PictureControl
                    as c_ulong,
                pData: &mut picture_control as *mut _ as *mut c_void,
            },
            "RawDevelopment(PictureControl)",
        )
    }

    pub fn image_info(&self) -> Result<ImageInfo> {
        let mut param = sys::NkflImageInfoParam {
            ulSize: size_of::<sys::NkflImageInfoParam>() as c_ulong,
            ulSessionID: self.session_id,
            ulImageID: 0,
            ulWidth: 0,
            ulHeight: 0,
            ulByteDepth: 0,
            ulColor: 0,
            ulOrientation: 0,
            dbResolution: 0.0,
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_GetImageInfo as c_ulong,
            &mut param,
            "GetImageInfo",
        )?;
        Ok(ImageInfo {
            width: param.ulWidth as u32,
            height: param.ulHeight as u32,
            byte_depth: param.ulByteDepth as u32,
            color: param.ulColor as u64,
            orientation: param.ulOrientation as u64,
            resolution: param.dbResolution,
        })
    }

    pub fn render_rgb8(&self) -> Result<RgbImage> {
        let info = self.image_info()?;
        if info.width > i16::MAX as u32 || info.height > i16::MAX as u32 {
            return Err(NikonError::ImageTooLarge {
                width: info.width as u64,
                height: info.height as u64,
            });
        }

        let channel_count = 3usize;
        let sample_bytes = info.byte_depth as usize;
        let byte_count = info.width as usize * info.height as usize * channel_count * sample_bytes;
        let mut raw = vec![0u8; byte_count];
        let mut param = sys::NkflImageParam {
            ulSize: size_of::<sys::NkflImageParam>() as c_ulong,
            ulSessionID: self.session_id,
            ulImageID: 0,
            rectArea: sys::Rect {
                top: 0,
                left: 0,
                bottom: info.height as i16,
                right: info.width as i16,
            },
            ulDataSize: raw.len() as c_ulong,
            pData: raw.as_mut_ptr() as *mut c_void,
            pFunc: None,
            pProgressParam: ptr::null_mut(),
        };

        sdk_call(
            sys::eNkflCommand_kNkfl_Cmd_GetImageData as c_ulong,
            &mut param,
            "GetImageData",
        )?;
        let data = match info.byte_depth {
            1 => raw,
            2 => raw
                .chunks_exact(2)
                .map(|sample| u16::from_ne_bytes([sample[0], sample[1]]) >> 8)
                .map(|sample| sample as u8)
                .collect(),
            byte_depth => return Err(NikonError::UnsupportedByteDepth(byte_depth as c_ulong)),
        };

        Ok(RgbImage { info, data })
    }
}

impl Drop for NikonSession<'_> {
    fn drop(&mut self) {
        if self.session_id == 0 {
            return;
        }

        let mut param = sys::NkflSessionParam {
            ulSize: size_of::<sys::NkflSessionParam>() as c_ulong,
            ulSessionID: self.session_id,
            ulType: 0,
            pFileInfo: ptr::null_mut(),
            ulFileSize: 0,
            bImageLoadSkip: false,
        };

        unsafe {
            let _ = sys::Nkfl_Entry(
                sys::eNkflCommand_kNkfl_Cmd_CloseSession as c_ulong,
                &mut param as *mut _ as *mut c_void,
            );
        }
    }
}

fn sdk_call<T>(command: c_ulong, param: &mut T, operation: &'static str) -> Result<()> {
    let code = unsafe { sys::Nkfl_Entry(command, param as *mut T as *mut c_void) };
    if code == sys::eNkflCode_kNkfl_Code_None as c_ulong {
        Ok(())
    } else {
        Err(NikonError::Sdk { operation, code })
    }
}

fn is_supported(supported_mask: u64, flag: sys::eNkflRawDevelopment) -> bool {
    supported_mask & flag as u64 != 0
}

fn set_develop_color_mode_applied_in_camera() -> Result<()> {
    let mut mode = sys::NkflDevelopColorMode {
        ulSize: size_of::<sys::NkflDevelopColorMode>() as c_ulong,
        lDevelopColorMode: sys::eNkflDevelopColorMode_kNkfl_DevelopColorMode_AppliedInCamera.into(),
    };

    sdk_call(
        sys::eNkflCommand_kNkfl_Cmd_SetDevelopColorMode as c_ulong,
        &mut mode,
        "SetDevelopColorMode",
    )
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate is under repo_root/crates")
        .to_path_buf()
}
