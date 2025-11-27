use std::ops::RangeInclusive;

use floem::{
    peniko::{
        color::{AlphaColor, DisplayP3, Oklab, Oklch, Srgb},
        Color,
    },
    prelude::{palette::css, *},
    reactive::{DerivedRwSignal, SignalGet, SignalUpdate},
    style_class,
    taffy::prelude::*,
    unit::PxPctAuto,
    views::slider::Slider,
    window::WindowConfig,
    AnyView, Application,
};

style_class!(SliderGrid);
style_class!(SliderControl);

fn slider_row<F>(
    label_text: &str,
    get_value: impl Fn() -> f32 + 'static,
    set_value: F,
    range: RangeInclusive<f64>,
) -> Vec<AnyView>
where
    F: Fn(f32) + 'static,
{
    (
        text(label_text),
        Slider::new_ranged(move || get_value() as f64, range)
            .on_change_value(move |new_value| {
                set_value(new_value as f32);
            })
            .style(|s| s.flex_grow(1.0).height(20).min_width(100)),
    )
        .into_views()
}

fn color_display<S>(rgb_signal: S) -> impl IntoView
where
    S: SignalGet<Color> + SignalUpdate<Color> + Copy + 'static,
{
    let lightness = move || rgb_signal.get().convert::<Oklab>().components[0];
    v_stack((
        empty().style(move |s| {
            let rgb = rgb_signal.get();
            s.background(rgb).width_full().height(75)
        }),
        label(move || {
            let [r, g, b, _a] = rgb_signal.get().components;
            format!(
                "sRGB: ({}, {}, {})",
                (r * 255.0) as i32,
                (g * 255.0) as i32,
                (b * 255.0) as i32,
            )
        })
        .style(move |s| {
            s.font_size(18)
                .absolute()
                .padding(10)
                .apply_if(lightness() > 0.5, |s| s.color(css::BLACK))
                .apply_if(lightness() < 0.5, |s| s.color(css::WHITE))
        }),
    ))
    .style(|s| s.gap(10).items_center().width_full())
}

fn labeled_slider_grid(name: &str, rows: impl IntoIterator<Item = Vec<AnyView>>) -> impl IntoView {
    let header = text(name).style(|s| {
        s.justify_self(AlignItems::Center)
            .grid_column(Line::from_span(2))
    });

    let mut views = vec![header.into_any()];
    for row in rows {
        views.extend(row);
    }
    views.class(SliderGrid)
}

fn lch_color_view() -> impl IntoView {
    let lch_signal = RwSignal::new((0.5, 0.25, 180.0));
    let rgb_signal = DerivedRwSignal::new(
        lch_signal,
        |(l, c, h)| {
            let lch = AlphaColor::<Oklch>::new([*l, *c, *h, 1.0]);
            lch.convert()
        },
        |rgb: &Color| {
            let lch: AlphaColor<Oklch> = rgb.convert();
            let [l, c, h, _a] = lch.components;
            (l, c, h)
        },
    );

    let sliders = labeled_slider_grid(
        "OKLCH",
        [
            slider_row(
                "Lightness",
                move || lch_signal.get().0,
                move |val| lch_signal.update(|(l, _, _)| *l = val),
                0f64..=1f64,
            ),
            slider_row(
                "Chroma",
                move || lch_signal.get().1,
                move |val| lch_signal.update(|(_, c, _)| *c = val),
                0f64..=0.5f64,
            ),
            slider_row(
                "Hue",
                move || lch_signal.get().2,
                move |val| lch_signal.update(|(_, _, h)| *h = val),
                0f64..=360f64,
            ),
        ],
    );

    (sliders, color_display(rgb_signal))
        .v_stack()
        .class(SliderControl)
}

fn oklab_color_view() -> impl IntoView {
    let lab_signal = RwSignal::new((0.5, 0.0, 0.0));
    let rgb_signal = DerivedRwSignal::new(
        lab_signal,
        |(l, a, b)| {
            let lab = AlphaColor::<Oklab>::new([*l, *a, *b, 1.0]);
            lab.convert()
        },
        |rgb: &Color| {
            let lab: AlphaColor<Oklab> = rgb.convert();
            let [l, a, b, _] = lab.components;
            (l, a, b)
        },
    );

    let sliders = labeled_slider_grid(
        "OKLAB",
        [
            slider_row(
                "Lightness",
                move || lab_signal.get().0,
                move |val| lab_signal.update(|(l, _, _)| *l = val),
                0f64..=1f64,
            ),
            slider_row(
                "a (green-red)",
                move || lab_signal.get().1,
                move |val| lab_signal.update(|(_, a, _)| *a = val),
                -0.4f64..=0.4f64,
            ),
            slider_row(
                "b (blue-yellow)",
                move || lab_signal.get().2,
                move |val| lab_signal.update(|(_, _, b)| *b = val),
                -0.4f64..=0.4f64,
            ),
        ],
    );

    (sliders, color_display(rgb_signal))
        .v_stack()
        .class(SliderControl)
}

fn display_p3_color_view() -> impl IntoView {
    let p3_signal = RwSignal::new((0.5, 0.5, 0.5));
    let rgb_signal = DerivedRwSignal::new(
        p3_signal,
        |(r, g, b)| {
            let p3 = AlphaColor::<DisplayP3>::new([*r, *g, *b, 1.0]);
            p3.convert()
        },
        |rgb: &Color| {
            let p3: AlphaColor<DisplayP3> = rgb.convert();
            let [r, g, b, _] = p3.components;
            (r, g, b)
        },
    );

    let sliders = labeled_slider_grid(
        "Display P3",
        [
            slider_row(
                "Red",
                move || p3_signal.get().0,
                move |val| p3_signal.update(|(r, _, _)| *r = val),
                0f64..=1f64,
            ),
            slider_row(
                "Green",
                move || p3_signal.get().1,
                move |val| p3_signal.update(|(_, g, _)| *g = val),
                0f64..=1f64,
            ),
            slider_row(
                "Blue",
                move || p3_signal.get().2,
                move |val| p3_signal.update(|(_, _, b)| *b = val),
                0f64..=1f64,
            ),
        ],
    );

    (sliders, color_display(rgb_signal))
        .v_stack()
        .class(SliderControl)
}

fn rgb_color_view() -> impl IntoView {
    let rgb_signal = RwSignal::new((128.0, 128.0, 128.0));
    let rgb_derived = DerivedRwSignal::new(
        rgb_signal,
        |(r, g, b)| Color::new([*r / 255.0, *g / 255.0, *b / 255.0, 1.0]),
        |rgb: &Color| {
            let [r, g, b, _a] = rgb.components;
            (r * 255.0, g * 255.0, b * 255.0)
        },
    );

    let sliders = labeled_slider_grid(
        "sRGB",
        [
            slider_row(
                "Red",
                move || rgb_signal.get().0,
                move |val| rgb_signal.update(|(r, _, _)| *r = val),
                0f64..=255f64,
            ),
            slider_row(
                "Green",
                move || rgb_signal.get().1,
                move |val| rgb_signal.update(|(_, g, _)| *g = val),
                0f64..=255f64,
            ),
            slider_row(
                "Blue",
                move || rgb_signal.get().2,
                move |val| rgb_signal.update(|(_, _, b)| *b = val),
                0f64..=255f64,
            ),
        ],
    );

    (sliders, color_display(rgb_derived))
        .v_stack()
        .class(SliderControl)
}

fn palette_color(color: impl Fn() -> Color + 'static + Copy) -> impl IntoView {
    let swatch = empty().style(move |s| {
        s.background(color())
            .min_width(100)
            .width_full()
            .aspect_ratio(235.0 / 185.0)
    });

    let lightness = move || color().convert::<Oklab>().components[0];
    let text_color = move || {
        if lightness() > 0.5 {
            css::BLACK
        } else {
            css::WHITE
        }
    };
    let hex = move || {
        let rgba = color().to_rgba8();
        format!("#{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b)
    };
    let swatch_hex_label = label(hex).style(move |s| {
        s.absolute()
            .padding(10)
            .font_size(14)
            .color(text_color())
            .font_family("monospace")
    });

    (swatch, swatch_hex_label)
        .v_stack()
        .style(|s| s.items_center().width_full())
}

fn palette_row(
    num_steps: impl Fn() -> usize + 'static,
    make_color: impl Fn(usize) -> Color + Copy + 'static,
) -> impl IntoView {
    dyn_view(move || {
        let steps = num_steps();
        (0..steps)
            .map(move |step_idx| palette_color(move || make_color(step_idx)))
            .h_stack()
            .style(|s| s.gap(10).width_full())
    })
    .style(|s| s.flex_grow(1.))
}

fn palette_controls() -> impl IntoView {
    let num_palettes = RwSignal::new(8u32);
    let num_steps = RwSignal::new(4u32);
    let chroma = RwSignal::new(0.05f32);
    let lightness_min = RwSignal::new(0.8f32);
    let lightness_max = RwSignal::new(0.95f32);
    let hue_offset = RwSignal::new(0.0f32);

    let sliders = labeled_slider_grid(
        "Palette",
        [
            slider_row(
                "Palettes",
                move || num_palettes.get() as f32,
                move |val| num_palettes.set(val.round() as u32),
                1f64..=16f64,
            ),
            slider_row(
                "Steps",
                move || num_steps.get() as f32,
                move |val| num_steps.set(val.round() as u32),
                2f64..=8f64,
            ),
            slider_row(
                "Chroma",
                move || chroma.get(),
                move |val| chroma.set(val),
                0f64..=0.2f64,
            ),
            slider_row(
                "Light Min",
                move || lightness_min.get(),
                move |val| lightness_min.set(val),
                0.2f64..=1f64,
            ),
            slider_row(
                "Light Max",
                move || lightness_max.get(),
                move |val| lightness_max.set(val),
                0.2f64..=1f64,
            ),
            slider_row(
                "Hue Offset",
                move || hue_offset.get(),
                move |val| hue_offset.set(val),
                0f64..=360f64,
            ),
        ],
    )
    .style(|s| s.max_width(500).margin(PxPctAuto::Auto));

    let make_color = move |palette_idx: usize, step_idx: usize| {
        let n = num_palettes.get() as usize;
        let steps = num_steps.get() as usize;
        let c = chroma.get();
        let l_min = lightness_min.get();
        let l_max = lightness_max.get();
        let h_off = hue_offset.get();

        let hue = ((palette_idx as f32 / n as f32) * 360.0 + h_off) % 360.0;
        let l = if steps > 1 {
            l_max - (l_max - l_min) * (step_idx as f32 / (steps - 1) as f32)
        } else {
            (l_max + l_min) / 2.0
        };
        AlphaColor::<Oklch>::new([l, c, hue, 1.0]).convert::<Srgb>()
    };

    let palettes_view = dyn_view(move || {
        let n = num_palettes.get() as usize;

        (0..n)
            .map(move |palette_idx| {
                palette_row(
                    move || num_steps.get() as usize,
                    move |step_idx| make_color(palette_idx, step_idx),
                )
            })
            .h_stack()
            .style(|s| s.gap(20).width_full().flex_wrap(FlexWrap::Wrap))
    })
    .style(|s| s.width_full());

    (sliders, palettes_view)
        .v_stack()
        .style(|s| s.width_full().gap(10))
}

fn app_view() -> impl IntoView {
    let color_controls = (
        rgb_color_view(),
        lch_color_view(),
        oklab_color_view(),
        display_p3_color_view(),
    )
        .h_stack()
        .style(|s| {
            s.gap(10)
                .width_full()
                .padding(10)
                .flex_wrap(FlexWrap::Wrap)
                .justify_center()
        });

    (color_controls, palette_controls())
        .v_stack()
        .style(|s| {
            s.padding(6)
                .gap(10)
                .flex_grow(1.)
                .width_full()
                .class(SliderGrid, |s| {
                    s.gap(3)
                        .grid()
                        .width_full()
                        .items_center()
                        .grid_template_columns([auto(), fr(1.)])
                })
                .class(SliderControl, |s| s.width_full().gap(10).max_width(300))
        })
        .scroll()
        .style(|s| s.margin_horiz(10).padding_vert(10).size_full())
}

fn main() {
    let window_config = WindowConfig::default().title("Color Palette");

    Application::new()
        .window(|_| app_view(), Some(window_config))
        .run();
}
