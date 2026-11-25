//! The QR code the owner's window shows for each address, so a phone opens it with
//! its camera instead of typing it. Drawn as SVG (crisp at any size), delivered as a
//! data: URI the page puts in an <img> (its CSP allows data: images, nothing else).

use qrcode::{Color, EcLevel, QrCode};

/// SVG markup for `text`: dark modules on white with a 4-module quiet zone (the
/// standard's minimum; phones fail to read a code without it).
pub fn svg(text: &str) -> Option<String> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).ok()?;
    let w = code.width();
    let colors = code.to_colors();
    let q = 4;
    let size = w + 2 * q;
    let mut path = String::new();
    for y in 0..w {
        let mut x = 0;
        while x < w {
            if colors[y * w + x] == Color::Dark {
                let start = x;
                while x < w && colors[y * w + x] == Color::Dark {
                    x += 1;
                }
                path.push_str(&format!(
                    "M{} {}h{}v1h-{}z",
                    start + q,
                    y + q,
                    x - start,
                    x - start
                ));
            } else {
                x += 1;
            }
        }
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" shape-rendering=\"crispEdges\"><rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#1A1414\"/></svg>"
    ))
}

/// As a data: URI (percent-encoding only what a URI cannot carry).
pub fn data_uri(text: &str) -> Option<String> {
    let s = svg(text)?;
    let mut out = String::from("data:image/svg+xml;utf8,");
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '"' => out.push('\''),
            c => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_becomes_a_small_square_code() {
        let s = svg("http://192.168.0.103:47900/").unwrap();
        assert!(
            s.starts_with("<svg") && s.contains("viewBox=\"0 0 37 37\""),
            "version 3 (29) + quiet zone: {}",
            &s[..80]
        );
        let d = data_uri("http://192.168.0.103:47900/").unwrap();
        assert!(
            d.starts_with("data:image/svg+xml;utf8,%3Csvg") && !d.contains('#') && !d.contains('"')
        );
    }
}
