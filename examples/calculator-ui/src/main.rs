use floem::{
    kurbo::Size,
    prelude::{palette::css, *},
    reactive::WriteSignal,
    taffy::{self, prelude::FromFr},
    unit::{Px, PxPct},
    views::{Decorators, container, svg},
    window::WindowConfig,
};
mod base_styles;

fn theme_switch_stack(set_theme: WriteSignal<bool>) -> Stack {
    // sun icon
    let svg_content: &str = r##"
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="#ffffff"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M12 12m-4 0a4 4 0 1 0 8 0a4 4 0 1 0 -8 0" />
      <path d="M3 12h1m8 -9v1m8 8h1m-9 8v1m-6.4 -15.4l.7 .7m12.1 -.7l-.7 .7m0 11.4l.7 .7m-12.1 -.7l-.7 .7" />
    </svg>"##;

    let moon_content: &str = r##"
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="#ffffff"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M12 3c.132 0 .263 0 .393 0a7.5 7.5 0 0 0 7.92 12.446a9 9 0 1 1 -8.313 -12.454z" />
    </svg>"##;

    h_stack((
        button(svg(svg_content).style(|s| s.size(22, 22).color(css::WHITE)))
            .style(|s| {
                s.border_top_left_radius(PxPct::Px(10f64))
                    .border_bottom_left_radius(PxPct::Px(10f64))
                    .apply(base_styles::default_theme_buttons_style())
            })
            .action(move || set_theme.update(|is_theme_dark| *is_theme_dark = false)),
        button(svg(moon_content).style(|s| s.size(22, 22).color(css::WHITE)))
            .style(|s| {
                s.border_top_right_radius(PxPct::Px(10f64))
                    .border_bottom_right_radius(PxPct::Px(10f64))
                    .apply(base_styles::default_theme_buttons_style())
            })
            .action(move || set_theme.update(|is_theme_dark| *is_theme_dark = true)),
    ))
    .style(|s| {
        s.flex()
            .items_center()
            .justify_center()
            .width_full()
            .height(36f64)
    })
}

fn calculator_buttons_stack() -> Stack {
    v_stack((
        base_styles::render_input_button("7"),
        base_styles::render_input_button("8"),
        base_styles::render_input_button("9"),
        base_styles::render_input_button("/"),
        base_styles::render_input_button("4"),
        base_styles::render_input_button("5"),
        base_styles::render_input_button("6"),
        base_styles::render_input_button("*"),
        base_styles::render_input_button("1"),
        base_styles::render_input_button("2"),
        base_styles::render_input_button("3"),
        base_styles::render_input_button("-"),
        base_styles::render_input_button("0"),
        base_styles::render_input_button("CLS"),
        base_styles::render_input_button("="),
        base_styles::render_input_button("+"),
    ))
    .style(|s| {
        s.display(taffy::style::Display::Grid)
            .grid_template_columns(vec![
                taffy::style::TrackSizingFunction::from_fr(25.0),
                taffy::style::TrackSizingFunction::from_fr(25.0),
                taffy::style::TrackSizingFunction::from_fr(25.0),
                taffy::style::TrackSizingFunction::from_fr(25.0),
            ])
            .gap(5)
            .flex_grow(100f32)
            .border_top_left_radius(30f32)
            .border_top_right_radius(30f32)
            .border_bottom_left_radius(30f32)
            .border_bottom_right_radius(30f32)
            .padding(16f32)
    })
    .class(base_styles::InputButtonIsland)
}

fn calculator_view() -> impl IntoView {
    let (is_dark_theme, set_theme) = create_signal(true);

    v_stack((
        theme_switch_stack(set_theme),
        container((label(move || String::from("0"))
            .style(|s| s.font_size(Px(32f64)).font_bold())
            .class(base_styles::OutputTxt),))
        .style(|s| {
            s.width_full()
                .flex()
                .justify_end()
                .items_end()
                .height(200f64)
        }),
        calculator_buttons_stack(),
    ))
    .style(move |_| {
        {
            if is_dark_theme.get() {
                base_styles::dark_theme()
            } else {
                base_styles::light_theme()
            }
        }
        .size_full()
        .flex()
        .flex_col()
        .gap(10)
        .padding(5)
    })
}

fn main() {
    floem::Application::new()
        .window(
            |_| calculator_view(),
            Some(
                WindowConfig::default()
                    .apply_default_theme(false)
                    .resizable(false)
                    .size(Size {
                        width: 400f64,
                        height: 800f64,
                    }),
            ),
        )
        .run()
}
