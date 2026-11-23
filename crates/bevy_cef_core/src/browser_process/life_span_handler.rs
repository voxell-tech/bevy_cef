//! CEF LifeSpanHandler — tracks which browsers are alive.

use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, BrowserSettings, CefString, Client, DictionaryValue, Frame, ImplBrowser,
    ImplLifeSpanHandler, PopupFeatures, WindowInfo, WindowOpenDisposition, WrapLifeSpanHandler,
    sys,
};
use std::cell::RefCell;
use std::os::raw::c_int;

/// The browsers CEF has created and not yet closed, popups and DevTools included.
pub type SharedLiveBrowsers = std::rc::Rc<RefCell<Vec<Browser>>>;

/// A page asking to open `url` in a new window or tab: a link with `target="_blank"`, one
/// opened with a modifier, or `window.open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupRequest {
    /// The webview whose page asked.
    pub webview: Entity,
    pub url: String,
    /// Whether the new page should be shown at once. A link opened in the background, as
    /// with Ctrl or the middle button, should not.
    pub foreground: bool,
}

/// The popups pages asked for and the app has not yet taken.
pub type SharedPopups = std::rc::Rc<RefCell<Vec<PopupRequest>>>;

/// Whether a page asking for a new page with `disposition` wants it shown at once.
pub(crate) fn in_front(disposition: WindowOpenDisposition) -> bool {
    disposition != WindowOpenDisposition::NEW_BACKGROUND_TAB
}

/// Adds each created browser to a [`SharedLiveBrowsers`] list and removes it when CEF closes it.
///
/// ## Reference
///
/// - [`CefLifeSpanHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/145/classCefLifeSpanHandler.html)
pub struct LifeSpanHandlerBuilder {
    object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>,
    live: SharedLiveBrowsers,
    /// The webview the browser draws, and where its page's popups are put instead of opening
    /// them. `None`, as for DevTools, lets CEF open them as it would.
    popups: Option<(Entity, SharedPopups)>,
}

impl LifeSpanHandlerBuilder {
    /// Builds a handler that records into `live`.
    ///
    /// A popup shares its opener's client, so it is recorded in the same list.
    pub fn build(live: SharedLiveBrowsers) -> cef::LifeSpanHandler {
        cef::LifeSpanHandler::new(Self {
            object: core::ptr::null_mut(),
            live,
            popups: None,
        })
    }

    /// Builds a handler that records into `live`, and puts each popup the page of `webview`
    /// asks for in `popups` rather than opening a window of CEF's own for it.
    pub fn build_for(
        live: SharedLiveBrowsers,
        webview: Entity,
        popups: SharedPopups,
    ) -> cef::LifeSpanHandler {
        cef::LifeSpanHandler::new(Self {
            object: core::ptr::null_mut(),
            live,
            popups: Some((webview, popups)),
        })
    }
}

impl Rc for LifeSpanHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for LifeSpanHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            live: self.live.clone(),
            popups: self.popups.clone(),
        }
    }
}

impl WrapLifeSpanHandler for LifeSpanHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplLifeSpanHandler for LifeSpanHandlerBuilder {
    #[allow(clippy::too_many_arguments)]
    fn on_before_popup(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        _popup_id: c_int,
        target_url: Option<&CefString>,
        _target_frame_name: Option<&CefString>,
        target_disposition: WindowOpenDisposition,
        _user_gesture: c_int,
        _popup_features: Option<&PopupFeatures>,
        _window_info: Option<&mut WindowInfo>,
        _client: Option<&mut Option<Client>>,
        _settings: Option<&mut BrowserSettings>,
        _extra_info: Option<&mut Option<DictionaryValue>>,
        _no_javascript_access: Option<&mut c_int>,
    ) -> c_int {
        let Some((webview, popups)) = &self.popups else {
            return 0;
        };
        // A windowless browser's popup would be a window of its own, outside the app's
        // scene, so it is cancelled and the app is asked to open it instead.
        let url = target_url.map(ToString::to_string).unwrap_or_default();
        if !url.is_empty() {
            popups.borrow_mut().push(PopupRequest {
                webview: *webview,
                url,
                foreground: in_front(target_disposition),
            });
        }
        1
    }

    fn on_after_created(&self, browser: Option<&mut Browser>) {
        if let Some(browser) = browser {
            self.live.borrow_mut().push(browser.clone());
        }
    }

    fn on_before_close(&self, browser: Option<&mut Browser>) {
        if let Some(browser) = browser {
            let identifier = browser.identifier();
            self.live
                .borrow_mut()
                .retain(|live| live.identifier() != identifier);
        }
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_life_span_handler_t {
        self.object.cast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_link_opened_in_the_background_stays_behind() {
        assert!(!in_front(WindowOpenDisposition::NEW_BACKGROUND_TAB));
        for shown in [
            WindowOpenDisposition::NEW_FOREGROUND_TAB,
            WindowOpenDisposition::NEW_POPUP,
            WindowOpenDisposition::NEW_WINDOW,
        ] {
            assert!(in_front(shown), "{shown:?}");
        }
    }
}
