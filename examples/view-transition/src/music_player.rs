use floem::{
    animate::Animation,
    peniko::{Brush, Color},
    reactive::{RwSignal, SignalGet, SignalUpdate},
    style::{Background, Transition},
    text::Weight,
    unit::DurationUnitExt,
    views::{
        container, dyn_container, empty, h_stack, slider, svg, v_stack, ButtonClass, Decorators,
        Stack, SvgClass,
    },
    AnyView, IntoView,
};

use crate::box_shadow;

const FONT_SIZE: f32 = 12.;
const BACKGROUND: Color = Color::rgb8(235, 235, 240);
const SLIDER: Color = Color::rgb8(210, 209, 216);
const ICON: Color = Color::rgb8(120, 120, 127); // medium gray - icons and accent bar and image
const MUSIC_ICON: Color = Color::rgb8(11, 11, 21);
const TEXT_COLOR: Color = Color::rgb8(48, 48, 54);

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

    fn into_view(self) -> Self::V {
        let song_artist = v_stack((
            self.title.style(|s| s.font_weight(Weight::MEDIUM)),
            self.artist
                .style(|s| s.font_size(FONT_SIZE * 0.8).color(Color::GRAY)),
        ))
        .style(|s| s.gap(5.));

        h_stack((
            empty().style(|s| s.size(50, 50).border_radius(8).background(ICON)),
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
            PlayPause::Play => PlayPause::Pause,
            PlayPause::Pause => PlayPause::Play,
        };
    }
    fn view(&self) -> AnyView {
        match self {
            PlayPause::Play => svg(|| svg::PLAY.to_string()).into_any(),
            PlayPause::Pause => svg(|| svg::PAUSE.to_string()).into_any(),
        }
        .animation(|a| Animation::scale_effect(a).run_on_remove(false))
    }
}

pub fn music_player() -> impl IntoView {
    let song_info = RwSignal::new(SongInfo::default());

    let now_playing = h_stack((
        svg(|| svg::MUSIC.to_string()).style(|s| s.color(MUSIC_ICON)),
        "Now Playing".style(|s| s.font_weight(Weight::MEDIUM)),
    ))
    .style(|s| s.gap(5).items_center());

    let play_pause_state = RwSignal::new(PlayPause::Play);

    let play_pause_button = container(
        dyn_container(move || play_pause_state.get(), move |which| which.view()).class(ButtonClass),
    )
    .on_click_stop(move |_| play_pause_state.update(|which| which.toggle()));

    let media_buttons = h_stack((
        container(svg(|| svg::BACKWARD.to_string())).class(ButtonClass),
        play_pause_button,
        container(svg(|| svg::BACKWARD.to_string())).class(ButtonClass),
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
        slider::slider(move || 40.)
            .style(|s| s.width_full())
            .slider_style(|s| {
                s.bar_height(3)
                    .accent_bar_height(3.)
                    .bar_color(SLIDER)
                    .accent_bar_color(ICON)
                    .handle_color(Brush::Solid(Color::TRANSPARENT))
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

    container(card).style(|s| {
        s.size(300, 175)
            .items_center()
            .justify_center()
            .font_size(FONT_SIZE)
            .color(TEXT_COLOR)
            .class(SvgClass, |s| {
                s.size(20, 20)
                    .items_center()
                    .justify_center()
                    .transition_size(Transition::linear(25.millis()))
                    .transition_color(Transition::linear(25.millis()))
            })
            .class(ButtonClass, |s| {
                s.border(0)
                    .size(25, 25)
                    .items_center()
                    .justify_center()
                    .background(Color::TRANSPARENT)
                    .hover(|s| s.background(SLIDER))
                    .active(|s| {
                        s.set_style_value(Background, floem::style::StyleValue::Unset)
                            .class(SvgClass, |s| s.size(12, 12).color(ICON))
                    })
            })
    })
}
