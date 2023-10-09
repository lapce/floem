> [!WARNING]  
> **Work in progress, which is subject to frequent change.**
 If you plan to add changes, please make sure you reach out in our discord first.

 The end-goal is to support reactive animations, similar to Swift UI(including spring animations). 
 The API we are currently aiming for looks something like this:
 ```rust
    let (is_hovered, set_is_hovered) = create_signal(false);
    let (scroll_offset_pct, set_scroll_offset_pct) = create_signal(0.);

    scroll({
        button()
            .style(|s| {
                s.width(move || {50.0})
            })
            .animation(|s| {
                s.width(300)
                // we get animation on scroll "for free", since everything is integrated with the reactive system
                .opacity(move || scroll_offset_pct)
                .scale(move || is_hovered.get() {1.2} else {1.0}  )
                .easing_fn(EasingFn::Cubic)
                .ease_in_out()
                .duration(Duration::from_secs(1))
            })
    }.on_scroll(move |scroll| {
        let offset_pct = ......snip........
        set_scroll_offset_pct.update(|value| *value = offset_pct);
        true
      })
    )
 ```
