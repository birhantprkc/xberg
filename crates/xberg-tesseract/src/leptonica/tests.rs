use super::*;

const TEST_X_RESOLUTION: i32 = 240;
const TEST_Y_RESOLUTION: i32 = 180;

fn make_rgb_pix(width: u32, height: u32, fill: u8) -> Pix {
    let data = vec![fill; (width * height * 3) as usize];
    Pix::from_raw_rgb(&data, width, height).expect("from_raw_rgb failed")
}

fn gray_pixel(pix: &Pix, x: i32, y: i32) -> f64 {
    pix.clip_rectangle(x, y, 1, 1)
        .expect("clip failed")
        .mean_gray_value(1)
        .expect("pixel read failed")
}

fn gray_fixture(width: u32, height: u32) -> Pix {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = 48 + ((x * 160) / width) as u8;
            let value = if (x + y * 17).is_multiple_of(53) { 255 } else { value };
            data.extend_from_slice(&[value, value, value]);
        }
    }
    make_gray_pix_with_resolution(&data, width, height)
}

fn make_gray_pix_with_resolution(data: &[u8], width: u32, height: u32) -> Pix {
    let mut pix = Pix::from_raw_rgb(data, width, height)
        .expect("from_raw_rgb failed")
        .to_grayscale()
        .expect("to_grayscale failed");
    pix.set_resolution(TEST_X_RESOLUTION, TEST_Y_RESOLUTION)
        .expect("set_resolution failed");
    pix
}

#[test]
fn test_from_raw_rgb_dimensions() {
    let pix = make_rgb_pix(16, 8, 200);
    assert_eq!(pix.width(), 16);
    assert_eq!(pix.height(), 8);
    assert_eq!(pix.depth(), 32);
}

#[test]
fn test_from_raw_rgb_wrong_length() {
    let data = vec![0u8; 10];
    let err = Pix::from_raw_rgb(&data, 4, 4).unwrap_err();
    assert!(matches!(err, TesseractError::InvalidImageData));
}

#[test]
fn test_from_raw_rgb_zero_dimensions() {
    let err = Pix::from_raw_rgb(&[], 0, 4).unwrap_err();
    assert!(matches!(err, TesseractError::InvalidImageData));

    let err = Pix::from_raw_rgb(&[], 4, 0).unwrap_err();
    assert!(matches!(err, TesseractError::InvalidImageData));
}

#[test]
fn test_as_ptr_is_non_null() {
    let pix = make_rgb_pix(8, 8, 128);
    assert!(!pix.as_ptr().is_null());
}

#[test]
fn test_to_grayscale() {
    let pix = make_rgb_pix(32, 32, 150);
    let gray = pix.to_grayscale().expect("to_grayscale failed");
    assert_eq!(gray.width(), 32);
    assert_eq!(gray.height(), 32);
    assert_eq!(gray.depth(), 8);
}

#[test]
fn test_scale_up() {
    let pix = make_rgb_pix(20, 10, 100);
    let scaled = pix.scale(2.0, 2.0).expect("scale failed");
    assert_eq!(scaled.width(), 40);
    assert_eq!(scaled.height(), 20);
}

#[test]
fn test_unsharp_mask_returns_same_dimensions() {
    let pix = make_rgb_pix(32, 32, 200);
    let sharpened = pix.unsharp_mask(2, 0.4).expect("unsharp_mask failed");
    assert_eq!(sharpened.width(), 32);
    assert_eq!(sharpened.height(), 32);
}

#[test]
fn test_adaptive_threshold_produces_1bpp() {
    let pix = make_rgb_pix(64, 64, 180);
    let gray = pix.to_grayscale().expect("to_grayscale failed");
    let binary = gray.adaptive_threshold(32, 32).expect("adaptive_threshold failed");
    assert_eq!(binary.depth(), 1);
}

#[test]
fn median_filter_removes_single_pixel_noise_and_preserves_resolution() {
    const WIDTH: u32 = 9;
    const HEIGHT: u32 = 9;
    let mut data = vec![200; (WIDTH * HEIGHT * 3) as usize];
    let center = ((HEIGHT / 2 * WIDTH + WIDTH / 2) * 3) as usize;
    data[center..center + 3].fill(0);
    let gray = make_gray_pix_with_resolution(&data, WIDTH, HEIGHT);

    let filtered = gray.median_filter(3, 3).expect("median_filter failed");

    assert_eq!(gray_pixel(&filtered, 4, 4), 200.0);
    assert_eq!(
        filtered.get_resolution().unwrap(),
        (TEST_X_RESOLUTION, TEST_Y_RESOLUTION)
    );
}

#[test]
fn contrast_stretch_changes_dynamic_range_and_preserves_resolution() {
    let width = 3;
    let data = [50, 50, 50, 125, 125, 125, 200, 200, 200];
    let gray = make_gray_pix_with_resolution(&data, width, 1);

    let stretched = gray.contrast_stretch(1.0, 50, 200).expect("contrast_stretch failed");

    assert_eq!(gray_pixel(&stretched, 0, 0), 0.0);
    assert_eq!(gray_pixel(&stretched, 2, 0), 255.0);
    assert_eq!(
        stretched.get_resolution().unwrap(),
        (TEST_X_RESOLUTION, TEST_Y_RESOLUTION)
    );
}

#[test]
fn global_otsu_threshold_produces_binary_and_preserves_resolution() {
    let gray = gray_fixture(96, 64);

    let binary = gray.otsu_threshold().expect("otsu_threshold failed");

    assert_eq!(binary.depth(), 1);
    assert_eq!(binary.get_resolution().unwrap(), (TEST_X_RESOLUTION, TEST_Y_RESOLUTION));
}

#[test]
fn adaptive_otsu_threshold_produces_binary_and_preserves_resolution() {
    let gray = gray_fixture(96, 64);

    let binary = gray.adaptive_threshold(32, 32).expect("adaptive_threshold failed");

    assert_eq!(binary.depth(), 1);
    assert_eq!(binary.get_resolution().unwrap(), (TEST_X_RESOLUTION, TEST_Y_RESOLUTION));
}

#[test]
fn tiled_sauvola_threshold_produces_binary_and_preserves_resolution() {
    let gray = gray_fixture(96, 64);

    let binary = gray.sauvola_threshold(7, 0.35, 2, 2).expect("sauvola_threshold failed");

    assert_eq!(binary.depth(), 1);
    assert_eq!(binary.get_resolution().unwrap(), (TEST_X_RESOLUTION, TEST_Y_RESOLUTION));
}

#[test]
fn preprocessing_wrappers_reject_invalid_parameters_before_ffi() {
    let rgb = make_rgb_pix(32, 32, 128);
    let gray = rgb.to_grayscale().expect("to_grayscale failed");

    assert!(matches!(
        rgb.median_filter(3, 3),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.median_filter(0, 3),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.contrast_stretch(f32::NAN, 0, 255),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.contrast_stretch(1.0, 200, 100),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.adaptive_threshold(8, 32),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.sauvola_threshold(1, 0.35, 1, 1),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.sauvola_threshold(7, -0.1, 1, 1),
        Err(TesseractError::InvalidParameterError)
    ));
    assert!(matches!(
        gray.sauvola_threshold(7, 0.35, 0, 1),
        Err(TesseractError::InvalidParameterError)
    ));
}

#[test]
fn test_invert_flips_uniform_gray_value() {
    let pix = make_rgb_pix(16, 16, 30);
    let gray = pix.to_grayscale().expect("to_grayscale failed");
    let inverted = gray.invert().expect("invert failed");

    assert_eq!(inverted.width(), gray.width());
    assert_eq!(inverted.height(), gray.height());
    assert_eq!(inverted.depth(), 8);

    let original_mean = gray.mean_gray_value(1).expect("mean_gray_value failed");
    let inverted_mean = inverted.mean_gray_value(1).expect("mean_gray_value failed");
    assert!((original_mean + inverted_mean - 255.0).abs() < 1.0);
}

#[test]
fn test_invert_twice_round_trips() {
    let pix = make_rgb_pix(16, 16, 90);
    let gray = pix.to_grayscale().expect("to_grayscale failed");

    let once = gray.invert().expect("first invert failed");
    let twice = once.invert().expect("second invert failed");

    let original_mean = gray.mean_gray_value(1).expect("mean_gray_value failed");
    let round_tripped_mean = twice.mean_gray_value(1).expect("mean_gray_value failed");
    assert!((original_mean - round_tripped_mean).abs() < 1.0);
}

#[test]
fn test_mean_gray_value_light_vs_dark() {
    let light = make_rgb_pix(8, 8, 220).to_grayscale().expect("to_grayscale failed");
    let dark = make_rgb_pix(8, 8, 20).to_grayscale().expect("to_grayscale failed");

    let light_mean = light.mean_gray_value(1).expect("mean_gray_value failed");
    let dark_mean = dark.mean_gray_value(1).expect("mean_gray_value failed");

    assert!(light_mean > 200.0, "light_mean was {light_mean}");
    assert!(dark_mean < 40.0, "dark_mean was {dark_mean}");
}
