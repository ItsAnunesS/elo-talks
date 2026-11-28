// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use cef::*;
use std::sync::Arc;

#[cfg(target_os = "linux")]
type CefOsEvent<'a> = Option<&'a mut cef::sys::XEvent>;
#[cfg(target_os = "macos")]
type CefOsEvent<'a> = *mut u8;
#[cfg(windows)]
type CefOsEvent<'a> = Option<&'a mut cef::sys::MSG>;

wrap_keyboard_handler! {
  pub struct TauriCefKeyboardHandler {
    devtools_enabled: bool,
    native_paste_handler: Option<Arc<crate::NativePasteHandler>>,
  }

  impl KeyboardHandler {
    fn on_pre_key_event(
      &self,
      _browser: Option<&mut Browser>,
      event: Option<&KeyEvent>,
      _os_event: CefOsEvent<'_>,
      _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
    ) -> ::std::os::raw::c_int {
      if let (Some(browser), Some(event), Some(handler)) = (_browser, event, &self.native_paste_handler) {
        use cef::sys::{cef_event_flags_t as flags, cef_key_event_type_t};
        #[cfg(windows)]
        let modifiers = event.modifiers as i32;
        #[cfg(not(windows))]
        let modifiers = event.modifiers;
        let control = modifiers & flags::EVENTFLAG_CONTROL_DOWN.0 != 0;
        let other = modifiers & (flags::EVENTFLAG_ALT_DOWN.0 | flags::EVENTFLAG_SHIFT_DOWN.0 | flags::EVENTFLAG_COMMAND_DOWN.0) != 0;
        if event.type_ == cef_key_event_type_t::KEYEVENT_RAWKEYDOWN.into()
          && event.windows_key_code == 86 && control && !other && handler(browser.clone()) {
          if let Some(shortcut) = _is_keyboard_shortcut { *shortcut = 1; }
          return 1;
        }
      }
      // If devtools is disabled, block devtools keyboard shortcuts.
      if !self.devtools_enabled {
        let Some(event) = event else {
          return 0;
        };

        // Check if this is a keydown event.
        use cef::sys::cef_key_event_type_t;
        let keydown_type: cef::KeyEventType = cef_key_event_type_t::KEYEVENT_RAWKEYDOWN.into();
        if event.type_ != keydown_type {
          return 0;
        }

        // Get modifier keys.
        use cef::sys::cef_event_flags_t;
        #[cfg(windows)]
        let modifiers = event.modifiers as i32;
        #[cfg(not(windows))]
        let modifiers = event.modifiers;

        #[cfg(not(target_os = "macos"))]
        let ctrl = (modifiers & (cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0)) != 0;
        #[cfg(not(target_os = "macos"))]
        let shift = (modifiers & (cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0)) != 0;

        let key_code = event.windows_key_code;

        // Block F12 (key code 123).
        if key_code == 123 {
          if let Some(is_keyboard_shortcut) = _is_keyboard_shortcut {
            *is_keyboard_shortcut = 1;
          }
          return 1;
        }

        // Block Ctrl+Shift+I (key code 73 = 'I') on Linux/Windows.
        #[cfg(not(target_os = "macos"))]
        if key_code == 73 && ctrl && shift {
          if let Some(is_keyboard_shortcut) = _is_keyboard_shortcut {
            *is_keyboard_shortcut = 1;
          }
          return 1;
        }

        // Block Cmd+Opt+I on macOS.
        #[cfg(target_os = "macos")]
        {
          let meta = (modifiers & cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0) != 0;
          let alt = (modifiers & cef_event_flags_t::EVENTFLAG_ALT_DOWN.0) != 0;
          if key_code == 73 && meta && alt {
            if let Some(is_keyboard_shortcut) = _is_keyboard_shortcut {
              *is_keyboard_shortcut = 1;
            }
            return 1;
          }
        }
      }

      0
    }
  }
}
