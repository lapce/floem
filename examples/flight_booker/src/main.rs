use floem::{
    peniko::Color,
    reactive::{create_rw_signal, create_signal},
    unit::UnitExt,
    views::{
        button, dyn_container, empty, h_stack, labeled_radio_button, text, text_input, v_stack,
        Decorators,
    },
    IntoView,
};
use time::Date;

fn oneway_message(start_text: String) -> String {
    format!("You have booked a one-way flight on {start_text}")
}

fn return_message(start_text: String, return_text: String) -> String {
    format!("You have booked a flight on {start_text} and a return flight on {return_text}",)
}

#[derive(Eq, PartialEq, Clone)]
enum FlightMode {
    OneWay,
    Return,
}

static DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[day]-[month]-[year]");

pub fn app_view() -> impl IntoView {
    let (flight_mode, flight_mode_set) = create_signal(FlightMode::OneWay);

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

    let mode_picker = h_stack((
        labeled_radio_button(
            FlightMode::OneWay,
            move || flight_mode.get(),
            || "One way flight",
        )
        .on_click_stop(move |_| flight_mode_set.set(FlightMode::OneWay)),
        labeled_radio_button(
            FlightMode::Return,
            move || flight_mode.get(),
            || "Return flight",
        )
        .on_click_stop(move |_| flight_mode_set.set(FlightMode::Return)),
    ));

    let start_date_input = text_input(start_text)
        .placeholder("Start date")
        .style(move |s| s.apply_if(!start_date_is_valid(), |s| s.background(Color::RED)));
    let return_date_input = text_input(return_text)
        .placeholder("Return date")
        .style(move |s| s.apply_if(!return_date_is_valid(), |s| s.background(Color::RED)))
        .disabled(move || !return_text_is_enabled());

    let book_button = button(|| "Book")
        .disabled(move || {
            !(dates_are_chronological() && start_date_is_valid() && return_date_is_valid())
        })
        .on_click_stop(move |_| did_booking.set(true));

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
    .style(|s| s.column_gap(5))
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
