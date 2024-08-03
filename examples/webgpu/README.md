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

## Specifying the canvas container

You must specify the ID of the parent element of the canvas in the `WindowConfig` struct.
This is because the canvas is created as a child of the specified element.

```rust
let window_config = WindowConfig::default()
    .with_web_config(|w| w.canvas_parent_id("canvas-container"));
```

The application will otherwise panic with the following message:

```
Specify a parent element ID for the canvas.
```

## Resizing the canvas

The canvas has a default size of 800x600. You can change this by changing the `size` field of the `WindowConfig`.

```rust
let window_config = WindowConfig::default()
    .size(Size::new(800.0, 600.0));
```

## Fonts

This example comes with a selection of fonts in the `fonts` directory.
For simplicity, these are embedded in the binary in this example.
Without manually configuring fonts, cosmic-text won't find any fonts and will panic.
At the time of this writing, the default fonts (precisely those in the `fonts` directory) are hardcoded in cosmic-text.
