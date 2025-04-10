use std::fmt::Display;

use floem::views::StackExt;
use floem::{
    peniko::color::palette,
    reactive::{create_rw_signal, RwSignal, SignalGet, SignalUpdate},
    unit::UnitExt,
    views::{button, dyn_container, empty, text, text_input, v_stack, Decorators, RadioButton},
    IntoView,
};
use strum::IntoEnumIterator;
use time::Date;

fn oneway_message(start_text: String) -> String {
    format!("You have booked a one-way flight on {start_text}")
}

fn return_message(start_text: String, return_text: String) -> String {
    format!("You have booked a flight on {start_text} and a return flight on {return_text}",)
}

#[derive(Eq, PartialEq, Clone, Copy, strum::EnumIter)]
enum FlightMode {
    OneWay,
    Return,
}
impl Display for FlightMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlightMode::OneWay => f.write_str("One Way Flight"),
            FlightMode::Return => f.write_str("Return Flight"),
        }
    }
}

static DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[day]-[month]-[year]");

pub fn app_view() -> impl IntoView {
    let flight_mode = RwSignal::new(FlightMode::OneWay);

    let start_text = create_rw_signal("24-02-2024".to_string());
    let start_date = move || Date::parse(&start_text.get(), &DATE_FORMAT).ok();
    let start_date_is_valid = move || start_date().is_some();

    let return_text = create_rw_signal("24-02-2024".to_string());
    let return_date = move || Date::parse(&return_text.get(), &DATE_FORMAT).ok();
    let return_text_is_enabled = move || flight_mode.get() == FlightMode::Return;
    let return_date_is_valid = move || {
        if return_text_is_enabled() {
            return_date().is_some()
        } else {
            true
        }
    };

    let dates_are_chronological = move || match flight_mode.get() {
        FlightMode::OneWay => true,
        FlightMode::Return => match (return_date(), start_date()) {
            (Some(ret), Some(start)) => ret >= start,
            _ => false,
        },
    };

    let did_booking = create_rw_signal(false);

    let mode_picker = FlightMode::iter()
        .map(move |fm| RadioButton::new_labeled_rw(fm, flight_mode, move || fm))
        .h_stack();

    let start_date_input = text_input(start_text)
        .placeholder("Start date")
        .style(move |s| s.apply_if(!start_date_is_valid(), |s| s.background(palette::css::RED)));
    let return_date_input = text_input(return_text)
        .placeholder("Return date")
        .style(move |s| s.apply_if(!return_date_is_valid(), |s| s.background(palette::css::RED)))
        .disabled(move || !return_text_is_enabled());

    let book_button = button("Book")
        .disabled(move || {
            !(dates_are_chronological() && start_date_is_valid() && return_date_is_valid())
        })
        .action(move || did_booking.set(true));

    let success_message = dyn_container(
        move || (did_booking.get(), flight_mode.get()),
        move |value| match value {
            (true, FlightMode::OneWay) => text(oneway_message(start_text.get())).into_any(),
            (true, FlightMode::Return) => {
                text(return_message(start_text.get(), return_text.get())).into_any()
            }
            (false, _) => empty().into_any(),
        },
    );

    v_stack((
        mode_picker,
        start_date_input,
        return_date_input,
        book_button,
        success_message,
    ))
    .style(|s| s.row_gap(5))
    .style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn main() {
    floem::launch(app_view);
}
