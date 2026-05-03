use crate::{
    Application,
    app::UserEvent,
    frame::{DisplayTiming, FrameTime, PresentPacing, PresentationInterval},
    platform::{Duration, Instant},
};
use raw_window_handle::HasWindowHandle;
use subduction::{FrameSourceTarget, frame_source::FrameSource as SubductionFrameSource};
use subduction_core::output::OutputId;
use winit::window::{Window as WinitWindow, WindowId};

#[cfg(target_os = "macos")]
use {
    objc2::rc::Retained,
    objc2_app_kit::NSView,
    raw_window_handle::RawWindowHandle,
    subduction::apple::{AppleDisplayLinkThread, AppleDisplayLinkView},
};

pub(crate) fn frame_pacing_diag_enabled() -> bool {
    std::env::var_os("FLOEM_FRAME_PACING_DIAG").is_some()
}

pub(crate) fn window_frame_interval(window: &dyn WinitWindow) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.current_video_mode())
        .and_then(|mode| mode.refresh_rate_millihertz())
        .map(|mhz| Duration::from_nanos(1_000_000_000_000 / mhz.get() as u64))
        .unwrap_or(Duration::from_millis(16))
}

#[derive(Debug)]
pub(crate) struct FrameSource {
    window_id: WindowId,
    output_id: OutputId,
    inner: SubductionFrameSource,
    #[cfg(target_os = "macos")]
    display_link: Option<MacDisplayLink>,
    #[cfg(target_os = "macos")]
    target_view: Option<AppleDisplayLinkView>,
    #[cfg(target_os = "macos")]
    target_refresh_millihertz: Option<u32>,
    #[cfg(target_os = "macos")]
    active: bool,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct MacDisplayLink {
    _link: AppleDisplayLinkThread,
    view: AppleDisplayLinkView,
    refresh_millihertz: Option<u32>,
}

pub(crate) fn new_window_frame_source(window_id: WindowId, output_id: u32) -> FrameSource {
    let inner = SubductionFrameSource::new(OutputId(output_id), move |tick| {
        if frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing display link callback window={:?} tick={} predicted={:?} refresh={:?}",
                window_id, tick.frame_index, tick.predicted_present, tick.refresh_interval,
            );
        }
        Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
    });
    FrameSource {
        window_id,
        output_id: OutputId(output_id),
        inner,
        #[cfg(target_os = "macos")]
        display_link: None,
        #[cfg(target_os = "macos")]
        target_view: None,
        #[cfg(target_os = "macos")]
        target_refresh_millihertz: None,
        #[cfg(target_os = "macos")]
        active: false,
    }
}

impl FrameSource {
    pub(crate) fn frame_interval(&mut self, window: &dyn WinitWindow) -> Duration {
        self.inner.frame_interval(window_frame_interval(window))
    }

    pub(crate) fn refresh_window_target(&mut self, window: &dyn WinitWindow) {
        let monitor = window.current_monitor();
        let raw_window_handle = window.window_handle().ok().map(|handle| handle.as_raw());
        let target = FrameSourceTarget {
            monitor_name: monitor
                .as_ref()
                .and_then(|monitor| monitor.name().map(|name| name.to_string())),
            refresh_millihertz: monitor
                .as_ref()
                .and_then(|monitor| monitor.current_video_mode())
                .and_then(|mode| mode.refresh_rate_millihertz())
                .map(|rate| rate.get()),
            raw_window_handle,
        };
        if frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame source target window={:?} monitor={:?} refresh_millihz={:?}",
                self.window_id, target.monitor_name, target.refresh_millihertz,
            );
        }
        #[cfg(target_os = "macos")]
        self.refresh_macos_display_link(
            raw_window_handle,
            self.output_id,
            target.refresh_millihertz,
        );
        self.inner.refresh_target(target);
    }

    pub(crate) fn current_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        let frame_interval = self.frame_interval(window);
        if let Some(tick) = self.inner.latest_tick()
            && let Some(predicted_present) = tick.predicted_present
        {
            let predicted_present = self.inner.host_to_instant(predicted_present);
            return FrameTime {
                now: predicted_present,
                interval: PresentationInterval {
                    deadline_min: now,
                    deadline_max: predicted_present,
                    predicted_present: Some(predicted_present),
                    display_timing: DisplayTiming::fixed(frame_interval),
                    present_pacing: PresentPacing::AtTime(predicted_present),
                    background_rendering,
                },
                frame_interval,
                frame_index: tick.frame_index,
            };
        }

        let predicted_present = now.checked_add(frame_interval).unwrap_or(now);
        FrameTime {
            now: predicted_present,
            interval: PresentationInterval {
                deadline_min: now,
                deadline_max: predicted_present,
                predicted_present: Some(predicted_present),
                display_timing: DisplayTiming::fixed(frame_interval),
                present_pacing: PresentPacing::AtTime(predicted_present),
                background_rendering,
            },
            frame_interval,
            frame_index: self.inner.frame_counter(),
        }
    }

    pub(crate) fn receive_frame_tick(&mut self, tick: subduction_core::timing::FrameTick) {
        self.inner.receive_frame_tick(tick);
    }

    pub(crate) fn latest_tick(&self) -> Option<subduction_core::timing::FrameTick> {
        self.inner.latest_tick()
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        #[cfg(target_os = "macos")]
        {
            if self.active == active {
                return;
            }
            self.active = active;
            if active {
                self.ensure_macos_display_link();
            } else {
                self.display_link = None;
            }
            return;
        }

        #[cfg(not(target_os = "macos"))]
        self.inner.set_active(active);
    }
}

#[cfg(target_os = "macos")]
impl FrameSource {
    fn refresh_macos_display_link(
        &mut self,
        raw_window_handle: Option<RawWindowHandle>,
        output: OutputId,
        refresh_millihertz: Option<u32>,
    ) {
        let Some(view) = macos_display_link_view(raw_window_handle) else {
            self.target_view = None;
            self.target_refresh_millihertz = None;
            self.display_link = None;
            return;
        };
        self.target_view = Some(view.clone());
        self.target_refresh_millihertz = refresh_millihertz;

        if let Some(display_link) = self.display_link.as_ref()
            && display_link.view.is_same_view(&view)
            && display_link.refresh_millihertz == refresh_millihertz
        {
            return;
        }

        self.display_link = None;
        self.ensure_macos_display_link_with(view, output, refresh_millihertz);
    }

    fn ensure_macos_display_link(&mut self) {
        let Some(view) = self.target_view.clone() else {
            return;
        };
        self.ensure_macos_display_link_with(view, self.output_id, self.target_refresh_millihertz);
    }

    fn ensure_macos_display_link_with(
        &mut self,
        view: AppleDisplayLinkView,
        output: OutputId,
        refresh_millihertz: Option<u32>,
    ) {
        if !self.active {
            return;
        }
        if let Some(display_link) = self.display_link.as_ref()
            && display_link.view.is_same_view(&view)
            && display_link.refresh_millihertz == refresh_millihertz
        {
            return;
        }

        let window_id = self.window_id;
        let link = AppleDisplayLinkThread::spawn_for_view_with_preferred_frame_rate_millihertz(
            move |tick| {
                if frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem frame pacing display link callback window={:?} tick={} predicted={:?} refresh={:?}",
                        window_id, tick.frame_index, tick.predicted_present, tick.refresh_interval,
                    );
                }
                Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
            },
            output,
            view.clone(),
            refresh_millihertz,
        );
        self.display_link = Some(MacDisplayLink {
            _link: link,
            view,
            refresh_millihertz,
        });
    }
}

#[cfg(target_os = "macos")]
fn macos_display_link_view(raw_window: Option<RawWindowHandle>) -> Option<AppleDisplayLinkView> {
    match raw_window? {
        RawWindowHandle::AppKit(handle) => {
            // SAFETY: raw-window-handle guarantees this is a valid NSView
            // pointer for the lifetime of the window handle.
            unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }
                .map(AppleDisplayLinkView::new)
        }
        _ => None,
    }
}
