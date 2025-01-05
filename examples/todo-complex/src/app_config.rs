use floem::action::set_window_scale;
use floem::keyboard::Key;
use floem::reactive::provide_context;
use floem::views::Decorators;
use floem::{
    event::{Event, EventListener},
    kurbo::{Point, Size},
    reactive::{create_updater, RwSignal, SignalGet, SignalUpdate, SignalWith},
    window::WindowConfig,
    Application, IntoView,
};
use serde::{Deserialize, Serialize};

use crate::OS_MOD;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppTheme {
    FollowSystem,
    DarkMode,
    LightMode,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppThemeState {
    pub system: floem::window::Theme,
    pub theme: AppTheme,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AppConfig {
    pub position: Point,
    pub size: Size,
    pub app_theme: AppThemeState,
    pub window_scale: f64,
}

impl std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            position: Point { x: 500.0, y: 500.0 },
            size: Size {
                width: 350.0,
                height: 650.0,
            },
            app_theme: AppThemeState {
                system: floem::window::Theme::Dark,
                theme: AppTheme::FollowSystem,
            },
            window_scale: 1.,
        }
    }
}

pub fn launch_with_track<V: IntoView + 'static>(app_view: impl FnOnce() -> V + 'static) {
    let config: AppConfig = confy::load("my_app", "floem-defaults").unwrap_or_default();

    let app = Application::new();

    // modifying this will rewrite app config to disk
    let app_config = RwSignal::new(config);

    provide_context(app_config);

    // todo: debounce this
    create_updater(
        move || app_config.get(),
        |config| {
            let _ = confy::store("my_app", "floem-defaults", config);
        },
    );

    let window_config = WindowConfig::default()
        .size(app_config.with(|ac| ac.size))
        .position(app_config.with(|ac| ac.position));

    app.window(
        move |_| {
            set_window_scale(app_config.with(|c| c.window_scale));
            app_view()
                .on_key_down(
                    Key::Character("=".into()),
                    |m| m == OS_MOD,
                    move |_| {
                        app_config.update(|ac| {
                            ac.window_scale *= 1.1;
                            floem::action::set_window_scale(ac.window_scale);
                        });
                    },
                )
                .on_key_down(
                    Key::Character("-".into()),
                    |m| m == OS_MOD,
                    move |_| {
                        app_config.update(|ac| {
                            ac.window_scale /= 1.1;
                            floem::action::set_window_scale(ac.window_scale);
                        });
                    },
                )
                .on_key_down(
                    Key::Character("0".into()),
                    |m| m == OS_MOD,
                    move |_| {
                        app_config.update(|ac| {
                            ac.window_scale = 1.;
                            floem::action::set_window_scale(ac.window_scale);
                        });
                    },
                )
                .on_event_stop(EventListener::WindowMoved, move |event| {
                    if let Event::WindowMoved(position) = event {
                        app_config.update(|val| {
                            val.position = *position;
                        })
                    }
                })
                .on_event_stop(EventListener::WindowResized, move |event| {
                    if let Event::WindowResized(size) = event {
                        app_config.update(|val| {
                            val.size = *size;
                        })
                    }
                })
        },
        Some(window_config),
    )
    .run();
}
