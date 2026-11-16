//! CEF PermissionHandler — answers each page's permission prompts, which a windowless browser
//! never shows and would otherwise leave pending: a page's `navigator.clipboard.readText()`
//! waited forever. A webview the app allows reads the clipboard; every other prompt is denied.

use bevy::platform::collections::HashSet;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, ImplPermissionHandler, ImplPermissionPromptCallback,
    PermissionPromptCallback, PermissionRequestResult, WrapPermissionHandler, sys,
};
use cef_dll_sys::cef_permission_request_types_t;
use std::cell::RefCell;
use std::os::raw::c_int;

/// The webviews whose pages may read the clipboard.
pub type SharedClipboard = std::rc::Rc<RefCell<HashSet<Entity>>>;

/// ## Reference
///
/// - [`CefPermissionHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/145/classCefPermissionHandler.html)
pub struct PermissionHandlerBuilder {
    object: *mut RcImpl<sys::_cef_permission_handler_t, Self>,
    webview: Entity,
    clipboard: SharedClipboard,
}

impl PermissionHandlerBuilder {
    pub fn build(webview: Entity, clipboard: SharedClipboard) -> cef::PermissionHandler {
        cef::PermissionHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            clipboard,
        })
    }
}

impl WrapPermissionHandler for PermissionHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_permission_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for PermissionHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for PermissionHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            clipboard: self.clipboard.clone(),
        }
    }
}

impl ImplPermissionHandler for PermissionHandlerBuilder {
    fn on_show_permission_prompt(
        &self,
        _browser: Option<&mut Browser>,
        _prompt_id: u64,
        _requesting_origin: Option<&CefString>,
        requested_permissions: u32,
        callback: Option<&mut PermissionPromptCallback>,
    ) -> c_int {
        let Some(callback) = callback else {
            return 0;
        };
        callback.cont(answer(
            requested_permissions,
            self.clipboard.borrow().contains(&self.webview),
        ));
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_permission_handler_t {
        self.object.cast()
    }
}

/// Accepts a prompt that asks for the clipboard alone, from a webview allowed it; denies the rest.
fn answer(requested: u32, clipboard_allowed: bool) -> PermissionRequestResult {
    let clipboard = cef_permission_request_types_t::CEF_PERMISSION_TYPE_CLIPBOARD as u32;
    if clipboard_allowed && requested != 0 && requested & !clipboard == 0 {
        PermissionRequestResult::ACCEPT
    } else {
        PermissionRequestResult::DENY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIPBOARD: u32 = cef_permission_request_types_t::CEF_PERMISSION_TYPE_CLIPBOARD as u32;
    const CAMERA: u32 = cef_permission_request_types_t::CEF_PERMISSION_TYPE_CAMERA_STREAM as u32;

    #[test]
    fn an_allowed_webview_reads_the_clipboard_and_nothing_more() {
        assert_eq!(answer(CLIPBOARD, true), PermissionRequestResult::ACCEPT);
        assert_eq!(
            answer(CLIPBOARD | CAMERA, true),
            PermissionRequestResult::DENY
        );
        assert_eq!(answer(CAMERA, true), PermissionRequestResult::DENY);
    }

    #[test]
    fn any_other_webview_is_denied() {
        assert_eq!(answer(CLIPBOARD, false), PermissionRequestResult::DENY);
    }
}
