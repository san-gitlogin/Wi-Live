//! The dashboard's files, built into the app. Nothing is fetched from the internet:
//! the page must work on a home network with no connection out.

use crate::http::Response;

pub struct Asset {
    pub path: &'static str,
    pub ctype: &'static str,
    pub body: &'static [u8],
}

macro_rules! asset {
    ($path:expr, $ctype:expr, $file:expr) => {
        Asset { path: $path, ctype: $ctype, body: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/", $file)) }
    };
}

pub static ASSETS: &[Asset] = &[
    asset!("/", "text/html; charset=utf-8", "index.html"),
    asset!("/app.css", "text/css; charset=utf-8", "app.css"),
    asset!("/app.js", "text/javascript; charset=utf-8", "app.js"),
    asset!("/crypto.js", "text/javascript; charset=utf-8", "crypto.js"),
    asset!("/logo.svg", "image/svg+xml", "logo.svg"),
    asset!("/fonts/inter.woff2", "font/woff2", "fonts/inter.woff2"),
    asset!("/fonts/jbmono.woff2", "font/woff2", "fonts/jbmono.woff2"),
];

/// FNV-1a of the bytes: an ETag that changes when the file does.
fn etag(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("\"{h:016x}\"")
}

/// The file for `path`, or a 304 when the browser already has it. Always
/// revalidated (`no-cache`), so an updated app never serves yesterday's script.
pub fn serve(path: &str, if_none_match: Option<&str>) -> Option<Response> {
    let a = ASSETS.iter().find(|a| a.path == path || (path == "/index.html" && a.path == "/"))?;
    let tag = etag(a.body);
    let headers = vec![("Cache-Control", "no-cache".to_string()), ("ETag", tag.clone())];
    if if_none_match == Some(tag.as_str()) {
        return Some(Response { status: 304, ctype: a.ctype, body: Vec::new(), headers });
    }
    Some(Response { status: 200, ctype: a.ctype, body: a.body.to_vec(), headers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_served_and_revalidates_to_304() {
        for a in ASSETS {
            let r = serve(a.path, None).unwrap();
            assert_eq!(r.status, 200, "{}", a.path);
            assert!(!r.body.is_empty(), "{}", a.path);
            let tag = r.headers.iter().find(|(k, _)| *k == "ETag").unwrap().1.clone();
            assert_eq!(serve(a.path, Some(&tag)).unwrap().status, 304);
        }
        assert!(serve("/nope", None).is_none());
        assert!(serve("/../Cargo.toml", None).is_none());
    }

    #[test]
    fn the_whole_dashboard_stays_small() {
        let code: usize = ASSETS.iter().filter(|a| !a.path.starts_with("/fonts")).map(|a| a.body.len()).sum();
        assert!(code < 150 * 1024, "the page's own files are {code} bytes (R-05: under 150 KB)");
    }
}
