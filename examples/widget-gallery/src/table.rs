use floem::prelude::*;

use crate::form::{form, form_item};

pub fn table_view() -> impl IntoView {
    form((form_item(
        "Virtualized Tip Percentage Table",
        percentage_table(),
    ),))
}

fn percentage_table() -> impl IntoView {
    let base_prices = im::vector![
        20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99, 15.49, 89.99, 67.50, 23.99,
        129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25, 149.99, 28.75, 95.00, 82.49, 17.99,
        165.00, 39.99, 72.50, 19.99, 20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99,
        15.49, 89.99, 67.50, 23.99, 129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25, 149.99,
        28.75, 95.00, 82.49, 17.99, 165.00, 39.99, 72.50, 19.99, 20.00, 50.00, 75.00, 36.00, 52.00,
        99.00, 105.00, 42.99, 15.49, 89.99, 67.50, 23.99, 129.99, 8.99, 45.75, 12.99, 199.99,
        55.50, 33.25, 149.99, 28.75, 95.00, 82.49, 17.99, 165.00, 39.99, 72.50, 19.99, 20.00,
        50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99, 15.49, 89.99, 67.50, 23.99, 129.99, 8.99,
        45.75, 12.99, 199.99, 55.50, 33.25, 149.99, 28.75, 95.00, 82.49, 17.99, 165.00, 39.99,
        72.50, 19.99, 20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99, 15.49, 89.99, 67.50,
        23.99, 129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25, 149.99, 28.75, 95.00, 82.49,
        17.99, 165.00, 39.99, 72.50, 19.99, 20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00,
        42.99, 15.49, 89.99, 67.50, 23.99, 129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25,
        149.99, 28.75, 95.00, 82.49, 17.99, 165.00, 39.99, 72.50, 19.99, 20.00, 50.00, 75.00,
        36.00, 52.00, 99.00, 105.00, 42.99, 15.49, 89.99, 67.50, 23.99, 129.99, 8.99, 45.75, 12.99,
        199.99, 55.50, 33.25, 149.99, 28.75, 95.00, 82.49, 17.99, 165.00, 39.99, 72.50, 19.99,
        20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99, 15.49, 89.99, 67.50, 23.99,
        129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25, 149.99, 28.75, 95.00, 82.49, 17.99,
        165.00, 39.99, 72.50, 19.99, 20.00, 50.00, 75.00, 36.00, 52.00, 99.00, 105.00, 42.99,
        15.49, 89.99, 67.50, 23.99, 129.99, 8.99, 45.75, 12.99, 199.99, 55.50, 33.25, 149.99,
        28.75, 95.00, 82.49, 17.99, 165.00, 39.99, 72.50, 19.99,
    ];

    // create a slice of tip percentages and then create a column from each
    let columns = [15, 20, 25, 50, 75].iter().map(move |pc| {
        // a column needs
        // 1. a header which can be any view
        // 2. a closure that can generate the data for the column given the row as input
        // It is assumed that all rows, including the column headers have the same height.
        // There wil be layout issues if this isn't respected
        Column::new(format!("With {pc}% tip"), |(_idx, v)| {
            let percent = *v * (*pc as f64 / 100. + 1.);
            format!("${percent:.2}")
        })
    });

    // create a table by supplying the input data and a `key_fn`.
    // The key function is used to determine if items are unique.
    // Here we use the index of the item in the table as it's unique identifier.
    // In other situations, other unique identifiers will need to be used
    let table = table(move || base_prices.clone().enumerate(), |(idx, _p)| *idx)
        // there are two ways to add colums. First by adding an individual column by passing in the title and closure
        .column("Base price", |(_idx, p)| format!("${p:.2}"))
        // or by supplying an iterator of columns
        .columns(columns);

    table
        .style(|s| {
            s.row_gap(10)
                .col_gap(30)
                .flex_grow(1.)
                .items_start()
                // it is important that all rows have the same height. You can include gaps, padding, margins, etc but all rows must be the same height.
                .class(LabelClass, |s| s.height(15))
        })
        .scroll()
        .style(|s| {
            s.border(1.0)
                .padding_horiz(15)
                .padding_right(20 + 15)
                .height(400.)
        })
}
