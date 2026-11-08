//! Asynchronous embedder-owned media consent. Dropping a request denies capture.
use cef::*;
pub type MediaPermissionHandler = dyn Fn(MediaPermissionRequest) + Send + Sync;
pub struct MediaPermissionRequest {
  pub browser: Browser,
  pub frame: Frame,
  pub origin: String,
  pub permissions: u32,
  pub(crate) callback: Option<MediaAccessCallback>,
}
impl MediaPermissionRequest {
  /// Complete on the CEF UI thread. Never grant bits absent from the request.
  pub fn respond(mut self, allowed: u32) {
    if let Some(callback) = self.callback.take() {
      callback.cont(allowed & self.permissions);
    }
  }
}
impl Drop for MediaPermissionRequest {
  fn drop(&mut self) {
    if let Some(callback) = self.callback.take() {
      callback.cont(0);
    }
  }
}
