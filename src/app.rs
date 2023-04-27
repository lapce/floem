use glazier::{kurbo::Size, Application, WindowBuilder};
use leptos_reactive::{create_runtime, create_scope};

use crate::{
    app_handle::{AppContext, AppHandle},
    view::View,
    window::WindowConfig,
};

pub fn launch<V: View + 'static>(app_view: impl Fn(AppContext) -> V + 'static) {
    Builder::new().window(app_view, None).run()
}

/// Floem Application Builder
pub struct Builder {
    application: Application,
    reactive_runtime: leptos_reactive::RuntimeId,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            application: Application::new().unwrap(),
            reactive_runtime: create_runtime(),
        }
    }

    /// create a new window for the application, if you want multiple windows,
    /// just chain more window method to the builder
    pub fn window<V: View + 'static>(
        self,
        app_view: impl Fn(AppContext) -> V + 'static,
        config: Option<WindowConfig>,
    ) -> Self {
        let application = self.application.clone();
        let _ = create_scope(self.reactive_runtime, move |cx| {
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

            builder = builder.handler(Box::new(app));
            let window = builder.build().unwrap();
            window.bring_to_front_and_focus();
        });
        self
    }

    pub fn run(self) {
        self.application.run(None);
    }
}
