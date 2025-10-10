use floem::prelude::*;

use crate::form::{form, form_item};

// Source: https://www.svgrepo.com/svg/509804/check | License: MIT
const CUSTOM_CHECK_SVG: &str = r##"
<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
<path fill-rule="evenodd" clip-rule="evenodd" d="M20.6097 5.20743C21.0475 5.54416 21.1294 6.17201 20.7926 6.60976L10.7926 19.6098C10.6172 19.8378 10.352 19.9793 10.0648 19.9979C9.77765 20.0166 9.49637 19.9106 9.29289 19.7072L4.29289 14.7072C3.90237 14.3166 3.90237 13.6835 4.29289 13.2929C4.68342 12.9024 5.31658 12.9024 5.70711 13.2929L9.90178 17.4876L19.2074 5.39034C19.5441 4.95258 20.172 4.87069 20.6097 5.20743Z" fill="#000000"/>
</svg>
"##;

// Source: https://www.svgrepo.com/svg/505349/cross | License: MIT
pub const CROSS_SVG: &str = r##"
<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
<path d="M19 5L5 19M5.00001 5L19 19" stroke="#000000" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
"##;

pub fn checkbox_view() -> impl IntoView {
    // let width = 160.0;
    let is_checked = RwSignal::new(true);
    form((
        form_item("Checkbox:", Checkbox::new_rw(is_checked)),
        form_item(
            "Custom Checkbox 1:",
            Checkbox::new_rw_custom(is_checked, CUSTOM_CHECK_SVG)
                .style(|s| s.color(palette::css::GREEN)),
        ),
        form_item(
            "Disabled Checkbox:",
            checkbox(move || is_checked.get()).style(|s| s.set_disabled(true)),
        ),
        form_item(
            "Labeled Checkbox:",
            Checkbox::labeled_rw(is_checked, || "Check me!"),
        ),
        form_item(
            "Custom Checkbox 2:",
            Checkbox::custom_labeled_rw(is_checked, move || "Custom Check Mark", CROSS_SVG)
                .style(|s| s.class(CheckboxClass, |s| s.color(palette::css::RED))),
        ),
        form_item(
            "Disabled Labeled Checkbox:",
            labeled_checkbox(move || is_checked.get(), || "Check me!")
                .style(|s| s.set_disabled(true)),
        ),
    ))
}
