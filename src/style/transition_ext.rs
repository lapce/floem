//! Inspector-preview extension for `Transition`.
//!
//! `Transition` itself lives in `floem-style` so that the style engine can be
//! reused by hosts that do not pull in the view layer. Its rich inspector
//! preview (a canvas + tooltip) constructs `floem` views, so it's split out
//! into this extension trait that lives in the `floem` crate alongside the
//! view types it depends on.

use floem_reactive::{RwSignal, SignalGet};
use floem_renderer::Renderer;
use peniko::color::palette;
use peniko::kurbo::{self, Point, Stroke};
use taffy::prelude::{auto, fr};

use crate::theme::StyleThemeExt;
use crate::view::{IntoView, View};
use crate::views::{ContainerExt, Decorators, Label, Stack, TooltipExt, canvas};

pub trait TransitionDebugViewExt {
    fn debug_view(&self) -> Box<dyn View>;
}

impl TransitionDebugViewExt for floem_style::Transition {
    fn debug_view(&self) -> Box<dyn View> {
        let transition = self.clone();
        let easing_clone = transition.easing.clone();

        let curve_color = RwSignal::new(palette::css::BLUE);
        let axis_color = RwSignal::new(palette::css::GRAY);

        // Visual preview of the easing curve
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let graph_width = width - padding * 2.0;
            let graph_height = height - padding * 2.0;

            // Sample the easing function
            let sample_count = 50;
            let mut path = kurbo::BezPath::new();

            for i in 0..=sample_count {
                let t = i as f64 / sample_count as f64;
                let eased = easing_clone.eval(t);
                let x = padding + t * graph_width;
                let y = padding + (1.0 - eased) * graph_height;

                if i == 0 {
                    path.move_to(Point::new(x, y));
                } else {
                    path.line_to(Point::new(x, y));
                }
            }

            // Draw the curve
            cx.stroke(
                &path,
                curve_color.get(),
                &Stroke {
                    width: 2.0,
                    ..Default::default()
                },
            );

            // Draw axes
            let axis_stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };

            // X axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, height - padding),
                    Point::new(width - padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );

            // Y axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, padding),
                    Point::new(padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| s.border_color(t.border()))
        });

        let tooltip_view = move || {
            let transition = transition.clone();

            let duration_row = super::debug_view_impl::views((
                "Duration:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || format!("{:.0}ms", transition.duration.as_millis())),
            ));

            let easing_name = format!("{:?}", transition.easing);
            let easing_row = super::debug_view_impl::views((
                "Easing:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || easing_name.clone()),
            ));

            // Show velocity at key points if available
            let velocity_samples = if transition.easing.velocity(0.0).is_some() {
                let samples = vec![0.0, 0.25, 0.5, 0.75, 1.0]
                    .into_iter()
                    .filter_map(|t| {
                        transition
                            .easing
                            .velocity(t)
                            .map(|v| Label::new(format!("t={:.2}: {:.3}", t, v)))
                    })
                    .collect::<Vec<_>>();

                if !samples.is_empty() {
                    Some(super::debug_view_impl::views((
                        "Velocity:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                        Stack::vertical_from_iter(samples).style(|s| s.gap(2.0)),
                    )))
                } else {
                    None
                }
            } else {
                None
            };

            let mut rows = vec![duration_row.into_any(), easing_row.into_any()];

            if let Some(velocity_row) = velocity_samples {
                rows.push(velocity_row.into_any());
            }

            Stack::vertical_from_iter(rows).style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(12)
                    .col_gap(10)
                    .padding(20)
            })
        };

        preview
            .tooltip(tooltip_view)
            .style(|s| s.items_center())
            .into_any()
    }
}
