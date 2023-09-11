<div align="center">

# Floem

A native Rust UI library with fine-grained reactivity
</div>

It's still early days so expect lots of things missing!

```rust
fn app_view() -> impl View {
    // create a counter reactive signal with initial value 0
    let (counter, set_counter) = create_signal(0);

    // create user interface with Floem view functions
    stack((
        label(move || format!("Value: {}", counter.get())),
        stack((
            text("Increment")
                .on_click(move |_| {
                    set_counter.update(|value| *value += 1);
                    true
                }),
            text("Decrement")
                .on_click(move |_| {
                    set_counter.update(|value| *value -= 1);
                    true
                }),
        )),
    ))
}

fn main() {
    floem::launch(app_view);
}
```


## Features
Inspired by [Xilem](https://github.com/linebender/xilem), [Leptos](https://github.com/leptos-rs/leptos) and [rui](https://github.com/audulus/rui), Floem aims to be a high performance declarative UI library with minimal effort from the user. 
- **Cross-platform support**: Supports Windows, macOS and Linux with rendering using [Wgpu](https://github.com/gfx-rs/wgpu). A software renderer is also planned in case a GPU is unavailable.
- **Fine-grained reactivity**: The entire library is built around reactive primitives inspired by [leptos_reactive](https://crates.io/crates/leptos_reactive). The reactive "signals" give the user a nice way to do reactive state management while maintaining very high performance.
- **Performance**: The view tree is only run once, so the user can't accidentally put something expensive in the view generation function which slows down the whole application. The library also provides tools to help users write performant UI code. Check out the high performance [virtual list example](https://github.com/lapce/floem/tree/main/examples/virtual_list)
- **Flexbox layout**: Using [taffy](https://crates.io/crates/taffy), the library provides the Flexbox (or Grid) layout system, which can be applied to any View node.


## Contributions
[Contributions welcome!](CONTRIBUTING.md) If you'd like to improve how Floem works and fix things, feel free to open an issue or submit a PR. If you'd like a conversation with Floem devs, you can join in the #floem channel on this [Discord](https://discord.gg/RB6cRYerXX).
