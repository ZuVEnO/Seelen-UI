pub mod cli;
pub mod hook;

use crate::{
    error_handler::Result,
    log_error,
    modules::virtual_desk::get_vd_manager,
    seelen::get_app_handle,
    state::application::FULL_STATE,
    utils::{are_overlaped, constants::OVERLAP_BLACK_LIST_BY_EXE},
    windows_api::{window::Window, AppBarData, AppBarDataEdge, WindowsApi},
};
use itertools::Itertools;
use seelen_core::state::HideMode;
use serde::Serialize;
use tauri::{Emitter, Listener, Manager, WebviewWindow};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::HMONITOR,
    UI::WindowsAndMessaging::{SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE},
};

pub struct FancyToolbar {
    window: WebviewWindow,
    pub cached_monitor: HMONITOR,
    /// Is the rect that the toolbar should have when it isn't hidden
    pub theoretical_rect: RECT,
    last_focus: Option<isize>,
    overlaped: bool,
}

impl Drop for FancyToolbar {
    fn drop(&mut self) {
        log::info!("Dropping {}", self.window.label());
        if let Ok(hwnd) = self.window.hwnd() {
            AppBarData::from_handle(hwnd).unregister_bar();
        }
        log_error!(self.window.destroy());
    }
}

impl FancyToolbar {
    pub fn new(postfix: &str) -> Result<Self> {
        log::info!("Creating {}/{}", Self::TARGET, postfix);
        Ok(Self {
            window: Self::create_window(postfix)?,
            last_focus: None,
            cached_monitor: HMONITOR(-1),
            theoretical_rect: RECT::default(),
            overlaped: false,
        })
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()> {
        self.window.emit_to(self.window.label(), event, payload)?;
        Ok(())
    }

    pub fn is_overlapping(&self, hwnd: HWND) -> Result<bool> {
        let window_rect = WindowsApi::get_inner_window_rect(hwnd)?;
        Ok(are_overlaped(&self.theoretical_rect, &window_rect))
    }

    pub fn set_overlaped_status(&mut self, is_overlaped: bool) -> Result<()> {
        if self.overlaped == is_overlaped {
            return Ok(());
        }
        self.overlaped = is_overlaped;
        self.emit("set-auto-hide", self.overlaped)?;
        Ok(())
    }

    pub fn handle_overlaped_status(&mut self, hwnd: HWND) -> Result<()> {
        let window = Window::from(hwnd);
        let is_overlaped = self.is_overlapping(hwnd)?
            && !window.is_desktop()
            && !window.is_seelen_overlay()
            && !OVERLAP_BLACK_LIST_BY_EXE
                .contains(&WindowsApi::exe(hwnd).unwrap_or_default().as_str());
        self.set_overlaped_status(is_overlaped)
    }

    pub fn hide(&mut self) -> Result<()> {
        WindowsApi::show_window_async(self.window.hwnd()?, SW_HIDE)?;
        Ok(())
    }

    pub fn show(&mut self) -> Result<()> {
        WindowsApi::show_window_async(self.window.hwnd()?, SW_SHOWNOACTIVATE)?;
        Ok(())
    }

    pub fn focus_changed(&mut self, hwnd: HWND) -> Result<()> {
        self.last_focus = Some(hwnd.0);
        Ok(())
    }
}

// statics
impl FancyToolbar {
    pub const TITLE: &'static str = "Seelen Fancy Toolbar";
    const TARGET: &'static str = "fancy-toolbar";

    /// Work area no works fine on multiple monitors
    /// so we use this functions that only takes the toolbar in account
    pub fn get_work_area_by_monitor(monitor: isize) -> Result<RECT> {
        let monitor_info = WindowsApi::monitor_info(HMONITOR(monitor))?;

        let dpi = WindowsApi::get_device_pixel_ratio(HMONITOR(monitor))?;
        let mut rect = monitor_info.monitorInfo.rcMonitor;

        let state = FULL_STATE.load();
        if state.is_bar_enabled() {
            let toolbar_height = state.settings().fancy_toolbar.height;
            rect.top += (toolbar_height as f32 * dpi) as i32;
        }

        Ok(rect)
    }

    pub fn set_positions(&mut self, monitor: isize) -> Result<()> {
        let hwnd = HWND(self.window.hwnd()?.0);
        let hmonitor = HMONITOR(monitor);

        if hmonitor.is_invalid() {
            return Err("Invalid Monitor".into());
        }
        self.cached_monitor = hmonitor;

        let state = FULL_STATE.load();
        let settings = &state.settings().fancy_toolbar;

        let monitor_info = WindowsApi::monitor_info(hmonitor)?;
        let monitor_dpi = WindowsApi::get_device_pixel_ratio(hmonitor)?;
        let rc_monitor = monitor_info.monitorInfo.rcMonitor;
        self.theoretical_rect = RECT {
            bottom: rc_monitor.top + (settings.height as f32 * monitor_dpi) as i32,
            ..rc_monitor
        };

        let mut abd = AppBarData::from_handle(hwnd);
        match settings.hide_mode {
            HideMode::Never => {
                abd.set_edge(AppBarDataEdge::Top);
                abd.set_rect(self.theoretical_rect);
                abd.register_as_new_bar();
            }
            _ => abd.unregister_bar(),
        };

        // pre set position for resize in case of multiples dpi
        WindowsApi::move_window(hwnd, &rc_monitor)?;
        WindowsApi::set_position(hwnd, None, &rc_monitor, SWP_NOACTIVATE)?;
        Ok(())
    }

    fn create_window(postfix: &str) -> Result<WebviewWindow> {
        let manager = get_app_handle();

        let label = format!("{}/{}", Self::TARGET, postfix);
        let window = match manager.get_webview_window(&label) {
            Some(window) => window,
            None => tauri::WebviewWindowBuilder::new(
                &manager,
                label,
                tauri::WebviewUrl::App("toolbar/index.html".into()),
            )
            .title(Self::TITLE)
            .minimizable(false)
            .maximizable(false)
            .closable(false)
            .resizable(false)
            .visible(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .drag_and_drop(false)
            .build()?,
        };

        window.set_ignore_cursor_events(true)?;
        window.once("store-events-ready", Self::on_store_events_ready);
        Ok(window)
    }

    fn on_store_events_ready(_: tauri::Event) {
        // TODO refactor this implementation
        std::thread::spawn(|| -> Result<()> {
            let handler = get_app_handle();
            let vd = get_vd_manager();
            let desktops = vd
                .get_all()?
                .iter()
                .map(|d| d.as_serializable())
                .collect_vec();
            handler.emit("workspaces-changed", &desktops)?;
            handler.emit("active-workspace-changed", vd.get_current()?.id())?;
            Ok(())
        });
    }
}
