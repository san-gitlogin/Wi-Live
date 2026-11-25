//! The tray icons (crates/wilive/assets, drawn by brand/make-icons.sh).

use crate::model::Icon;

/// RGBA pixels, width, height, and whether macOS should colour it as a template.
pub fn rgba(kind: Icon) -> (Vec<u8>, u32, u32, bool) {
    #[cfg(target_os = "macos")]
    let (bytes, template): (&[u8], bool) = match kind {
        Icon::Idle => (include_bytes!("../assets/mac-idle-44.png"), true),
        Icon::Live => (include_bytes!("../assets/mac-live-44.png"), false),
    };
    #[cfg(not(target_os = "macos"))]
    let (bytes, template): (&[u8], bool) = match kind {
        Icon::Idle => (include_bytes!("../assets/color-idle-64.png"), false),
        Icon::Live => (include_bytes!("../assets/color-live-64.png"), false),
    };
    let dec = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut r = dec.read_info().expect("bundled icon");
    let mut buf = vec![0; r.output_buffer_size().expect("icon size")];
    let info = r.next_frame(&mut buf).expect("bundled icon");
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height, template)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows app icon (build.rs puts it in the .exe as icon 1): every size
    /// Windows asks for, each a real image of that size - a missing small size is
    /// drawn blurry in the title bar and taskbar.
    #[test]
    fn the_windows_app_icon_has_every_size() {
        let ico: &[u8] = include_bytes!("../assets/app.ico");
        let u16_at = |o: usize| u16::from_le_bytes([ico[o], ico[o + 1]]) as usize;
        let u32_at = |o: usize| u32::from_le_bytes(ico[o..o + 4].try_into().unwrap()) as usize;
        assert_eq!((u16_at(0), u16_at(2)), (0, 1), "an .ico file");
        let mut sizes = Vec::new();
        for i in 0..u16_at(4) {
            let d = 6 + 16 * i;
            let side = if ico[d] == 0 { 256 } else { ico[d] as u32 };
            let (len, at) = (u32_at(d + 8), u32_at(d + 12));
            let dec = png::Decoder::new(std::io::Cursor::new(&ico[at..at + len]));
            let info = dec.read_info().expect("each size is a PNG").info().clone();
            assert_eq!(
                (info.width, info.height),
                (side, side),
                "the image is the size it claims"
            );
            sizes.push(side);
        }
        sizes.sort();
        assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]);
    }

    #[test]
    fn every_icon_decodes_to_square_rgba() {
        for k in [Icon::Idle, Icon::Live] {
            let (px, w, h, _) = rgba(k);
            assert_eq!(px.len(), (w * h * 4) as usize);
            assert!(w >= 22 && w == h);
        }
    }
}
