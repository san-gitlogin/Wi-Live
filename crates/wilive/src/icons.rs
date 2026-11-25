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

    #[test]
    fn every_icon_decodes_to_square_rgba() {
        for k in [Icon::Idle, Icon::Live] {
            let (px, w, h, _) = rgba(k);
            assert_eq!(px.len(), (w * h * 4) as usize);
            assert!(w >= 22 && w == h);
        }
    }
}
