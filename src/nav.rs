use peniko::kurbo::{Point, Rect};
use ui_events::keyboard::NamedKey;

use crate::{id::ViewId, view::view_tab_navigation, window_state::WindowState};

pub(crate) fn view_arrow_navigation(key: NamedKey, window_state: &mut WindowState, view: ViewId) {
    let focused = match window_state.focus {
        Some(id) => id,
        None => {
            view_tab_navigation(
                view,
                window_state,
                matches!(key, NamedKey::ArrowUp | NamedKey::ArrowLeft),
            );
            return;
        }
    };

    // Get the rectangle of the focused element with a small padding
    let rect = focused.layout_rect().inflate(10.0, 10.0);
    let center = rect.center();

    // Create a narrow ray extending from the center in the direction of navigation
    let ray_target = match key {
        NamedKey::ArrowUp => Rect::new(rect.x0, f64::NEG_INFINITY, rect.x1, rect.y0),
        NamedKey::ArrowDown => Rect::new(rect.x0, rect.y1, rect.x1, f64::INFINITY),
        NamedKey::ArrowLeft => Rect::new(f64::NEG_INFINITY, rect.y0, rect.x0, rect.y1),
        NamedKey::ArrowRight => Rect::new(rect.x1, rect.y0, f64::INFINITY, rect.y1),
        _ => panic!("Unexpected key for arrow navigation"),
    };

    // Create a wider area representing the general direction
    let direction_target = match key {
        NamedKey::ArrowUp => {
            Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, rect.y0)
        }
        NamedKey::ArrowDown => Rect::new(f64::NEG_INFINITY, rect.y1, f64::INFINITY, f64::INFINITY),
        NamedKey::ArrowLeft => {
            Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, rect.x0, f64::INFINITY)
        }
        NamedKey::ArrowRight => Rect::new(rect.x1, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY),
        _ => panic!("Unexpected key for arrow navigation"),
    };

    // Collect all focusable elements
    let mut focusable: Vec<ViewId> = window_state.focusable.iter().copied().collect();
    focusable.retain(|id| {
        let layout = id.layout_rect();
        direction_target.contains(layout.center()) && *id != focused
    });

    // Find the best target in a single pass with a priority scoring system
    let mut best_target: Option<(ViewId, f64, bool)> = None; // (id, distance, in_ray)

    for id in focusable {
        let id_rect = id.layout_rect();
        let id_center = id_rect.center();

        // Calculate the edge point of the target element closest to the current element
        let id_edge = match key {
            NamedKey::ArrowUp => Point::new(id_center.x, id_rect.y1),
            NamedKey::ArrowDown => Point::new(id_center.x, id_rect.y0),
            NamedKey::ArrowLeft => Point::new(id_rect.x1, id_center.y),
            NamedKey::ArrowRight => Point::new(id_rect.x0, id_center.y),
            _ => panic!("Unexpected key for arrow navigation"),
        };

        let id_distance = center.distance_squared(id_edge);
        let is_in_ray = !id_rect.intersect(ray_target).is_zero_area();

        // Update best target using the following rules:
        // 1. Always prefer elements in the ray over elements not in the ray
        // 2. Within each category, prefer the closest element
        if let Some((_, current_distance, current_in_ray)) = best_target {
            if is_in_ray && !current_in_ray {
                // This element is in the ray but current best isn't, so this is better
                best_target = Some((id, id_distance, is_in_ray));
            } else if is_in_ray == current_in_ray && id_distance < current_distance {
                // Both elements are in the same category (ray or not), pick the closer one
                best_target = Some((id, id_distance, is_in_ray));
            }
        } else {
            // First valid target found
            best_target = Some((id, id_distance, is_in_ray));
        }
    }

    // Update focus to the best target if found
    if let Some((id, _, _)) = best_target {
        window_state.clear_focus();
        window_state.update_focus(id, true);
    }
}
