//! CEF RequestHandler — puts a link opened with Ctrl or the middle button with the page's
//! popups. CEF asks here, not in `on_before_popup`, and opens it in the same page if nothing
//! takes it.

use crate::browser_process::{PopupRequest, SharedPopups, life_span_handler::in_front};
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, Frame, ImplRequestHandler, WindowOpenDisposition, WrapRequestHandler, sys,
};
use std::os::raw::c_int;

/// Whether a link opened with `disposition` asks for a new page rather than the same one.
fn wants_new_page(disposition: WindowOpenDisposition) -> bool {
    [
        WindowOpenDisposition::NEW_FOREGROUND_TAB,
        WindowOpenDisposition::NEW_BACKGROUND_TAB,
        WindowOpenDisposition::NEW_POPUP,
        WindowOpenDisposition::NEW_WINDOW,
    ]
    .contains(&disposition)
}

/// ## Reference
///
/// - [`CefRequestHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/145/classCefRequestHandler.html)
pub struct RequestHandlerBuilder {
    object: *mut RcImpl<sys::_cef_request_handler_t, Self>,
    webview: Entity,
    popups: SharedPopups,
}

impl RequestHandlerBuilder {
    /// Builds a handler that puts each new page the page of `webview` asks for in `popups`.
    pub fn build(webview: Entity, popups: SharedPopups) -> cef::RequestHandler {
        cef::RequestHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            popups,
        })
    }
}

impl WrapRequestHandler for RequestHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_request_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for RequestHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for RequestHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            popups: self.popups.clone(),
        }
    }
}

impl ImplRequestHandler for RequestHandlerBuilder {
    fn on_open_urlfrom_tab(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        target_url: Option<&CefString>,
        target_disposition: WindowOpenDisposition,
        _user_gesture: c_int,
    ) -> c_int {
        let url = target_url.map(ToString::to_string).unwrap_or_default();
        if url.is_empty() || !wants_new_page(target_disposition) {
            return 0;
        }
        self.popups.borrow_mut().push(PopupRequest {
            webview: self.webview,
            url,
            foreground: in_front(target_disposition),
        });
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_request_handler_t {
        self.object.cast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_tab_or_window_is_taken() {
        assert!(wants_new_page(WindowOpenDisposition::NEW_BACKGROUND_TAB));
        assert!(wants_new_page(WindowOpenDisposition::NEW_FOREGROUND_TAB));
        assert!(!wants_new_page(WindowOpenDisposition::CURRENT_TAB));
    }
}
