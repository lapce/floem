use floem::{
    AnyView, Application, LazyView,
    action::{exec_after_animation_frame, set_window_scale},
    event::listener,
    kurbo::Size,
    new_window,
    peniko::Color,
    prelude::*,
    text::Alignment,
    theme::StyleThemeExt,
    view::ViewId,
    views::{ButtonClass, Decorators, Empty, NavigationStack, Overlay},
    window::{WindowConfig, WindowId},
};
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, Ordering},
};

thread_local! {
    static ROUTER: RefCell<Option<Router>> = const { RefCell::new(None) };
}

const BACK_CHEVRON_SVG: &str = r#"
<svg width="12" height="20" viewBox="0 0 12 20" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M10.5 1.5L2 10L10.5 18.5" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
"#;

const HOME_SVG: &str = r#"
<svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M4 10.5L12 4L20 10.5V20H14.5V14H9.5V20H4V10.5Z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
"#;

static PAYMENT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

const OS_MOD: Modifiers = if cfg!(target_os = "macos") {
    Modifiers::META
} else {
    Modifiers::CONTROL
};

#[derive(Clone, Copy)]
struct Spot {
    id: &'static str,
    city: &'static str,
    title: &'static str,
    subtitle: &'static str,
    accent: Color,
    detail: &'static str,
    price: &'static str,
}

impl Spot {
    fn key(self) -> &'static str {
        self.id
    }

    fn card(self) -> impl IntoView {
        let city = self
            .city
            .style(|s| {
                s.font_size(13.0)
                    .font_bold()
                    .with_theme(|s, t| s.color(t.text_muted()))
            })
            .debug_name("city");

        let title = self
            .title
            .style(|s| s.font_size(20.0).font_bold())
            .debug_name("title");

        let heading = (city, title)
            .v_stack()
            .style(|s| s.row_gap(4.0))
            .debug_name("heading");

        let price = self
            .price
            .style(|s| s.font_size(18.0).font_bold())
            .debug_name("price");

        let top_row = (heading, price)
            .h_stack()
            .style(|s| s.width_full().justify_between().items_center())
            .debug_name("top_row");

        let subtitle = self
            .subtitle
            .style(|s| s.font_size(14.0).with_theme(|s, t| s.color(t.text_muted())))
            .debug_name("subtitle");

        let dots = feature_dots(self.accent).debug_name("dots");

        let content = (top_row, subtitle, dots)
            .v_stack()
            .style(move |s| {
                s.width_full()
                    .padding(18.0)
                    .row_gap(10.0)
                    .border_color(self.accent.with_alpha(0.25))
                    .border(1.0)
                    .with_theme(move |s, t| {
                        s.border_radius(t.border_radius())
                            .background(t.bg_elevated())
                    })
            })
            .debug_name("content");

        content
            .button()
            .action(move || current_router().nav_to_detail(self))
            .class(ButtonClass)
            .style(|s| s.width_full())
            .debug_name("destination_card")
    }

    fn feature_banner(self) -> impl IntoView {
        let pills = (Pill("Curated"), Pill("iOS style"), Pill("Quiet stay"))
            .h_stack()
            .style(|s| {
                s.col_gap(8.0)
                    .row_gap(8.0)
                    .flex_wrap(floem::taffy::FlexWrap::Wrap)
            })
            .debug_name("pills");

        let title = self
            .title
            .style(|s| s.font_size(30.0).font_bold().color(Color::WHITE))
            .debug_name("title");

        let subtitle = self
            .subtitle
            .style(|s| s.font_size(14.0).color(Color::WHITE.with_alpha(0.88)))
            .debug_name("subtitle");

        (pills, title, subtitle)
            .v_stack()
            .style(move |s| {
                s.width_full()
                    .padding(22.0)
                    .row_gap(14.0)
                    .height(220.0)
                    .justify_end()
                    .border_radius(24.0)
                    .background(self.accent)
            })
            .debug_name("feature_banner")
    }

    fn detail_screen(self) -> impl IntoView {
        let header = MobileHeader::new("Stay Details", HeaderAction::Pop);
        let banner = self.feature_banner().debug_name("banner");

        let city = self
            .city
            .style(|s| {
                s.font_size(13.0)
                    .font_bold()
                    .with_theme(|s, t| s.color(t.text_muted()))
            })
            .debug_name("city");
        let title = self
            .title
            .style(|s| s.font_size(26.0).font_bold())
            .debug_name("title");
        let subtitle = self
            .subtitle
            .style(|s| s.font_size(15.0).with_theme(|s, t| s.color(t.text_muted())))
            .debug_name("subtitle");
        let detail = self
            .detail
            .style(|s| {
                s.font_size(14.0)
                    .line_height(1.5)
                    .with_theme(|s, t| s.color(t.text_muted()))
                    .text_wrap()
            })
            .debug_name("detail");
        let amenities = amenity_row().debug_name("amenities");
        let reserve = primary_button("Reserve this stay", move || {
            current_router().nav_to_booking(self)
        })
        .debug_name("reserve");

        let content = (city, title, subtitle, detail, amenities, reserve)
            .v_stack()
            .style(|s| s.width_full().row_gap(12.0).padding_bottom(24.0))
            .debug_name("content");

        (header, banner, content)
            .v_stack()
            .scroll()
            .style(|s| s.items_stretch().row_gap(18.0))
            .debug_name("detail_screen")
    }

    fn booking_screen(self) -> impl IntoView {
        let header = MobileHeader::new("Confirm Booking", HeaderAction::Pop);

        let summary_title = "Summary"
            .style(|s| s.font_size(18.0).font_bold())
            .debug_name("summary_title");
        let stay = booking_row("Stay", self.title).debug_name("stay");
        let rate = booking_row("Rate", self.price).debug_name("rate");
        let cleaning = booking_row("Cleaning", "$35").debug_name("cleaning");
        let service = booking_row("Service", "$18").debug_name("service");
        let total = booking_total("$293").debug_name("total");

        let summary = (summary_title, stay, rate, cleaning, service, total)
            .v_stack()
            .style(|s| {
                s.width_full()
                    .padding(18.0)
                    .row_gap(12.0)
                    .border(1.0)
                    .with_theme(|s, t| {
                        s.border_radius(t.border_radius())
                            .background(t.bg_elevated())
                            .border_color(t.border())
                    })
            })
            .debug_name("summary");

        let note = "The place is held for 12 minutes while you review."
            .style(|s| s.font_size(14.0).with_theme(|s, t| s.color(t.text_muted())))
            .debug_name("note");
        let pay = primary_button("Pay deposit", move || {
            current_router().complete_payment(self)
        })
        .debug_name("pay");

        let actions = (note, pay)
            .v_stack()
            .style(|s| s.width_full().row_gap(10.0))
            .debug_name("actions");

        (header, summary, actions)
            .v_stack()
            .style(|s| s.width_full().items_stretch().row_gap(18.0))
            .debug_name("booking_screen")
    }

    fn payment_result_screen(self, result: PaymentResult) -> impl IntoView {
        let header = MobileHeader::new("Payment Status", HeaderAction::Home);

        let badge = payment_badge(result).debug_name("badge");
        let title = result
            .title()
            .style(|s| {
                s.font_size(28.0)
                    .font_bold()
                    .text_align(Alignment::Center)
                    .text_wrap()
            })
            .debug_name("title");
        let message = result
            .message(self)
            .style(|s| {
                s.font_size(15.0)
                    .line_height(1.5)
                    .text_align(Alignment::Center)
                    .max_width_full()
                    .text_wrap()
                    .with_theme(|s, t| s.color(t.text_muted()))
            })
            .debug_name("message");

        let result_body = (badge, title, message)
            .v_stack()
            .style(move |s| {
                s.width_full()
                    .items_center()
                    .row_gap(14.0)
                    .padding(22.0)
                    .max_width(400)
                    .border(1.0)
                    .with_theme(move |s, t| {
                        let semantic = t.def(move |theme| result.color(&theme));
                        s.border_radius(t.border_radius())
                            .background(t.bg_elevated())
                            .border_color(semantic.map(|color| color.with_alpha(0.3)))
                    })
            })
            .debug_name("result_body");

        let primary = match result {
            PaymentResult::Success => {
                primary_button("Back home", || current_router().nav_to_home())
                    .debug_name("primary")
                    .into_any()
            }
            PaymentResult::Failure => primary_button("Try payment again", move || {
                current_router().nav_to_booking(self)
            })
            .debug_name("primary")
            .into_any(),
        };

        let secondary = secondary_button("Go Home", || current_router().nav_to_home())
            .debug_name("secondary")
            .into_any();

        let actions = match result {
            PaymentResult::Success => primary.into_any(),
            PaymentResult::Failure => Stack::new((primary, secondary))
                .style(|s| s.flex_col().width_full().row_gap(10.0))
                .debug_name("actions")
                .into_any(),
        };

        (header, result_body, actions)
            .v_stack()
            .style(|s| s.items_stretch().row_gap(18.0))
            .debug_name("payment_result_screen")
    }
}

const SPOTS: [Spot; 3] = [
    Spot {
        id: "desert-house",
        city: "Palm Springs",
        title: "Desert House",
        subtitle: "Pool, citrus garden, late check-out",
        accent: Color::from_rgb8(214, 109, 78),
        detail: "A warm modern stay with a shaded courtyard, a long pool, and a quiet reading room for slow mornings.",
        price: "$240",
    },
    Spot {
        id: "fjord-loft",
        city: "Bergen",
        title: "Fjord Loft",
        subtitle: "Sauna, harbor view, breakfast included",
        accent: Color::from_rgb8(53, 108, 126),
        detail: "Minimal but comfortable. Early fog, wood interiors, and a small sauna overlooking the water.",
        price: "$185",
    },
    Spot {
        id: "garden-studio",
        city: "Kyoto",
        title: "Garden Studio",
        subtitle: "Tea set, tatami room, private patio",
        accent: Color::from_rgb8(74, 128, 96),
        detail: "Compact and calm. Moss garden outside, soft light in the afternoon, and a tiny kitchen for one-pan dinners.",
        price: "$210",
    },
];

#[derive(Clone, Copy)]
enum Route {
    Home,
    Detail(Spot),
    Booking(Spot),
    PaymentResult(Spot, PaymentResult),
}

impl PartialEq for Route {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Route::Home, Route::Home) => true,
            (Route::Detail(left), Route::Detail(right)) => left.key() == right.key(),
            (Route::Booking(left), Route::Booking(right)) => left.key() == right.key(),
            (
                Route::PaymentResult(left_spot, left_result),
                Route::PaymentResult(right_spot, right_result),
            ) => left_spot.key() == right_spot.key() && left_result == right_result,
            _ => false,
        }
    }
}

impl Eq for Route {}

impl Hash for Route {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match *self {
            Route::Home => {
                0u8.hash(state);
            }
            Route::Detail(spot) => {
                1u8.hash(state);
                spot.key().hash(state);
            }
            Route::Booking(spot) => {
                2u8.hash(state);
                spot.key().hash(state);
            }
            Route::PaymentResult(spot, result) => {
                3u8.hash(state);
                spot.key().hash(state);
                result.hash(state);
            }
        }
    }
}

impl IntoView for Route {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        self.into_view()
    }

    fn into_view(self) -> Self::V {
        match self {
            Route::Home => home_screen().into_any(),
            Route::Detail(spot) => spot.detail_screen().into_any(),
            Route::Booking(spot) => spot.booking_screen().into_any(),
            Route::PaymentResult(spot, result) => spot.payment_result_screen(result).into_any(),
        }
        .style(|s| s.padding(18))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PaymentResult {
    Success,
    Failure,
}

impl PaymentResult {
    fn title(self) -> &'static str {
        match self {
            PaymentResult::Success => "Payment confirmed",
            PaymentResult::Failure => "Payment failed",
        }
    }

    fn message(self, spot: Spot) -> String {
        match self {
            PaymentResult::Success => format!(
                "Your deposit for {} is confirmed. Check-in details will arrive shortly.",
                spot.title
            ),
            PaymentResult::Failure => format!(
                "We couldn't complete the deposit for {}. Your card was not charged. Please try again.",
                spot.title
            ),
        }
    }

    fn color(self, theme: &floem::style::theme::DesignSystem) -> Color {
        match self {
            PaymentResult::Success => theme.success(),
            PaymentResult::Failure => theme.danger(),
        }
    }
}

#[derive(Clone, Copy)]
enum HeaderAction {
    Pop,
    Home,
}
impl IntoView for HeaderAction {
    type V = Button;

    type Intermediate = Self::V;

    fn into_intermediate(self) -> Self::Intermediate {
        self.into_view()
    }

    fn into_view(self) -> Self::V {
        match self {
            HeaderAction::Pop => svg(BACK_CHEVRON_SVG).style(|s| s.size(12, 12)),
            HeaderAction::Home => svg(HOME_SVG).style(|s| s.size(24, 24)),
        }
        .button()
        .style(move |s| {
            s.border_radius(999.0)
                .background(Color::WHITE)
                .color(Color::from_rgb8(44, 37, 33))
                .font_size(18.0)
                .font_bold()
        })
        .action(move || match self {
            HeaderAction::Pop => current_router().pop(),
            HeaderAction::Home => current_router().nav_to_home(),
        })
    }
}

#[derive(Clone, Copy)]
struct MobileHeader {
    title: &'static str,
    back: Option<HeaderAction>,
}

impl MobileHeader {
    fn new(title: &'static str, back: HeaderAction) -> Self {
        Self {
            title,
            back: Some(back),
        }
    }
}

impl IntoView for MobileHeader {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        self.into_view()
    }

    fn into_view(self) -> Self::V {
        let leading = match self.back {
            Some(action_button) => action_button.into_any(),
            None => Empty::new().into_any(),
        }
        .style(|s| s.size(44, 44));

        let title = self
            .title
            .style(|s| s.font_size(18.0).font_bold())
            .debug_name("title");
        let trailing = Empty::new()
            .style(|s| s.size(44.0, 44.0))
            .debug_name("trailing");

        (leading, title, trailing)
            .h_stack()
            .style(|s| {
                s.width_full()
                    .justify_between()
                    .items_center()
                    .padding_horiz(18.0)
                    .padding_vert(12.0)
                    .with_theme(|s, t| s.color(t.text()))
            })
            .debug_name("header")
            .into_any()
    }
}

#[derive(Clone, Copy)]
struct Router {
    path: RwSignal<Vec<Route>>,
    quick_actions_open: RwSignal<bool>,
}

impl Router {
    fn new() -> Self {
        Self {
            path: RwSignal::new(vec![Route::Home]),
            quick_actions_open: RwSignal::new(false),
        }
    }

    fn nav_to_home(self) {
        self.quick_actions_open.set(false);
        self.path.set(vec![Route::Home]);
    }

    fn nav_to_detail(self, spot: Spot) {
        self.quick_actions_open.set(false);
        self.path.set(vec![Route::Home, Route::Detail(spot)]);
    }

    fn push_detail(self, spot: Spot) {
        self.quick_actions_open.set(false);
        self.path.update(|path| {
            if matches!(path.last().copied(), Some(Route::Detail(current)) if current.id == spot.id)
            {
                return;
            }
            path.push(Route::Detail(spot));
        });
    }

    fn nav_to_booking(self, spot: Spot) {
        self.quick_actions_open.set(false);
        self.path.update(|path| match path.last().copied() {
            Some(Route::Booking(current)) if current.id == spot.id => {}
            Some(Route::Detail(current)) if current.id == spot.id => {
                path.push(Route::Booking(spot))
            }
            _ => *path = vec![Route::Home, Route::Detail(spot), Route::Booking(spot)],
        });
    }

    fn complete_payment(self, spot: Spot) {
        self.quick_actions_open.set(false);
        let attempt = PAYMENT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let result = if attempt.is_multiple_of(2) {
            PaymentResult::Success
        } else {
            PaymentResult::Failure
        };
        self.path.set(vec![
            Route::Home,
            Route::Detail(spot),
            Route::Booking(spot),
            Route::PaymentResult(spot, result),
        ]);
    }

    fn pop(self) {
        self.quick_actions_open.set(false);
        self.path.update(|path| {
            if path.len() > 1 {
                path.pop();
            }
        });
    }

    fn toggle_quick_actions(self) {
        self.quick_actions_open.update(|open| *open = !*open);
    }

    fn close_quick_actions(self) {
        self.quick_actions_open.set(false);
    }
}

fn current_router() -> Router {
    ROUTER.with(|router| router.borrow().expect("Router missing"))
}

fn main() {
    Application::new()
        .window(
            app_view,
            Some(
                WindowConfig::default()
                    .size(Size::new(393.0, 852.0))
                    .min_size(Size::new(393.0, 700.0))
                    .title("Navigation Stack Mobile"),
            ),
        )
        .on_event(|event| {
            if let floem::AppEvent::Reopen {
                has_visible_windows,
            } = event
                && !has_visible_windows
            {
                new_window(app_view, None);
            }
        })
        .run();
}

fn app_view(window_id: WindowId) -> impl IntoView {
    let router = Router::new();
    ROUTER.with(|current| {
        current.replace(Some(router));
    });
    let mut window_scale = RwSignal::new(1.0);

    let nav = NavigationStack::new(router.path)
        .style(|s| s.width_full().max_width(500.))
        .container()
        .style(|s| s.justify_center().size_full())
        .debug_name("nav");

    let fab_button = floating_action_button().into_intermediate();
    let fab_button_id = fab_button.view_id();

    let fab = Overlay::new(fab_button)
        .style(|s| s.absolute().inset_right(24.0).inset_bottom(28.0))
        .debug_name("fab");
    let quick_actions =
        Overlay::new_dyn(move || quick_action_sheet(fab_button_id)).debug_name("quick_actions");
    let overlays = Stack::new((fab, quick_actions)).debug_name("overlays");

    Stack::new((nav, overlays))
        .style(|s| {
            s.size_full()
                .font_size(14.0)
                .with_theme(|s, t| s.background(t.bg_base()).color(t.text()))
        })
        .window_title(|| "Navigation Stack Mobile".to_owned())
        .debug_name("app")
        .on_event_stop(
            el::KeyUp,
            move |_cx, KeyboardEvent { modifiers, key, .. }| {
                if *key == Key::Named(NamedKey::F11) {
                    floem::action::inspect();
                } else if *key == Key::Character("q".into()) && modifiers.contains(OS_MOD) {
                    floem::quit_app();
                } else if *key == Key::Character("w".into()) && modifiers.contains(OS_MOD) {
                    floem::close_window(window_id);
                }
            },
        )
        .on_event_stop(
            el::KeyDown,
            move |_, KeyboardEvent { key, modifiers, .. }| match key {
                Key::Character(ch) if (ch == "=" || ch == "+") && modifiers.contains(OS_MOD) => {
                    window_scale *= 1.1;
                    set_window_scale(window_scale.get());
                }
                Key::Character(ch) if ch == "-" && *modifiers == OS_MOD => {
                    window_scale /= 1.1;
                    set_window_scale(window_scale.get());
                }
                Key::Character(ch) if ch == "0" && *modifiers == OS_MOD => {
                    window_scale.set(1.0);
                    set_window_scale(window_scale.get());
                }
                _ => {}
            },
        )
}

fn home_screen() -> impl IntoView {
    let hero_title = "Stay somewhere with a point of view"
        .style(|s| s.font_size(28.0).font_bold().margin_top(18.0).text_wrap())
        .debug_name("hero_title");
    let hero_subtitle = "Hand-picked city escapes for a long weekend."
        .style(|s| s.font_size(14.0).with_theme(|s, t| s.color(t.text_muted())))
        .debug_name("hero_subtitle");
    let hero_dots = feature_dots(Color::from_rgb8(108, 132, 255)).debug_name("hero_dots");

    let hero = (hero_title, hero_subtitle, hero_dots)
        .v_stack()
        .style(|s| {
            s.padding(22.0)
                .row_gap(10.0)
                .border(1.0)
                .with_theme(|s, t| {
                    s.border_radius(t.border_radius())
                        .background(t.bg_elevated())
                        .border_color(t.border())
                })
        })
        .debug_name("hero");

    let section_title = "Tonight's picks"
        .style(|s| {
            s.font_size(22.0)
                .font_bold()
                .margin_top(34.0)
                .margin_bottom(6.0)
        })
        .debug_name("section_title");

    let cards = Stack::from_iter(SPOTS.into_iter().map(Spot::card))
        .style(|s| s.flex_col().items_stretch().row_gap(14.0))
        .debug_name("cards");

    (hero, section_title, cards)
        .v_stack()
        .scroll()
        .style(|s| s.items_stretch().row_gap(14.0))
        .debug_name("home_screen")
}

fn floating_action_button() -> impl IntoView {
    "Quick Jump"
        .button()
        .action(|| current_router().toggle_quick_actions())
        .class(ButtonClass)
        .style(|s| {
            s.width_full()
                .padding_vert(14.0)
                .padding_horiz(18.0)
                .font_size(16.0)
                .font_bold()
                .justify_center()
                .border_radius(999.0)
                .border(1.0)
                .with_theme(|s, t| {
                    s.background(t.def(|theme| theme.bg_elevated().with_alpha(0.96)))
                        .border_color(t.border())
                        .color(t.primary())
                })
        })
        .debug_name("floating_action_button")
}

fn quick_action_sheet(button_id: ViewId) -> impl IntoView {
    let visible = current_router().quick_actions_open.get();
    let button_rect = button_id.get_visual_rect();
    let button_gap = 12.0;
    let button_height = button_rect.height();

    let backdrop = ""
        .button()
        .action(|| current_router().close_quick_actions())
        .style(move |s| {
            s.absolute()
                .inset(0.0)
                .with_theme(|s, t| s.background(t.def(|theme| theme.text().with_alpha(0.18))))
        })
        .debug_name("backdrop");

    let title = "Quick jump"
        .style(|s| s.font_size(18.0).font_bold())
        .debug_name("title");
    let palm_springs =
        quick_action_button("Palm Springs", || current_router().push_detail(SPOTS[0]))
            .debug_name("palm_springs");
    let bergen = quick_action_button("Bergen", || current_router().push_detail(SPOTS[1]))
        .debug_name("bergen");
    let kyoto =
        quick_action_button("Kyoto", || current_router().push_detail(SPOTS[2])).debug_name("kyoto");
    let home = quick_action_button((svg(HOME_SVG).style(|s| s.size(24, 24)), "Home"), || {
        current_router().nav_to_home()
    })
    .debug_name("home");

    let last = current_router().path.with(|p| p.last().copied());
    let options = match last {
        Some(route) if route != Route::Home => (title, home, palm_springs, bergen, kyoto).v_stack(),
        _ => (title, palm_springs, bergen, kyoto).v_stack(),
    };

    let sheet = options
        .style(move |s| {
            s.absolute()
                .inset_bottom(28.0 + button_height + button_gap)
                .inset_right(24.0)
                .padding(18.0)
                .row_gap(10.0)
                .keyboard_navigable()
                .border(1.0)
                .with_theme(|s, t| {
                    s.border_radius(t.border_radius())
                        .background(t.bg_elevated())
                        .border_color(t.border())
                })
        })
        .on_event_stop(listener::FocusLost, |_, _| {
            current_router().close_quick_actions();
        })
        .on_event_stop(listener::PointerDown, |cx, _| {
            cx.prevent_default();
        })
        .debug_name("sheet");

    if visible {
        let sheet_id = sheet.id();
        exec_after_animation_frame(move |_| {
            sheet_id.request_focus();
        });
    }

    Stack::new((backdrop, sheet))
        .style(move |s| s.absolute().size_full().apply_if(!visible, |s| s.hide()))
        .debug_name("quick_action_sheet")
}

fn payment_badge(result: PaymentResult) -> impl IntoView {
    let label = match result {
        PaymentResult::Success => "Confirmed",
        PaymentResult::Failure => "Needs Attention",
    };

    label
        .style(move |s| {
            s.padding_horiz(14.0)
                .padding_vert(8.0)
                .border_radius(999.0)
                .font_bold()
                .with_theme(move |s, t| {
                    let semantic = t.def(move |theme| result.color(&theme));
                    s.background(semantic.clone().map(|color| color.with_alpha(0.14)))
                        .color(semantic)
                })
        })
        .debug_name("payment_badge")
}

fn primary_button(label: &'static str, action: impl Fn() + 'static) -> impl IntoView {
    label
        .button()
        .action(action)
        .class(ButtonClass)
        .style(|s| {
            s.width_full()
                .padding_vert(15.0)
                .font_bold()
                .font_size(15.0)
                .with_theme(|s, t| {
                    s.border_radius(t.border_radius())
                        .background(t.primary())
                        .color(t.bg_base())
                })
        })
        .debug_name("primary_button")
}

fn secondary_button(label: &'static str, action: impl Fn() + 'static) -> impl IntoView {
    label
        .button()
        .action(action)
        .class(ButtonClass)
        .style(|s| {
            s.width_full()
                .padding_vert(15.0)
                .font_bold()
                .font_size(15.0)
                .with_theme(|s, t| {
                    s.border_radius(t.border_radius())
                        .background(t.bg_overlay())
                        .color(t.text())
                })
        })
        .debug_name("secondary_button")
}

fn quick_action_button(
    view: impl IntoView + 'static,
    action: impl Fn() + 'static,
) -> impl IntoView {
    view.button()
        .action(action)
        .class(ButtonClass)
        .style(|s| {
            s.width_full()
                .justify_start()
                .padding(14.0)
                .with_theme(|s, t| {
                    s.border_radius(t.border_radius())
                        .background(t.bg_overlay())
                        .color(t.text())
                })
        })
        .debug_name("quick_action_button")
}

fn booking_row(label: &'static str, value: &'static str) -> impl IntoView {
    let label = label
        .style(|s| s.font_size(14.0).with_theme(|s, t| s.color(t.text_muted())))
        .debug_name("label");
    let value = value
        .style(|s| s.font_size(15.0).font_bold())
        .debug_name("value");

    (label, value)
        .h_stack()
        .style(|s| s.width_full().justify_between().items_center())
        .debug_name("booking_row")
}

fn booking_total(value: &'static str) -> impl IntoView {
    let label = "Total".style(|s| s.font_size(16.0).font_bold());

    let value = value
        .style(|s| s.font_size(20.0).font_bold())
        .debug_name("value");

    (label, value)
        .h_stack()
        .style(|s| {
            s.width_full()
                .justify_between()
                .items_center()
                .margin_top(6.0)
        })
        .debug_name("booking_total")
}

fn amenity_row() -> impl IntoView {
    (Pill("Breakfast"), Pill("Sauna"), Pill("Late checkout"))
        .h_stack()
        .style(|s| {
            s.width_full()
                .col_gap(8.0)
                .flex_wrap(floem::taffy::FlexWrap::Wrap)
        })
        .debug_name("amenity_row")
}

fn feature_dots(accent: Color) -> impl IntoView {
    Stack::horizontal((
        dot(accent),
        dot(accent.with_alpha(0.7)),
        dot(accent.with_alpha(0.45)),
    ))
    .style(|s| s.col_gap(6.0))
    .debug_name("feature_dots")
}

fn dot(color: Color) -> impl IntoView {
    Empty::new()
        .style(move |s| s.size(10.0, 10.0).border_radius(999.0).background(color))
        .debug_name("dot")
}

pub struct Pill(&'static str);
impl IntoView for Pill {
    type V = Label;

    type Intermediate = LazyView<&'static str>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self.0)
    }

    fn into_view(self) -> Self::V {
        self.0
            .style(|s| {
                s.padding_horiz(12.0)
                    .padding_vert(8.0)
                    .border_radius(999.0)
                    .background(Color::WHITE.with_alpha(0.18))
                    .color(Color::WHITE)
                    .selectable(false)
            })
            .debug_name("pill")
            .into_view()
    }
}
