# Floem on WebGPU

**WARNING**: WebGPU support is highly experimental right now. Expect missing features, bugs, or performance issues.

## Requirements

* [Trunk](https://trunkrs.dev/)
* [Browser with WebGPU support](https://caniuse.com/webgpu)

## Run

From this directory, run:

```sh
trunk serve --open
```

## Specifying the canvas element

You must specify the ID of the HTML canvas element in the `WindowConfig` struct.

```rust
let window_config = WindowConfig::default()
    .with_web_config(|w| w.canvas_id("the-canvas"));
```

The application will otherwise panic with the following message:

```
Specify an id for the canvas.
```

## Resizing the canvas

By default, the floem window should automatically resize to fit the HTML canvas.
This is usually the desired behavior, as the canvas will thereby integrate with the rest of the web application.
You can change this behavior by specifying an explicit `size` in the `WindowConfig`.

```rust
let window_config = WindowConfig::default()
    .size(Size::new(800.0, 600.0));
```

Then, the canvas will have a fixed size and will not resize automatically based on the HTML canvas size.

## Fonts

This example comes with a selection of fonts in the `fonts` directory.
For simplicity, these are embedded in the binary in this example.
Without manually configuring fonts, cosmic-text won't find any fonts and will panic.
At the time of this writing, the default fonts (precisely those in the `fonts` directory) are hardcoded in cosmic-text.
