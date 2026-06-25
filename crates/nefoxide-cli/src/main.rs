use std::env;
use std::error::Error;
use std::path::PathBuf;

use mozjpeg_rs::{Encoder, Preset, Subsampling};
use nefoxide_nikon::NikonLibrary;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("convert") => {
            let input = args.next().ok_or("missing input NEF path")?;
            let output = args.next().ok_or("missing output JPG path")?;
            if args.next().is_some() {
                return Err("usage: nefoxide-cli convert <input.nef> <output.jpg>".into());
            }

            convert(PathBuf::from(input), PathBuf::from(output))
        }
        Some("compare") => {
            let ours = args.next().ok_or("missing first image path")?;
            let reference = args.next().ok_or("missing reference image path")?;
            if args.next().is_some() {
                return Err("usage: nefoxide-cli compare <ours.jpg> <reference.jpg>".into());
            }

            compare(PathBuf::from(ours), PathBuf::from(reference))
        }
        Some("raw-params") => {
            let input = args.next().ok_or("missing input NEF path")?;
            if args.next().is_some() {
                return Err("usage: nefoxide-cli raw-params <input.nef>".into());
            }

            raw_params(PathBuf::from(input))
        }
        _ => Err("usage: nefoxide-cli <convert <input.nef> <output.jpg>|compare <ours.jpg> <reference.jpg>|raw-params <input.nef>>".into()),
    }
}

fn convert(input: PathBuf, output: PathBuf) -> Result<(), Box<dyn Error>> {
    let library = NikonLibrary::open()?;
    let session = library.open_session(&input)?;
    let image = session.render_rgb8()?;
    let icc_profile = std::fs::read(nikon_srgb_profile_path())?;

    let jpeg = Encoder::new(Preset::BaselineBalanced)
        .quality(95)
        .subsampling(Subsampling::S444)
        .icc_profile(icc_profile)
        .encode_rgb(&image.data, image.info.width, image.info.height)?;
    std::fs::write(&output, jpeg)?;

    println!(
        "converted {} -> {} ({}x{}, {} byte/channel)",
        input.display(),
        output.display(),
        image.info.width,
        image.info.height,
        image.info.byte_depth,
    );
    Ok(())
}

fn nikon_srgb_profile_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate is under repo_root/crates")
        .join("lib")
        .join("NikonSDK")
        .join("Profiles")
        .join("NKsRGB.icm")
}

fn raw_params(input: PathBuf) -> Result<(), Box<dyn Error>> {
    let library = NikonLibrary::open()?;
    let session = library.open_session(&input)?;
    let params = session.raw_params()?;

    println!("raw development params for {}:", input.display());
    println!("{params:#?}");
    Ok(())
}

fn compare(ours: PathBuf, reference: PathBuf) -> Result<(), Box<dyn Error>> {
    let ours_image = image::open(&ours)?.to_rgb8();
    let reference_image = image::open(&reference)?.to_rgb8();
    if ours_image.dimensions() != reference_image.dimensions() {
        return Err(format!(
            "image dimensions differ: {} is {:?}, {} is {:?}",
            ours.display(),
            ours_image.dimensions(),
            reference.display(),
            reference_image.dimensions()
        )
        .into());
    }

    let mut all_count = 0u64;
    let mut all_delta = [0i128; 3];
    let mut orange_count = 0u64;
    let mut orange_delta = [0i128; 3];
    let mut orange_ours = [0u128; 3];
    let mut orange_reference = [0u128; 3];

    for (ours_pixel, reference_pixel) in ours_image.pixels().zip(reference_image.pixels()) {
        all_count += 1;
        for channel in 0..3 {
            all_delta[channel] += reference_pixel[channel] as i128 - ours_pixel[channel] as i128;
        }

        let [red, green, blue] = ours_pixel.0;
        let is_orange = red > 120
            && green > 45
            && green < 190
            && blue < 130
            && red as f32 > green as f32 * 1.12
            && green as f32 > blue as f32 * 1.05;

        if is_orange {
            orange_count += 1;
            for channel in 0..3 {
                orange_delta[channel] +=
                    reference_pixel[channel] as i128 - ours_pixel[channel] as i128;
                orange_ours[channel] += ours_pixel[channel] as u128;
                orange_reference[channel] += reference_pixel[channel] as u128;
            }
        }
    }

    println!("all pixels: {all_count}");
    println!(
        "average delta reference-ours RGB: {}",
        format_i128_average(all_delta, all_count)
    );
    println!("orange-ish pixels: {orange_count}");
    if orange_count > 0 {
        println!(
            "average ours orange RGB: {}",
            format_u128_average(orange_ours, orange_count)
        );
        println!(
            "average reference orange RGB: {}",
            format_u128_average(orange_reference, orange_count)
        );
        println!(
            "average orange delta reference-ours RGB: {}",
            format_i128_average(orange_delta, orange_count)
        );
    }

    Ok(())
}

fn format_i128_average(values: [i128; 3], count: u64) -> String {
    format!(
        "{:.2}, {:.2}, {:.2}",
        values[0] as f64 / count as f64,
        values[1] as f64 / count as f64,
        values[2] as f64 / count as f64
    )
}

fn format_u128_average(values: [u128; 3], count: u64) -> String {
    format!(
        "{:.2}, {:.2}, {:.2}",
        values[0] as f64 / count as f64,
        values[1] as f64 / count as f64,
        values[2] as f64 / count as f64
    )
}
