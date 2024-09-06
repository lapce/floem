use floem::{
    animate::Bezier,
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

const FONT_SIZE: f32 = 12.;
const BACKGROUND: Color = Color::rgb8(235, 235, 240);
const SLIDER: Color = Color::rgb8(210, 209, 216);
const ICON: Color = Color::rgb8(120, 120, 127); // medium gray - icons and accent bar and image
const MUSIC_ICON: Color = Color::rgb8(11, 11, 21);
const TEXT_COLOR: Color = Color::rgb8(48, 48, 54);

const PLAY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="size-6">
  <path fill-rule="evenodd" d="M4.5 5.653c0-1.427 1.529-2.33 2.779-1.643l11.54 6.347c1.295.712 1.295 2.573 0 3.286L7.28 19.99c-1.25.687-2.779-.217-2.779-1.643V5.653Z" clip-rule="evenodd" />
</svg>
"#;
const PAUSE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="size-6">
  <path fill-rule="evenodd" d="M6.75 5.25a.75.75 0 0 1 .75-.75H9a.75.75 0 0 1 .75.75v13.5a.75.75 0 0 1-.75.75H7.5a.75.75 0 0 1-.75-.75V5.25Zm7.5 0A.75.75 0 0 1 15 4.5h1.5a.75.75 0 0 1 .75.75v13.5a.75.75 0 0 1-.75.75H15a.75.75 0 0 1-.75-.75V5.25Z" clip-rule="evenodd" />
</svg>
"#;
// const FORWARD_SVG: &str = r#"b<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="size-6">
//   <path d="M5.055 7.06C3.805 6.347 2.25 7.25 2.25 8.69v8.122c0 1.44 1.555 2.343 2.805 1.628L12 14.471v2.34c0 1.44 1.555 2.343 2.805 1.628l7.108-4.061c1.26-.72 1.26-2.536 0-3.256l-7.108-4.061C13.555 6.346 12 7.249 12 8.689v2.34L5.055 7.061Z" />
// </svg>
// "#;
const BACKWARD_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="size-6">
  <path d="M9.195 18.44c1.25.714 2.805-.189 2.805-1.629v-2.34l6.945 3.968c1.25.715 2.805-.188 2.805-1.628V8.69c0-1.44-1.555-2.343-2.805-1.628L12 11.029v-2.34c0-1.44-1.555-2.343-2.805-1.628l-7.108 4.061c-1.26.72-1.26 2.536 0 3.256l7.108 4.061Z" />
</svg>
"#;
const MUSIC_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="size-6">
  <path fill-rule="evenodd" d="M19.952 1.651a.75.75 0 0 1 .298.599V16.303a3 3 0 0 1-2.176 2.884l-1.32.377a2.553 2.553 0 1 1-1.403-4.909l2.311-.66a1.5 1.5 0 0 0 1.088-1.442V6.994l-9 2.572v9.737a3 3 0 0 1-2.176 2.884l-1.32.377a2.553 2.553 0 1 1-1.402-4.909l2.31-.66a1.5 1.5 0 0 0 1.088-1.442V5.25a.75.75 0 0 1 .544-.721l10.5-3a.75.75 0 0 1 .658.122Z" clip-rule="evenodd" />
</svg>
"#;

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
            PlayPause::Play => svg(|| PLAY_SVG.to_string()).into_any(),
            PlayPause::Pause => svg(|| PAUSE_SVG.to_string()).into_any(),
        }
        .animation(|a| {
            a.view_transition()
                .animate_to_default(Bezier::EASE_IN_OUT.into())
                .keyframe(0, |kf| kf.style(|s| s.size(0, 0)))
                .debug_name("Scale the width and height from zero to the default")
                .run_on_remove(false)
                .with_duration(|a, d| a.duration(d * 1))
        })
    }
}

pub fn music_player() -> impl IntoView {
    let song_info = RwSignal::new(SongInfo::default());

    let now_playing = h_stack((
        svg(|| MUSIC_SVG.to_string()).style(|s| s.color(MUSIC_ICON)),
        "Now Playing".style(|s| s.font_weight(Weight::MEDIUM)),
    ))
    .style(|s| s.gap(5).items_center());

    let play_pause_state = RwSignal::new(PlayPause::Play);

    let play_pause_button = container(
        dyn_container(move || play_pause_state.get(), move |which| which.view()).class(ButtonClass),
    )
    .on_click_stop(move |_| play_pause_state.update(|which| which.toggle()));

    let media_buttons = h_stack((
        container(svg(|| BACKWARD_SVG.to_string())).class(ButtonClass),
        play_pause_button,
        container(svg(|| BACKWARD_SVG.to_string())).class(ButtonClass),
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
            .box_shadow_color(Color::BLACK.with_alpha_factor(0.7))
            .box_shadow_h_offset(3)
            .box_shadow_v_offset(3.)
            .box_shadow_blur(1.5)
    });

    container(card).style(|s| {
        s.size(300, 175)
            .items_center()
            .justify_center()
            .font_size(FONT_SIZE)
            .color(TEXT_COLOR)
            .class(SvgClass, |s| {
                s.size(20, 20)
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
