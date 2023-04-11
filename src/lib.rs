pub mod app;
pub mod context;
pub mod event;
pub mod ext_event;
pub mod id;
pub mod renderer;
pub mod stack;
pub mod style;
pub mod text;
pub mod view;
pub mod view_tuple;
pub mod views;

pub use floem_renderer::cosmic_text;
pub use floem_renderer::Renderer;
pub use glazier;
use glazier::kurbo::Size;
pub use leptos_reactive as reactive;
pub use taffy;
pub use vello::peniko;

use app::{App, AppContext};
use glazier::{Application, WindowBuilder};
use leptos_reactive::{create_runtime, create_scope};
use view::View;

pub fn launch<V: View + 'static>(app_logic: impl Fn(AppContext) -> V + 'static) {
    create_scope(create_runtime(), |cx| {
        let app = App::new(cx, app_logic);
        let application = Application::new().unwrap();
        let mut builder = WindowBuilder::new(application.clone())
            .size(Size::new(800.0, 600.0))
            .handler(Box::new(app));
        let window = builder.build().unwrap();
        window.show();
        application.run(None);
    });
}
