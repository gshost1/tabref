/// A screenshot as `Page.captureScreenshot` sent it: the PNG bytes, and the
/// device scale it was taken at (CSS pixels to image pixels; 1 on most
/// screens, 2 on a high-density one).
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedImage {
    pub png: Vec<u8>,
    pub device_scale: f64,
}
