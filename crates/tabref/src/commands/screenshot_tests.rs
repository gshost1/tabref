use std::path::Path;

use tabref_page::CapturedImage;

use super::{saved, shown_size};

/// A PNG header for a `width` by `height` image, padded to `len` bytes.
fn image(width: u32, height: u32, len: usize, device_scale: f64) -> CapturedImage {
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.resize(len.max(png.len()), 0);
    CapturedImage { png, device_scale }
}

#[test]
fn sizes_step_from_bytes_to_kilobytes_to_megabytes() {
    assert_eq!(shown_size(500), "500 bytes");
    assert_eq!(shown_size(2048), "2.0 KB");
    assert_eq!(shown_size(3 * 1024 * 1024 + 512 * 1024), "3.5 MB");
}

#[test]
fn the_result_names_the_file_its_pixels_and_a_scale_other_than_one() {
    let plain = saved(
        Path::new("/w/a.png"),
        false,
        &image(120, 80, 2048, 1.0),
        None,
    )
    .unwrap();
    assert_eq!(
        plain.text,
        "saved a screenshot of the page: /w/a.png (120x80 px, 2.0 KB)"
    );
    let dense = saved(
        Path::new("/w/a.png"),
        true,
        &image(240, 160, 900, 2.0),
        Some("\"Red\""),
    )
    .unwrap();
    assert_eq!(
        dense.text,
        "replaced /w/a.png with a screenshot of \"Red\" (240x160 px at 2x, 900 bytes)"
    );
    assert_eq!(dense.data["replaced"], true);
    assert_eq!(dense.data["device_scale"], 2.0);
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_is_reported_without_a_panic() {
    use std::os::unix::ffi::OsStrExt;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"/w/caf\xe9.png"));
    let outcome = saved(path, false, &image(1, 1, 40, 1.0), None).unwrap();
    assert_eq!(outcome.data["path"], "/w/caf\u{fffd}.png");
}
