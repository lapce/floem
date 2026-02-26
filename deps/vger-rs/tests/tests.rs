use floem_vger::color::Color;
use floem_vger::defs::*;
use floem_vger::*;
use futures::executor::block_on;
extern crate rand;
mod common;
use common::*;
use std::sync::Arc;

fn load_font() -> fontdue::Font {
    let font_data =
        std::fs::read("C:/Windows/Fonts/arial.ttf").expect("Arial font not found on system");
    fontdue::Font::from_bytes(font_data.as_slice(), fontdue::FontSettings::default())
        .expect("Failed to parse Arial font")
}

/// Render a line of text at the given (x, y) baseline position.
///
/// Note: `render_glyph` does not use vger's transform stack — coordinates
/// are passed directly in screen space, matching how the floem adapter works.
fn render_text_line(
    vger: &mut Vger,
    font: &fontdue::Font,
    text: &str,
    font_size: f32,
    x: f32,
    y: f32,
    paint: PaintIndex,
) {
    let mut cursor_x = x;
    let size = font_size.round() as u32;

    for ch in text.chars() {
        let glyph_id = font.lookup_glyph_index(ch);
        let (metrics, bitmap) = font.rasterize(ch, font_size);

        if metrics.width > 0 && metrics.height > 0 {
            let image = GlyphImage {
                data: bitmap,
                width: metrics.width as u32,
                height: metrics.height as u32,
                left: metrics.xmin,
                top: metrics.height as i32 + metrics.ymin,
                colored: false,
            };

            vger.render_glyph(cursor_x.floor(), y.floor(), 0, glyph_id, size, (0, 0), || image, paint);
        }

        cursor_x += metrics.advance_width;
    }
}

fn setup() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let (device, queue) = block_on(common::setup());
    (Arc::new(device), Arc::new(queue))
}

#[test]
fn test_color_hex() {
    let c = Color::hex("#00D4FF").unwrap();
    assert_eq!(c.r, 0.0);
    assert_eq!(c.g, 212.0 / 255.0);
    assert_eq!(c.b, 1.0);
    assert_eq!(c.a, 1.0);

    let c = Color::hex_const("#00D4FF");
    assert_eq!(c.r, 0.0);
    assert_eq!(c.g, 0.831373);
    assert_eq!(c.b, 1.0);
    assert_eq!(c.a, 1.0);

    let c = Color::hex_const("#009BBA");
    assert_eq!(c.r, 0.0);
}

#[test]
fn fill_circle() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);
    let cyan = vger.color_paint(Color::CYAN);
    vger.fill_circle([100.0, 100.0], 20.0, cyan);

    render_test(&mut vger, &device, &queue, "circle.png", false);

    assert!(png_not_black("circle.png"));
}

#[test]
fn fill_circle_array() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);
    let cyan = vger.color_paint(Color::CYAN);

    for i in 0..5 {
        vger.fill_circle([100.0 * (i as f32), 100.0], 20.0, cyan);
    }

    render_test(&mut vger, &device, &queue, "circle_array.png", false);
}

#[test]
fn fill_circle_translate() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);
    let cyan = vger.color_paint(Color::CYAN);
    vger.translate([256.0, 256.0]);
    vger.fill_circle([0.0, 0.0], 20.0, cyan);

    render_test(&mut vger, &device, &queue, "circle_translate.png", false);
}

#[test]
fn fill_rect() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);
    let cyan = vger.color_paint(Color::CYAN);
    vger.fill_rect(euclid::rect(100.0, 100.0, 100.0, 100.0), 10.0, cyan, 0.0);

    render_test(&mut vger, &device, &queue, "rect.png", false);
}

#[test]
fn fill_rect_gradient() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [200.0, 200.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.fill_rect(euclid::rect(100.0, 100.0, 100.0, 100.0), 10.0, paint, 0.0);

    render_test(&mut vger, &device, &queue, "rect_gradient.png", false);
}

#[test]
fn stroke_rect_gradient() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [200.0, 200.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_rect(
        [100.0, 100.0].into(),
        [200.0, 200.0].into(),
        10.0,
        4.0,
        paint,
    );

    render_test(
        &mut vger,
        &device,
        &queue,
        "rect_stroke_gradient.png",
        false,
    );
}

#[test]
fn stroke_arc_gradient() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [300.0, 300.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_arc(
        [200.0, 200.0],
        100.0,
        4.0,
        0.0,
        std::f32::consts::PI / 2.0,
        paint,
    );

    render_test(&mut vger, &device, &queue, "arc_stroke_gradient.png", false);
}

#[test]
fn segment_stroke_gradient() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [200.0, 200.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_segment([100.0, 100.0], [200.0, 200.0], 4.0, paint);

    render_test(
        &mut vger,
        &device,
        &queue,
        "segment_stroke_gradient.png",
        false,
    );
}

#[test]
fn bezier_stroke_gradient() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [200.0, 200.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_bezier([100.0, 100.0], [150.0, 200.0], [200.0, 200.0], 4.0, paint);

    render_test(
        &mut vger,
        &device,
        &queue,
        "bezier_stroke_gradient.png",
        false,
    );
}

fn rand2<T: rand::Rng>(rng: &mut T) -> LocalPoint {
    LocalPoint::new(rng.gen_range(0.0..512.0), rng.gen_range(0.0..512.0))
}

#[test]
fn path_fill() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient([0.0, 0.0], [512.0, 512.0], Color::CYAN, Color::MAGENTA, 0.0);

    let mut rng = rand::thread_rng();

    let start = rand2(&mut rng);

    vger.move_to(start);

    for _ in 0..10 {
        vger.quad_to(rand2(&mut rng), rand2(&mut rng));
    }

    vger.quad_to(rand2(&mut rng), start);
    vger.fill(paint);

    let png_name = "path_fill.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));
}

#[test]
fn text() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let font = load_font();
    let paint = vger.color_paint(Color::WHITE);
    render_text_line(&mut vger, &font, "Hello, World!", 24.0, 32.0, 256.0, paint);

    let png_name = "text.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));
}

#[test]
fn text_small() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let font = load_font();
    let paint = vger.color_paint(Color::WHITE);
    render_text_line(&mut vger, &font, "Small text at 12px", 12.0, 32.0, 256.0, paint);

    let png_name = "text_small.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));

    let atlas_png_name = "text_small_atlas.png";
    save_png(
        &vger.glyph_cache.mask_atlas.atlas_texture,
        &floem_vger::atlas::Atlas::get_texture_desc(vger.glyph_cache.size, vger.glyph_cache.size),
        &device,
        &queue,
        atlas_png_name,
    );
}

#[test]
fn text_scale() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(256.0, 256.0, 2.0);

    let font = load_font();
    let paint = vger.color_paint(Color::WHITE);
    render_text_line(&mut vger, &font, "Scaled 2x", 18.0, 32.0, 128.0, paint);

    let png_name = "text_scale.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));

    let atlas_png_name = "text_scale_atlas.png";
    save_png(
        &vger.glyph_cache.mask_atlas.atlas_texture,
        &floem_vger::atlas::Atlas::get_texture_desc(vger.glyph_cache.size, vger.glyph_cache.size),
        &device,
        &queue,
        atlas_png_name,
    );
}

#[test]
fn text_box() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient([0.0, 0.0], [512.0, 512.0], Color::CYAN, Color::MAGENTA, 0.0);

    let lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";

    let font = load_font();

    // Simple word-wrap: render words, break to next line when exceeding max_width.
    let font_size = 18.0f32;
    let line_height = 24.0f32;
    let max_width = 448.0f32;
    let start_x = 32.0f32;
    let start_y = 32.0f32;
    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;

    for word in lorem.split_inclusive(' ') {
        let word_width: f32 = word
            .chars()
            .map(|ch| font.metrics(ch, font_size).advance_width)
            .sum();

        if cursor_x + word_width > max_width && cursor_x > 0.0 {
            cursor_x = 0.0;
            cursor_y += line_height;
        }

        render_text_line(&mut vger, &font, word, font_size, start_x + cursor_x, start_y + cursor_y, paint);

        cursor_x += word_width;
    }

    let png_name = "text_box.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));

    let atlas_png_name = "text_box_atlas.png";
    save_png(
        &vger.glyph_cache.mask_atlas.atlas_texture,
        &floem_vger::atlas::Atlas::get_texture_desc(vger.glyph_cache.size, vger.glyph_cache.size),
        &device,
        &queue,
        atlas_png_name,
    );
}

#[test]
fn test_scissor() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 2.0);

    vger.scissor(euclid::rect(200.0, 200.0, 100.0, 100.0), 0.0);
    let cyan = vger.color_paint(Color::WHITE);
    vger.fill_rect(euclid::rect(100.0, 100.0, 300.0, 300.0), 10.0, cyan, 0.0);

    let png_name = "scissor.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));
}

#[test]
fn test_scissor_text() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.color_paint(Color::WHITE);

    let font = load_font();
    // Scissor clips rendering to a region; text at (32, 256) should be partially visible.
    vger.scissor(euclid::rect(0.0, 230.0, 300.0, 50.0), 0.0);
    render_text_line(&mut vger, &font, "Clipped text in a scissor rect", 24.0, 32.0, 256.0, paint);

    let png_name = "text_box_scissor.png";
    render_test(&mut vger, &device, &queue, png_name, true);
    assert!(png_not_black(png_name));
}

#[test]
fn segment_stroke_stress() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient([0.0, 0.0], [512.0, 512.0], Color::CYAN, Color::MAGENTA, 0.0);

    for _ in 0..100000 {
        let mut rng = rand::thread_rng();
        let a = rand2(&mut rng);
        let b = rand2(&mut rng);

        vger.stroke_segment(a, b, 4.0, paint);
    }

    render_test(
        &mut vger,
        &device,
        &queue,
        "segment_stroke_stress.png",
        false,
    );
}

#[test]
fn segment_stroke_vertical() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [100.0, 200.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_segment([100.0, 100.0], [100.0, 200.0], 4.0, paint);

    render_test(
        &mut vger,
        &device,
        &queue,
        "segment_stroke_vertical.png",
        false,
    );
}

#[test]
fn segment_stroke_horizontal() {
    let (device, queue) = setup();

    let mut vger = Vger::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    vger.begin(512.0, 512.0, 1.0);

    let paint = vger.linear_gradient(
        [100.0, 100.0],
        [200.0, 100.0],
        Color::CYAN,
        Color::MAGENTA,
        0.0,
    );

    vger.stroke_segment([100.0, 100.0], [200.0, 100.0], 4.0, paint);

    render_test(
        &mut vger,
        &device,
        &queue,
        "segment_stroke_horizontal.png",
        false,
    );
}
