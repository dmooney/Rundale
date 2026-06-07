//! Integration proof for #1301: the shipping screenshot guard rejects a blank
//! capture instead of reporting it as a success.
//!
//! This exercises the *real, public* guard (`parish_tauri_lib::commands::
//! screenshot::reject_blank_capture` / `classify_png_content`) — the exact
//! code path an MCP `parish_take_screenshot` flows through after a frame is
//! produced — against full-resolution synthetic frames at the dimensions
//! reported in the bug (3024x1898). No mocks, no test-only re-implementation:
//! if the guard regressed, this binary fails.

use parish_tauri_lib::commands::screenshot::{
    PngContent, classify_png_content, reject_blank_capture,
};

/// The exact full resolution from the #1301 report.
const W: u32 = 3024;
const H: u32 = 1898;

/// Encodes a full-res RGBA PNG from a per-pixel colour closure.
fn make_png(mut color: impl FnMut(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((W as usize) * (H as usize) * 4);
    for y in 0..H {
        for x in 0..W {
            raw.extend_from_slice(&color(x, y));
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(std::io::Cursor::new(&mut out), W, H);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&raw).unwrap();
    }
    out
}

#[test]
fn blank_white_full_res_capture_is_rejected() {
    // The #1301 failure shape: a valid, full-resolution, solid-white PNG.
    let blank = make_png(|_, _| [0xff, 0xff, 0xff, 0xff]);

    // It decodes fine and is a non-trivial number of bytes, so the old envelope
    // checks (decode-ok + nonzero-length) would all pass.
    assert!(blank.len() > 1000, "blank frame is a real, sizeable PNG");

    // The shipping content guard nonetheless classifies it as blank and rejects.
    assert_eq!(classify_png_content(&blank).unwrap(), PngContent::Blank);
    let err = reject_blank_capture(&blank).expect_err("blank capture must be Err");
    assert!(
        err.contains("blank"),
        "guard error must name the blank-capture cause: {err}"
    );
}

#[test]
fn content_bearing_full_res_capture_is_accepted() {
    // A real render with varied pixels passes the guard unchanged.
    let content = make_png(|x, y| {
        if (x ^ y) & 1 == 0 {
            [0x12, 0x55, 0xaa, 0xff]
        } else {
            [0xe0, 0xd0, 0x40, 0xff]
        }
    });
    assert_eq!(
        classify_png_content(&content).unwrap(),
        PngContent::HasContent
    );
    reject_blank_capture(&content).expect("content capture must pass the guard");
}

#[test]
fn undecodable_bytes_are_not_silently_blank() {
    // A capture we cannot even decode must surface an error, never be reported
    // as a success and never be silently swallowed as "blank" (#1301 AC #5).
    let err = reject_blank_capture(b"\x89PNG\r\n\x1a\n garbage tail").unwrap_err();
    assert!(
        err.contains("decode") || err.contains("PNG"),
        "undecodable bytes must error on the decode, not pass: {err}"
    );
}
