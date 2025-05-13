use peniko::kurbo::{Point, Rect};
use ui_events::keyboard::NamedKey;

use crate::{app_state::AppState, id::ViewId, view::view_tab_navigation};

pub(crate) fn view_arrow_navigation(key: NamedKey, app_state: &mut AppState, view: ViewId) {
    let focused = match app_state.focus {
        Some(id) => id,
        None => {
            view_tab_navigation(
                view,
                app_state,
                matches!(key, NamedKey::ArrowUp | NamedKey::ArrowLeft),
            );
            return;
        }
    };
    let rect = focused.layout_rect().inflate(10.0, 10.0);
    let center = rect.center();
    let intersect_target = match key {
        NamedKey::ArrowUp => Rect::new(rect.x0, f64::NEG_INFINITY, rect.x1, center.y),
        NamedKey::ArrowDown => Rect::new(rect.x0, center.y, rect.x1, f64::INFINITY),
        NamedKey::ArrowLeft => Rect::new(f64::NEG_INFINITY, rect.y0, center.x, rect.y1),
        NamedKey::ArrowRight => Rect::new(center.x, rect.y0, f64::INFINITY, rect.y1),
        _ => panic!(),
    };
    let center_target = match key {
        NamedKey::ArrowUp => {
            Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, rect.y0)
        }
        NamedKey::ArrowDown => Rect::new(f64::NEG_INFINITY, rect.y1, f64::INFINITY, f64::INFINITY),
        NamedKey::ArrowLeft => {
            Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, rect.x0, f64::INFINITY)
        }
        NamedKey::ArrowRight => Rect::new(rect.x1, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY),
        _ => panic!(),
    };
    let mut keyboard_navigable: Vec<ViewId> =
        app_state.keyboard_navigable.iter().copied().collect();
    keyboard_navigable.retain(|id| {
        let layout = id.layout_rect();

        !layout.intersect(intersect_target).is_zero_area()
            && center_target.contains(layout.center())
            && app_state.can_focus(*id)
            && *id != focused
    });

    let mut new_focus = None;
    for id in keyboard_navigable {
        let id_rect = id.layout_rect();
        let id_center = id_rect.center();
        let id_edge = match key {
            NamedKey::ArrowUp => Point::new(id_center.x, id_rect.y1),
            NamedKey::ArrowDown => Point::new(id_center.x, id_rect.y0),
            NamedKey::ArrowLeft => Point::new(id_rect.x1, id_center.y),
            NamedKey::ArrowRight => Point::new(id_rect.x0, id_center.y),
            _ => panic!(),
        };
        let id_distance = center.distance_squared(id_edge);
        if let Some((_, distance)) = new_focus {
            if id_distance < distance {
                new_focus = Some((id, id_distance));
            }
        } else {
            new_focus = Some((id, id_distance));
        }
    }

    if let Some((id, _)) = new_focus {
        app_state.clear_focus();
        app_state.update_focus(id, true);
    }
}
