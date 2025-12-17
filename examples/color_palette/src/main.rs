use floem::kurbo::Size;
use floem::peniko::Color;
use floem::prelude::{Decorators, RwSignal};
use floem::reactive::{SignalGet, SignalUpdate};
use floem::style::Position;
use floem::views::slider::Slider;
use floem::views::{dyn_view, Empty, Label};
use floem::window::WindowConfig;
use floem::Application;
use floem::IntoView;
use palette::{Hsl, Hsv, IntoColor, Lch, Srgb};

fn create_color_sliders(
    label_text: &str,
    signal: RwSignal<f32>,
    update_fn: impl Fn(f32) + 'static,
) -> impl IntoView {
    let text_label = label_text.to_string();
    (
        Label::derived(move || text_label.clone()).style(|s| s.size(65, 30)),
        Slider::new(move || signal.get())
            .on_change_px(move |new_value| update_fn(new_value as f32))
            .style(|s| s.width(250)),
    )
        .style(|s| s.flex_row().gap(3).width_full())
}

fn create_color_display(
    red: RwSignal<f32>,
    green: RwSignal<f32>,
    blue: RwSignal<f32>,
) -> impl IntoView {
    (
        dyn_view(move || {
            format!(
                "rgb: ({}, {}, {})",
                (red.get() * 255.0) as i32,
                (green.get() * 255.0) as i32,
                (blue.get() * 255.0) as i32,
            )
        })
        .style(|s| s.width(50).font_size(18)),
        Empty::new().style(move |s| {
            s.background(Color::new([red.get(), green.get(), blue.get(), 1.0]))
                .size(310, 75)
        }),
    )
        .style(|s| s.flex_col().gap(10))
}

fn lch_view() -> impl IntoView {
    let l = RwSignal::new(40.0);
    let c = RwSignal::new(40.0);
    let h = RwSignal::new(40.0);

    let red = RwSignal::new(0.4);
    let green = RwSignal::new(0.4);
    let blue = RwSignal::new(0.4);

    let update_rgb = move |get_values: Box<dyn Fn() -> (f32, f32, f32)>| {
        let (l_val, c_val, h_val) = get_values();
        let rgb: Srgb = Lch::new(l_val, c_val, h_val).into_color();
        red.set(rgb.red);
        green.set(rgb.green);
        blue.set(rgb.blue);
    };

    (
        (
            create_color_sliders("Lightness", l, move |new_l| {
                l.set(new_l);
                update_rgb(Box::new(move || (new_l, c.get(), h.get())))
            }),
            create_color_sliders("Chroma", c, move |new_c| {
                c.set(new_c);
                update_rgb(Box::new(move || (l.get(), new_c, h.get())))
            }),
            create_color_sliders("Hue", h, move |new_h| {
                h.set(new_h);
                update_rgb(Box::new(move || (l.get(), c.get(), new_h)))
            }),
        )
            .style(|s| s.flex_col().gap(3)),
        create_color_display(red, green, blue),
    )
        .style(|s| s.flex_col().items_start().margin_right(10))
}
fn hsl_view() -> impl IntoView {
    let h = RwSignal::new(40.0);
    let s = RwSignal::new(40.0);
    let l = RwSignal::new(40.0);

    let red = RwSignal::new(0.2);
    let green = RwSignal::new(0.2);
    let blue = RwSignal::new(0.2);

    let update_rgb = move |get_values: Box<dyn Fn() -> (f32, f32, f32)>| {
        let (h_val, s_val, l_val) = get_values();
        let rgb: Srgb = Hsl::new(h_val, s_val, l_val).into_color();
        red.set(rgb.red);
        green.set(rgb.green);
        blue.set(rgb.blue);
    };

    (
        (
            create_color_sliders("Hue", h, move |new_h| {
                h.set(new_h);
                update_rgb(Box::new(move || {
                    (new_h * 3.6, s.get() / 100.0, l.get() / 100.0)
                }))
            }),
            create_color_sliders("Saturation", s, move |new_s| {
                s.set(new_s);
                update_rgb(Box::new(move || {
                    (h.get() * 3.6, new_s / 100.0, l.get() / 100.0)
                }))
            }),
            create_color_sliders("Lightness", l, move |new_l| {
                l.set(new_l);
                update_rgb(Box::new(move || {
                    (h.get() * 3.6, s.get() / 100.0, new_l / 100.0)
                }))
            }),
        )
            .style(|s| s.flex_col().gap(3)),
        create_color_display(red, green, blue),
    )
        .style(|s| s.flex_col().items_start().margin_right(10))
}

fn hsv_view() -> impl IntoView {
    let h = RwSignal::new(40.0);
    let s = RwSignal::new(40.0);
    let v = RwSignal::new(40.0);

    let red = RwSignal::new(0.2);
    let green = RwSignal::new(0.2);
    let blue = RwSignal::new(0.2);

    let update_rgb = move |get_values: Box<dyn Fn() -> (f32, f32, f32)>| {
        let (h_val, s_val, v_val) = get_values();
        let rgb: Srgb = Hsv::new(h_val, s_val, v_val).into_color();
        red.set(rgb.red);
        green.set(rgb.green);
        blue.set(rgb.blue);
    };

    (
        (
            create_color_sliders("Hue", h, move |new_h| {
                h.set(new_h);
                update_rgb(Box::new(move || {
                    (new_h * 3.6, s.get() / 100.0, v.get() / 100.0)
                }));
            }),
            create_color_sliders("Saturation", s, move |new_s| {
                s.set(new_s);
                update_rgb(Box::new(move || {
                    (h.get() * 3.6, new_s / 100.0, v.get() / 100.0)
                }))
            }),
            create_color_sliders("Value", v, move |new_v| {
                v.set(new_v);
                update_rgb(Box::new(move || {
                    (h.get() * 3.6, s.get() / 100.0, new_v / 100.0)
                }))
            }),
        )
            .style(|s| s.flex_col().gap(3)),
        create_color_display(red, green, blue),
    )
        .style(|s| s.flex_col().items_start().margin_right(10))
}

fn rgb_view() -> impl IntoView {
    let r = RwSignal::new(40);
    let g = RwSignal::new(40);
    let b = RwSignal::new(40);

    (
        (
            create_color_sliders("Red", RwSignal::new(r.get() as f32), move |new_r| {
                r.set(new_r as i32)
            }),
            create_color_sliders("Green", RwSignal::new(g.get() as f32), move |new_g| {
                g.set(new_g as i32)
            }),
            create_color_sliders("Blue", RwSignal::new(b.get() as f32), move |new_b| {
                b.set(new_b as i32)
            }),
        )
            .style(|s| s.flex_col().gap(3)),
        (
            dyn_view(move || format!("rgb: ({}, {}, {})", r.get(), g.get(), b.get(),))
                .style(|s| s.width(50).font_size(18)),
            ().style(move |s| {
                s.background(Color::from_rgb8(
                    r.get() as u8,
                    g.get() as u8,
                    b.get() as u8,
                ))
                .size(310, 75)
            }),
        )
            .style(|s| s.flex_col().gap(10)),
    )
        .style(|s| s.flex_col().items_start().margin_right(10))
}

fn palette() -> impl IntoView {
    (
        (
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(194, 255, 199))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#C2FFC7".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(158, 223, 156))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#9EDF9C".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(98, 130, 93))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#62825D".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(82, 110, 72))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#526E48".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(223, 242, 235))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#DFF2EB".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(185, 229, 232))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#B9E5E8".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(122, 178, 211))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#7AB2D3".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(74, 98, 138))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#4A628A".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(116, 9, 56))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#740938".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(175, 23, 64))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#AF1740".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(204, 43, 82))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#CC2B52".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(222, 124, 125))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#DE7C7D".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(255, 245, 228))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FFF5E4".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(255, 227, 225))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FFE3E1".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(255, 209, 209))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FFD1D1".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(255, 148, 148))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FF9494".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
        )
            .style(|s| s.flex_col().gap(10)),
        (
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(203, 157, 240))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#CB9DF0".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(240, 193, 225))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#F0C1E1".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(253, 219, 187))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FDDBBB".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(255, 249, 191))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FFF9BF".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(46, 7, 63))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#2E073F".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(122, 28, 172))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#7A1CAC".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(173, 73, 225))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#AD49E1".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(235, 211, 248))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#EBD3F8".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(111, 78, 55))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#6F4E37".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(166, 123, 91))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#A67B5B".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(236, 177, 118))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#ECB176".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(254, 216, 177))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FED8B1".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
            (
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(204, 213, 174))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#CCD5AE".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(224, 229, 182))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#E0E5B6".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(250, 237, 206))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FAEDCE".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
                (
                    Empty::new().style(|s| {
                        s.background(Color::from_rgb8(254, 250, 224))
                            .size(235, 185)
                            .position(Position::Relative)
                    }),
                    Label::derived(move || "#FEFAE0".to_string())
                        .style(|s| s.position(Position::Absolute).padding(10).font_size(18)),
                )
                    .style(|s| s.flex_col().items_center().gap(2)),
            )
                .style(|s| s.flex_row()),
        )
            .style(|s| s.flex_col().gap(10)),
    )
        .style(|s| s.flex_row().gap(20))
}

fn app_view() -> impl IntoView {
    ((rgb_view(), lch_view(), hsl_view(), hsv_view()), palette())
        .style(|s| s.flex_col().padding(6).gap(10))
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .size(Size::new(1800.0, 1000.0))
                    .title("Color Palette"),
            ),
        )
        .run();
}
