//! Screen capture on macOS with ScreenCaptureKit (macOS 12.3+; the older
//! CGDisplay APIs are deprecated, and CGDisplayCreateImage is gone in 15).
//! Through the `objc2` bindings, so building needs no Swift and no recent Xcode.
//!
//! ScreenCaptureKit pushes a frame only when the screen changed (status
//! `Complete`; an unchanged screen arrives as `Idle` with no pixels) and
//! scales on the GPU, so a Retina display is captured straight at the size the
//! phone needs. The output object keeps only the newest frame: the pipeline
//! takes it when it is ready, and anything older is replaced, never queued.
//!
//! Needs the Screen Recording permission. Without it `start` fails with a
//! message the tray turns into "Allow screen recording".

use crate::frame::Frame;
use crate::pipeline::Source;
use block2::RcBlock;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFRelease, CFRetain, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef};
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamFrameInfoStatus, SCStreamOutput, SCStreamOutputType, SCWindow,
};
use std::ffi::c_void;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The longest edge sent; a Retina screen is scaled down to this on the GPU.
/// The largest a capture gets (a viewer chose Sharp); see [`crate::edge`].
pub const MAX_EDGE: u32 = crate::edge::SHARP;
const PIXEL_BGRA: u32 = u32::from_be_bytes(*b"BGRA");
// SCFrameStatus values (SCStream.h): Complete = 0, Started = 4.
const STATUS_COMPLETE: i64 = 0;
const STATUS_STARTED: i64 = 4;

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMSampleBufferGetImageBuffer(sb: *mut c_void) -> *mut c_void;
    fn CMSampleBufferGetSampleAttachmentsArray(sb: *mut c_void, create: u8) -> CFArrayRef;
}
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferLockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(pb: *mut c_void) -> *mut c_void;
    fn CVPixelBufferGetBytesPerRow(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
}
const LOCK_READ_ONLY: u64 = 1;

/// A retained CVPixelBuffer, released on drop.
struct Pixels(*mut c_void);
// SAFETY: CVPixelBuffers are reference counted and thread safe; access to the
// pixels is under lock/unlock.
unsafe impl Send for Pixels {}
impl Drop for Pixels {
    fn drop(&mut self) {
        // SAFETY: retained once when stored.
        unsafe { CFRelease(self.0 as CFTypeRef) }
    }
}

#[derive(Default)]
struct Latest {
    pixels: Option<Pixels>,
    fresh: bool,
    /// Rung on every new frame (the pipeline takes it at once).
    waker: Arc<crate::pipeline::Waker>,
}

struct Ivars {
    latest: Arc<Mutex<Latest>>,
}

/// The frame status ScreenCaptureKit attaches to each sample, if any.
unsafe fn frame_status(sb: *mut c_void) -> Option<i64> {
    let atts = CMSampleBufferGetSampleAttachmentsArray(sb, 0);
    if atts.is_null() || CFArrayGetCount(atts) < 1 {
        return None;
    }
    let dict = CFArrayGetValueAtIndex(atts, 0) as CFDictionaryRef;
    if dict.is_null() {
        return None;
    }
    let key: &objc2_foundation::NSString = SCStreamFrameInfoStatus;
    let v = CFDictionaryGetValue(dict, key as *const _ as *const c_void) as CFNumberRef;
    if v.is_null() {
        return None;
    }
    let mut out: i64 = -1;
    if CFNumberGetValue(v, kCFNumberSInt64Type, &mut out as *mut i64 as *mut c_void) {
        Some(out)
    } else {
        None
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HidFiCaptureOutput"]
    #[ivars = Ivars]
    struct Output;

    unsafe impl NSObjectProtocol for Output {}

    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            let sb = sample as *const CMSampleBuffer as *mut c_void;
            // SAFETY: a valid sample for the duration of this call.
            unsafe {
                match frame_status(sb) {
                    Some(STATUS_COMPLETE) | Some(STATUS_STARTED) => {}
                    _ => return, // Idle, Blank, Suspended: no new pixels
                }
                let pb = CMSampleBufferGetImageBuffer(sb);
                if pb.is_null() {
                    return;
                }
                CFRetain(pb as CFTypeRef);
                let px = Pixels(pb);
                if let Ok(mut l) = self.ivars().latest.lock() {
                    l.pixels = Some(px); // the old one is released here
                    l.fresh = true;
                    l.waker.ring();
                }
            }
        }
    }
);

impl Output {
    fn new(latest: Arc<Mutex<Latest>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { latest });
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Output size for a display of `w`x`h` pixels: the same aspect, longest edge
/// at most [`MAX_EDGE`], even numbers (H.264).
pub fn fit(w: u32, h: u32) -> (u32, u32) {
    crate::edge::fit(w, h, crate::edge::get())
}

fn shareable() -> Result<Retained<SCShareableContent>, String> {
    let (tx, rx) = mpsc::channel::<Result<Retained<SCShareableContent>, String>>();
    let tx = Mutex::new(Some(tx));
    let block = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        let r = if !err.is_null() || content.is_null() {
            // SAFETY: a valid NSError when non-null.
            let msg = unsafe { err.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_default();
            Err(if msg.is_empty() {
                "screen recording is not allowed".to_string()
            } else {
                msg
            })
        } else {
            // SAFETY: a valid object for the duration of the block; retained to outlive it.
            unsafe { Retained::retain(content) }.ok_or_else(|| "no content".to_string())
        };
        if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
            let _ = tx.send(r);
        }
    });
    // SAFETY: the block is retained by the call until it runs.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "the system did not list the screens".to_string())?
}

/// The displays the user can choose from: (id, width, height) in points.
pub fn displays() -> Result<Vec<(u32, u32, u32)>, String> {
    let content = shareable()?;
    // SAFETY: plain property reads on a live object.
    unsafe {
        Ok(content
            .displays()
            .iter()
            .map(|d| {
                (
                    d.displayID(),
                    d.width().max(0) as u32,
                    d.height().max(0) as u32,
                )
            })
            .collect())
    }
}

pub struct ScreenSource {
    stream: Retained<SCStream>,
    _output: Retained<Output>,
    _queue: dispatch2::DispatchRetained<DispatchQueue>,
    latest: Arc<Mutex<Latest>>,
    width: u32,
    height: u32,
}

// SAFETY: SCStream is thread safe for start/stop; the output only touches the
// mutex-guarded state.
unsafe impl Send for ScreenSource {}

impl ScreenSource {
    /// Capture `display_id`, or the main (first) display if `None`.
    pub fn start(display_id: Option<u32>, fps: u32) -> Result<Self, String> {
        let content = shareable()?;
        // SAFETY: property reads and documented initialisers on live objects.
        unsafe {
            let displays = content.displays();
            let display: Retained<SCDisplay> = match display_id {
                Some(id) => displays.iter().find(|d| d.displayID() == id),
                None => displays.iter().next(),
            }
            .ok_or("no display to capture")?;
            // Points to pixels: at most 2x on Retina; fit() caps the result.
            let (w, h) = fit(
                display.width().max(1) as u32 * 2,
                display.height().max(1) as u32 * 2,
            );
            let none: Retained<NSArray<SCWindow>> = NSArray::new();
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &none,
            );
            let config = SCStreamConfiguration::new();
            config.setWidth(w as usize);
            config.setHeight(h as usize);
            config.setPixelFormat(PIXEL_BGRA);
            config.setShowsCursor(true);
            // The shortest queue ScreenCaptureKit allows: fewer frames waiting in it.
            config.setQueueDepth(3);
            config.setScalesToFit(true);
            config.setMinimumFrameInterval(CMTime {
                value: 1,
                timescale: fps.clamp(1, 60) as i32,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                None,
            );
            let latest: Arc<Mutex<Latest>> = Arc::default();
            let output = Output::new(latest.clone());
            let queue = DispatchQueue::new("live.wilive.capture", None);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| e.localizedDescription().to_string())?;
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let tx = Mutex::new(Some(tx));
            let done = RcBlock::new(move |err: *mut NSError| {
                let r = match err.as_ref() {
                    None => Ok(()),
                    Some(e) => Err(e.localizedDescription().to_string()),
                };
                if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                    let _ = tx.send(r);
                }
            });
            stream.startCaptureWithCompletionHandler(Some(&done));
            rx.recv_timeout(Duration::from_secs(10))
                .map_err(|_| "capture did not start".to_string())?
                .map_err(|e| format!("could not start capture ({e})"))?;
            Ok(ScreenSource {
                stream,
                _output: output,
                _queue: queue,
                latest,
                width: w,
                height: h,
            })
        }
    }
}

impl Source for ScreenSource {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn waker(&self) -> Option<Arc<crate::pipeline::Waker>> {
        self.latest.lock().ok().map(|l| l.waker.clone())
    }

    fn grab(&mut self, force: bool, f: &mut dyn FnMut(&Frame)) -> Result<bool, String> {
        let pb = {
            let mut l = self.latest.lock().map_err(|_| "capture state poisoned")?;
            if !l.fresh && !force {
                return Ok(false);
            }
            l.fresh = false;
            match &l.pixels {
                // Retain for our use, so a newer frame replacing it cannot free it.
                // SAFETY: a live, retained buffer.
                Some(p) => unsafe {
                    CFRetain(p.0 as CFTypeRef);
                    Pixels(p.0)
                },
                None => return Ok(false), // nothing captured yet
            }
        };
        // SAFETY: the buffer is retained (pb) and locked while read.
        unsafe {
            if CVPixelBufferLockBaseAddress(pb.0, LOCK_READ_ONLY) != 0 {
                return Err("could not lock the frame".into());
            }
            let base = CVPixelBufferGetBaseAddress(pb.0) as *const u8;
            let (w, h, stride) = (
                CVPixelBufferGetWidth(pb.0) as u32,
                CVPixelBufferGetHeight(pb.0) as u32,
                CVPixelBufferGetBytesPerRow(pb.0),
            );
            if !base.is_null() && w > 0 && h > 0 {
                let px = std::slice::from_raw_parts(base, stride * h as usize);
                self.width = w;
                self.height = h;
                f(&Frame {
                    width: w,
                    height: h,
                    stride,
                    bgra: px,
                });
            }
            CVPixelBufferUnlockBaseAddress(pb.0, LOCK_READ_ONLY);
        }
        Ok(true)
    }
}

impl Drop for ScreenSource {
    fn drop(&mut self) {
        // SAFETY: stopping a live stream; no completion handler needed.
        unsafe { self.stream.stopCaptureWithCompletionHandler(None) };
    }
}

/// Whether this process may record the screen, without asking.
pub fn allowed() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    // SAFETY: no arguments, no side effects.
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Shows the system prompt (once per app; afterwards System Settings decides).
pub fn ask() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    // SAFETY: no arguments.
    unsafe { CGRequestScreenCaptureAccess() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_caps_the_long_edge() {
        use crate::edge::{AUTO, SHARP};
        let f = crate::edge::fit;
        assert_eq!(f(3024, 1964, AUTO), (1920, 1246), "fast by default");
        assert_eq!(f(3024, 1964, SHARP), (3024, 1964), "Sharp: its own size");
        assert_eq!(f(1920, 1080, AUTO), (1920, 1080));
        assert_eq!(f(1280, 800, AUTO), (1280, 800));
        assert_eq!(f(1080, 1920, AUTO), (1080, 1920));
        assert_eq!(f(2160, 3840, SHARP), (2160, 3840));
        assert_eq!(f(1366, 768, AUTO), (1366, 768));
        assert_eq!(f(1, 1, AUTO), (16, 16));
        assert_eq!(f(5120, 1440, SHARP), (3840, 1080));
        crate::edge::set(SHARP);
        assert_eq!(
            fit(3024, 1964),
            (3024, 1964),
            "the capturing thread's choice"
        );
        crate::edge::set(AUTO);
    }

    // Opt-in: without permission, listing screens can make macOS show its
    // Screen Recording prompt on the desktop. Run with --ignored at the Mac.
    #[test]
    #[ignore]
    fn without_permission_start_fails_cleanly() {
        if allowed() {
            return; // covered by the live test instead
        }
        let r = ScreenSource::start(None, 30);
        assert!(r.is_err());
    }
}
