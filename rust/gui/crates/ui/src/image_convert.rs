//! HEIC/HEIF 解码：`image` crate 不支持 HEIC，内置纯 Rust 解码器兜底，
//! 各平台系统解码器（ImageIO/WIC/Android）优先或作为回退。
//! 抖音/小红书的实况图片既可能是 HEIC 原图，也可能是 HEIC 缩略图。

use std::path::Path;

/// ISO BMFF `ftyp` 中需要系统解码器处理的图片品牌。
pub fn needs_platform_decoder(data: &[u8]) -> bool {
    if data.len() < 12 || &data[4..8] != b"ftyp" {
        return false;
    }
    matches!(
        &data[8..12],
        b"heic"
            | b"heix"
            | b"hevc"
            | b"hevx"
            | b"heim"
            | b"heis"
            | b"hevm"
            | b"hevs"
            | b"mif1"
            | b"msf1"
            | b"avif"
            | b"avis"
    )
}

/// 把 `source` 转成 JPEG 写入 `target`。
/// `max_dimension` 为 0 表示保持原始分辨率。
pub fn convert_to_jpeg(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
    // Windows 的 WIC 只有装了可选的 HEIF/HEVC 扩展才能解 HEIC，
    // 所以优先用内置解码器，系统解码器留给 AVIF 等格式。
    #[cfg(target_os = "windows")]
    {
        if builtin_heif::convert(source, target, max_dimension, quality) {
            return true;
        }
        windows_imaging::convert(source, target, max_dimension, quality)
    }
    #[cfg(target_os = "android")]
    {
        android::convert(source, target, max_dimension, quality)
            || builtin_heif::convert(source, target, max_dimension, quality)
    }
    #[cfg(target_os = "ios")]
    {
        ios::convert(source, target, max_dimension, quality)
            || builtin_heif::convert(source, target, max_dimension, quality)
    }
    #[cfg(target_os = "macos")]
    {
        macos::convert(source, target, max_dimension, quality)
            || builtin_heif::convert(source, target, max_dimension, quality)
    }
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "windows",
        target_os = "macos"
    )))]
    {
        builtin_heif::convert(source, target, max_dimension, quality)
    }
}

/// 内置纯 Rust HEIC 解码，不依赖系统 HEIF 扩展。
mod builtin_heif {
    use std::io::BufWriter;
    use std::path::Path;

    use image::imageops::FilterType;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let Ok(data) = std::fs::read(source) else {
            return false;
        };
        let Ok(decoded) = heic_decoder::decode(&data) else {
            return false;
        };
        let Ok(rgb) = decoded.to_rgb8() else {
            return false;
        };
        let Some(image) = image::RgbImage::from_raw(rgb.width as u32, rgb.height as u32, rgb.data)
        else {
            return false;
        };

        let longest = image.width().max(image.height());
        let image = if max_dimension > 0 && longest > max_dimension {
            let ratio = max_dimension as f64 / longest as f64;
            let width = ((image.width() as f64 * ratio).round() as u32).max(1);
            let height = ((image.height() as f64 * ratio).round() as u32).max(1);
            image::imageops::resize(&image, width, height, FilterType::Triangle)
        } else {
            image
        };

        let Ok(file) = std::fs::File::create(target) else {
            return false;
        };
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            BufWriter::new(file),
            quality.clamp(1, 100),
        );
        match encoder.encode_image(&image) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("failed to encode HEIC as JPEG: {error}");
                let _ = std::fs::remove_file(target);
                false
            }
        }
    }
}

/// macOS 桌面端用系统自带的 sips（ImageIO）转码。
#[cfg(target_os = "macos")]
mod macos {
    use std::path::Path;
    use std::process::Command;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let mut command = Command::new("/usr/bin/sips");
        command
            .arg("-s")
            .arg("format")
            .arg("jpeg")
            .arg("-s")
            .arg("formatOptions")
            .arg(quality.clamp(1, 100).to_string());
        if max_dimension > 0 {
            command.arg("-Z").arg(max_dimension.to_string());
        }
        let output = command.arg(source).arg("--out").arg(target).output();
        match output {
            Ok(output) => output.status.success() && target.is_file(),
            Err(_) => false,
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    use gpui_android::android::jni as android_jni;
    use jni::objects::JValue;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let (Some(source), Some(target)) = (source.to_str(), target.to_str()) else {
            return false;
        };
        let result = android_jni::with_env(|env| {
            let activity = android_jni::activity(env)?;
            let source = env.new_string(source).map_err(|error| error.to_string())?;
            let target = env.new_string(target).map_err(|error| error.to_string())?;
            env.call_method(
                &activity,
                jni::jni_str!("convertImageToJpeg"),
                jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;II)Z"),
                &[
                    JValue::Object(&source),
                    JValue::Object(&target),
                    JValue::Int(max_dimension as i32),
                    JValue::Int(quality as i32),
                ],
            )
            .and_then(|result| result.z())
            .map_err(|error| error.to_string())
        });
        if let Err(error) = &result {
            eprintln!("failed to convert image via Android decoder: {error}");
        }
        matches!(result, Ok(true))
    }
}

#[cfg(target_os = "windows")]
mod windows_imaging {
    use std::path::Path;

    use windows::core::{Interface, HSTRING, PWSTR};
    use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, RPC_E_CHANGED_MODE};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, IWICBitmapEncoder,
        IWICBitmapFlipRotator, IWICBitmapFrameDecode, IWICBitmapFrameEncode, IWICBitmapScaler,
        IWICBitmapSource, IWICImagingFactory, WICBitmapEncoderNoCache,
        WICBitmapInterpolationModeFant, WICBitmapTransformFlipHorizontal,
        WICBitmapTransformFlipVertical, WICBitmapTransformOptions, WICBitmapTransformRotate180,
        WICBitmapTransformRotate270, WICBitmapTransformRotate90, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::StructuredStorage::{IPropertyBag2, PROPBAG2, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::{
        VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4, VT_UI2,
    };

    const ORIENTATION_QUERY: &str = "System.Photo.Orientation";
    const QUALITY_PROPERTY: &str = "ImageQuality";

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let (Some(source), Some(target)) = (source.to_str(), target.to_str()) else {
            return false;
        };
        let _com = match ComGuard::new() {
            Ok(guard) => guard,
            Err(()) => return false,
        };
        let result = unsafe { convert_inner(source, target, max_dimension, quality) };
        if let Err(error) = &result {
            eprintln!("failed to convert image via WIC: {error}");
        }
        result.is_ok()
    }

    struct ComGuard(Option<windows::core::HRESULT>);

    impl ComGuard {
        fn new() -> Result<Self, ()> {
            let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if status == RPC_E_CHANGED_MODE {
                return Ok(Self(None));
            }
            if status.is_err() {
                return Err(());
            }
            Ok(Self(Some(status)))
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0.is_some() {
                unsafe { CoUninitialize() };
            }
        }
    }

    unsafe fn convert_inner(
        source: &str,
        target: &str,
        max_dimension: u32,
        quality: u8,
    ) -> windows::core::Result<()> {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(source),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;

        let decoded: IWICBitmapSource = match frame_orientation(&frame) {
            Some(transform) => {
                let rotator: IWICBitmapFlipRotator = factory.CreateBitmapFlipRotator()?;
                rotator.Initialize(&frame, transform)?;
                rotator.cast()?
            }
            None => frame.cast()?,
        };

        let mut width = 0u32;
        let mut height = 0u32;
        decoded.GetSize(&mut width, &mut height)?;
        let longest = width.max(height);
        let result: IWICBitmapSource = if max_dimension > 0 && longest > max_dimension {
            let ratio = max_dimension as f64 / longest as f64;
            let scaled_width = ((width as f64 * ratio).round() as u32).max(1);
            let scaled_height = ((height as f64 * ratio).round() as u32).max(1);
            let scaler: IWICBitmapScaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(
                &decoded,
                scaled_width,
                scaled_height,
                WICBitmapInterpolationModeFant,
            )?;
            scaler.cast()?
        } else {
            decoded
        };

        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(target), GENERIC_WRITE.0)?;
        let encoder: IWICBitmapEncoder =
            factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;

        let mut frame_encode: Option<IWICBitmapFrameEncode> = None;
        let mut options: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame_encode, &mut options)?;
        let (Some(frame_encode), Some(options)) = (frame_encode, options) else {
            return Err(windows::core::Error::empty());
        };
        set_quality(&options, quality);
        frame_encode.Initialize(&options)?;
        frame_encode.WriteSource(&result, std::ptr::null())?;
        frame_encode.Commit()?;
        encoder.Commit()?;
        Ok(())
    }

    unsafe fn frame_orientation(
        frame: &IWICBitmapFrameDecode,
    ) -> Option<WICBitmapTransformOptions> {
        let reader = frame.GetMetadataQueryReader().ok()?;
        let mut value = PROPVARIANT::default();
        reader
            .GetMetadataByName(&HSTRING::from(ORIENTATION_QUERY), &mut value)
            .ok()?;
        if value.Anonymous.Anonymous.vt != VT_UI2 {
            return None;
        }
        match value.Anonymous.Anonymous.Anonymous.uiVal {
            2 => Some(WICBitmapTransformFlipHorizontal),
            3 => Some(WICBitmapTransformRotate180),
            4 => Some(WICBitmapTransformFlipVertical),
            5 => Some(WICBitmapTransformOptions(
                WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0,
            )),
            6 => Some(WICBitmapTransformRotate90),
            7 => Some(WICBitmapTransformOptions(
                WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0,
            )),
            8 => Some(WICBitmapTransformRotate270),
            _ => None,
        }
    }

    unsafe fn set_quality(options: &IPropertyBag2, quality: u8) {
        let mut name: Vec<u16> = QUALITY_PROPERTY
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut property: PROPBAG2 = std::mem::zeroed();
        property.pstrName = PWSTR(name.as_mut_ptr());
        let value = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_R4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        fltVal: quality.clamp(1, 100) as f32,
                    },
                }),
            },
        };
        let _ = options.Write(1, &property, &value);
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use std::ffi::CString;
    use std::path::Path;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let (Some(source), Some(target)) = (source.to_str(), target.to_str()) else {
            return false;
        };
        let (Ok(source), Ok(target)) = (CString::new(source), CString::new(target)) else {
            return false;
        };
        unsafe {
            extern "C" {
                fn reels_convert_image_to_jpeg(
                    source: *const std::os::raw::c_char,
                    target: *const std::os::raw::c_char,
                    max_dimension: i32,
                    quality: i32,
                ) -> bool;
            }
            reels_convert_image_to_jpeg(
                source.as_ptr(),
                target.as_ptr(),
                max_dimension as i32,
                quality as i32,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_heic_brands() {
        let mut heic = vec![0u8; 16];
        heic[4..8].copy_from_slice(b"ftyp");
        heic[8..12].copy_from_slice(b"heic");
        assert!(needs_platform_decoder(&heic));

        let mut avif = vec![0u8; 16];
        avif[4..8].copy_from_slice(b"ftyp");
        avif[8..12].copy_from_slice(b"avif");
        assert!(needs_platform_decoder(&avif));

        let mut jpeg = vec![0u8; 16];
        jpeg[0..2].copy_from_slice(&[0xFF, 0xD8]);
        assert!(!needs_platform_decoder(&jpeg));
        assert!(!needs_platform_decoder(b"short"));
    }
}
