pub mod app;
pub mod button;
pub mod context;
pub mod event;
pub mod id;
pub mod stack;
pub mod text;
pub mod view;
pub mod view_tuple;
mod views;

pub use leptos_reactive as reactive;
pub use taffy::style;
pub use views::*;

use app::{App, AppContext};
use glazier::{Application, WindowBuilder};
use leptos_reactive::{create_runtime, create_scope};
use view::View;

pub fn launch<V: View + 'static>(app_logic: impl Fn(AppContext) -> V + 'static) {
    create_scope(create_runtime(), |cx| {
        let app = App::new(cx, app_logic);
        let application = Application::new().unwrap();
        let mut builder = WindowBuilder::new(application.clone());
        builder.set_handler(Box::new(app));
        let window = builder.build().unwrap();
        window.show();
        application.run(None);
    });
}
