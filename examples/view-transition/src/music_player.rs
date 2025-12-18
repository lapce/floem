use floem::{
    animate::Animation,
    peniko::{color::palette, Brush, Color},
    reactive::{RwSignal, SignalGet, SignalUpdate},
    style::{ScaleX, ScaleY, Style, Transition},
    text::Weight,
    unit::{DurationUnitExt, UnitExt},
    views::{
        dyn_container, h_stack, slider, svg, v_stack, ButtonClass, Container, Decorators, Stack,
        SvgClass,
    },
    AnyView, IntoView,
};

use crate::box_shadow;

const FONT_SIZE: f32 = 12.;
const BACKGROUND: Color = Color::from_rgb8(235, 235, 240);
const SLIDER: Color = Color::from_rgb8(210, 209, 216);
const ICON: Color = Color::from_rgb8(120, 120, 127); // medium gray - icons and accent bar and image
const MUSIC_ICON: Color = Color::from_rgb8(11, 11, 21);
const TEXT_COLOR: Color = Color::from_rgb8(48, 48, 54);

mod svg;

#[derive(Debug, Clone)]
struct SongInfo {
    title: String,
    artist: String,
}
impl Default for SongInfo {
    fn default() -> Self {
        Self {
            title: "Cool Song Title".to_string(),
            artist: "Artist Name".to_string(),
        }
    }
}
impl IntoView for SongInfo {
    type V = Stack;
    type Intermediate = Stack;

    fn into_intermediate(self) -> Self::Intermediate {
        let song_artist = v_stack((
            self.title.style(|s| s.font_weight(Weight::MEDIUM)),
            self.artist
                .style(|s| s.font_size(FONT_SIZE * 0.8).color(palette::css::GRAY)),
        ))
        .style(|s| s.gap(5.));

        h_stack((
            ().style(|s| s.size(50, 50).border_radius(8).background(ICON)),
            song_artist,
        ))
        .style(|s| s.gap(10).items_center())
    }
}

#[derive(Debug, Clone, Copy)]
enum PlayPause {
    Play,
    Pause,
}
impl PlayPause {
    fn toggle(&mut self) {
        *self = match self {
            Self::Play => Self::Pause,
            Self::Pause => Self::Play,
        };
    }
    fn view(self) -> AnyView {
        match self {
            Self::Play => svg(svg::PLAY).into_any(),
            Self::Pause => svg(svg::PAUSE).into_any(),
        }
        .animation(|a| Animation::scale_effect(a).run_on_remove(false))
    }
}

pub fn music_player() -> impl IntoView {
    let song_info = RwSignal::new(SongInfo::default());

    let now_playing = h_stack((
        svg(svg::MUSIC).style(|s| s.color(MUSIC_ICON)),
        "Now Playing".style(|s| s.font_weight(Weight::MEDIUM)),
    ))
    .style(|s| s.gap(5).items_center());

    let play_pause_state = RwSignal::new(PlayPause::Play);

    let play_pause_button = Container::new(
        dyn_container(move || play_pause_state.get(), PlayPause::view).class(ButtonClass),
    )
    .on_click_stop(move |_| play_pause_state.update(PlayPause::toggle));

    let media_buttons = h_stack((
        Container::new(svg(svg::BACKWARD)).class(ButtonClass),
        play_pause_button,
        Container::new(svg(svg::FORWARD)).class(ButtonClass),
    ))
    .style(|s| {
        s.align_self(Some(floem::taffy::AlignItems::Center))
            .items_center()
            .gap(20)
            .class(SvgClass, |s| s.color(MUSIC_ICON))
    });

    let card = v_stack((
        now_playing,
        dyn_container(move || song_info.get(), |info| info),
        slider::slider(move || 40.pct())
            .style(|s| s.width_full())
            .slider_style(|s| {
                s.bar_height(3)
                    .accent_bar_height(3.)
                    .bar_color(SLIDER)
                    .accent_bar_color(ICON)
                    .handle_color(Brush::Solid(palette::css::TRANSPARENT))
                    .handle_radius(0)
            }),
        media_buttons,
    ))
    .style(|s| {
        s.background(BACKGROUND)
            .size_full()
            .border_radius(8)
            .padding(15)
            .gap(10)
            .width(300)
            .apply(box_shadow())
    });

    let button_style = |s: Style| {
        s.border(0.)
            .padding(5)
            .items_center()
            .justify_center()
            .background(palette::css::TRANSPARENT)
            .hover(|s| s.background(SLIDER))
            .active(|s| {
                s.class(SvgClass, |s| {
                    s.color(ICON).scale_x(50.pct()).scale_y(50.pct())
                })
            })
    };

    Container::new(card).style(move |s| {
        s.size(300, 175)
            .items_center()
            .justify_center()
            .font_size(FONT_SIZE)
            .color(TEXT_COLOR)
            .class(SvgClass, |s| {
                s.size(20, 20)
                    .items_center()
                    .justify_center()
                    .scale(100.pct())
                    .transition(ScaleX, Transition::spring(50.millis()))
                    .transition(ScaleY, Transition::spring(50.millis()))
                    .transition_color(Transition::linear(50.millis()))
            })
            .class(ButtonClass, button_style)
    })
}
