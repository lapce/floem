use glazier::{kurbo::Size, WindowBuilder};
use leptos_reactive::{create_runtime, raw_scope_and_disposer, Scope};

use crate::{app_handle::AppHandle, view::View, window::WindowConfig};

type AppEventCallback = dyn Fn(&AppEvent);

pub fn launch<V: View + 'static>(app_view: impl Fn() -> V + 'static) {
    Application::new().window(app_view, None).run()
}

pub enum AppEvent {
    WillTerminate,
}

/// Floem top level application
pub struct Application {
    application: glazier::Application,
    scope: Scope,
    event_listner: Option<Box<AppEventCallback>>,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl glazier::AppHandler for Application {
    fn command(&mut self, _id: u32) {}

    fn will_terminate(&mut self) {
        if let Some(action) = self.event_listner.as_ref() {
            action(&AppEvent::WillTerminate);
        }
    }
}

impl Application {
    pub fn new() -> Self {
        let runtime = create_runtime();
        let (scope, _) = raw_scope_and_disposer(runtime);
        Self {
            scope,
            application: glazier::Application::new().unwrap(),
            event_listner: None,
        }
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn on_event(mut self, action: impl Fn(&AppEvent) + 'static) -> Self {
        self.event_listner = Some(Box::new(action));
        self
    }

    /// create a new window for the application, if you want multiple windows,
    /// just chain more window method to the builder
    pub fn window<V: View + 'static>(
        self,
        app_view: impl FnOnce() -> V + 'static,
        config: Option<WindowConfig>,
    ) -> Self {
        let application = self.application.clone();
        let _ = self.scope.child_scope(move |cx| {
            let app = AppHandle::new(cx, app_view);
            let mut builder = WindowBuilder::new(application).size(
                config
                    .as_ref()
                    .and_then(|c| c.size)
                    .unwrap_or_else(|| Size::new(800.0, 600.0)),
            );
            if let Some(position) = config.as_ref().and_then(|c| c.position) {
                builder = builder.position(position);
            }
            if let Some(show_titlebar) = config.as_ref().and_then(|c| c.show_titlebar) {
                builder = builder.show_titlebar(show_titlebar);
            }

            builder = builder.handler(Box::new(app));
            let window = builder.build().unwrap();
            window.show();
        });
        self
    }

    pub fn run(self) {
        let application = self.application.clone();
        application.run(Some(Box::new(self)));
    }
}
