use crate::{id::WindowId, new_window, view::View, window::WindowConfig};

type AppEventCallback = dyn Fn(&AppEvent);

pub fn launch<V: View + 'static>(app_view: impl Fn() -> V + 'static) {
    Application::new()
        .window(WindowId::next(), app_view, None)
        .run()
}

pub enum AppEvent {
    WillTerminate,
    Reopen { has_visible_windows: bool },
}

/// Floem top level application
/// This is the entry point of the application.
pub struct Application {
    application: glazier::Application,
    event_listener: Option<Box<AppEventCallback>>,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl glazier::AppHandler for Application {
    fn command(&mut self, _id: u32) {}

    fn will_terminate(&mut self) {
        if let Some(action) = self.event_listener.as_ref() {
            action(&AppEvent::WillTerminate);
        }
    }

    fn should_handle_reopen(&mut self, has_visible_windows: bool) {
        if let Some(action) = self.event_listener.as_ref() {
            action(&AppEvent::Reopen {
                has_visible_windows,
            });
        }
    }
}

impl Application {
    pub fn new() -> Self {
        Self {
            application: glazier::Application::new().unwrap(),
            event_listener: None,
        }
    }

    pub fn on_event(mut self, action: impl Fn(&AppEvent) + 'static) -> Self {
        self.event_listener = Some(Box::new(action));
        self
    }

    /// create a new window for the application, if you want multiple windows,
    /// just chain more window method to the builder
    pub fn window<V: View + 'static>(
        self,
        window_id: WindowId,
        app_view: impl FnOnce() -> V + 'static,
        config: Option<WindowConfig>,
    ) -> Self {
        new_window(window_id, app_view, config);
        self
    }

    pub fn run(self) {
        let application = self.application.clone();
        application.run(Some(Box::new(self)));
    }
}
