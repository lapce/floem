//! Linux/FreeBSD context menu view implementation.
//!
//! This module provides a fallback context menu UI for platforms where
//! native context menus aren't fully supported by the muda crate.

use floem_reactive::{Effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use peniko::Color;
use peniko::color::palette;
use peniko::kurbo::{Point, Size};
use ui_events::keyboard::{Key, KeyboardEvent, NamedKey};

use crate::event::{Event, EventListener};
use crate::platform::menu::MudaMenu;
use crate::style::CursorStyle;

use crate::platform::menu_types;
use crate::unit::UnitExt;
use crate::view::{IntoView, View};
use crate::views::{Container, Decorators, Label, Stack, svg};

#[derive(Clone, PartialEq, Eq, Hash)]
enum MenuDisplay {
    Separator(usize),
    Item {
        id: Option<String>,
        enabled: bool,
        title: String,
        children: Option<Vec<MenuDisplay>>,
    },
}

fn format_menu(menu: &MudaMenu) -> Vec<MenuDisplay> {
    menu.items()
        .iter()
        .enumerate()
        .map(|(s, item)| match item {
            menu_types::MenuItemKind::MenuItem(menu_item) => MenuDisplay::Item {
                id: Some(menu_item.id().as_ref().to_string()),
                enabled: menu_item.is_enabled(),
                title: menu_item.text().to_string(),
                children: None,
            },
            menu_types::MenuItemKind::Submenu(submenu) => MenuDisplay::Item {
                id: None,
                enabled: submenu.is_enabled(),
                title: submenu.text().to_string(),
                children: Some(format_submenu(submenu)),
            },
            menu_types::MenuItemKind::Predefined(_) => MenuDisplay::Separator(s),
            menu_types::MenuItemKind::Check(check_item) => MenuDisplay::Item {
                id: Some(check_item.id().as_ref().to_string()),
                enabled: check_item.is_enabled(),
                title: check_item.text().to_string(),
                children: None,
            },
            menu_types::MenuItemKind::Icon(icon_item) => MenuDisplay::Item {
                id: Some(icon_item.id().as_ref().to_string()),
                enabled: icon_item.is_enabled(),
                title: icon_item.text().to_string(),
                children: None,
            },
        })
        .collect()
}

fn format_submenu(submenu: &menu_types::Submenu) -> Vec<MenuDisplay> {
    submenu
        .items()
        .iter()
        .enumerate()
        .map(|(s, item)| match item {
            menu_types::MenuItemKind::MenuItem(menu_item) => MenuDisplay::Item {
                id: Some(menu_item.id().as_ref().to_string()),
                enabled: menu_item.is_enabled(),
                title: menu_item.text().to_string(),
                children: None,
            },
            menu_types::MenuItemKind::Submenu(nested_submenu) => MenuDisplay::Item {
                id: None,
                enabled: nested_submenu.is_enabled(),
                title: nested_submenu.text().to_string(),
                children: Some(format_submenu(nested_submenu)),
            },
            menu_types::MenuItemKind::Predefined(_) => MenuDisplay::Separator(s),
            menu_types::MenuItemKind::Check(check_item) => MenuDisplay::Item {
                id: Some(check_item.id().as_ref().to_string()),
                enabled: check_item.is_enabled(),
                title: check_item.text().to_string(),
                children: None,
            },
            menu_types::MenuItemKind::Icon(icon_item) => MenuDisplay::Item {
                id: Some(icon_item.id().as_ref().to_string()),
                enabled: icon_item.is_enabled(),
                title: icon_item.text().to_string(),
                children: None,
            },
        })
        .collect()
}

pub(crate) fn context_menu_view(
    cx: Scope,
    context_menu: RwSignal<Option<(menu_types::Menu, Point, bool)>>,
    window_size: RwSignal<Size>,
) -> impl IntoView {
    use crate::{
        app::{AppUpdateEvent, add_app_update_event},
        views::dyn_stack,
    };

    let context_menu_items = cx.create_memo(move |_| {
        context_menu.with(|menu| {
            menu.as_ref()
                .map(|(menu, _, _): &(MudaMenu, Point, bool)| format_menu(menu))
        })
    });
    let context_menu_size = cx.create_rw_signal(Size::ZERO);

    fn view_fn(
        menu: MenuDisplay,
        context_menu: RwSignal<Option<(MudaMenu, Point, bool)>>,
        on_child_submenu_for_parent: RwSignal<bool>,
    ) -> impl IntoView {
        match menu {
            MenuDisplay::Item {
                id,
                enabled,
                title,
                children,
            } => {
                let menu_width = RwSignal::new(0.0);
                let show_submenu = RwSignal::new(false);
                let on_submenu = RwSignal::new(false);
                let on_child_submenu = RwSignal::new(false);
                let has_submenu = children.is_some();
                let submenu_svg = r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M10.072 8.024L5.715 3.667l.618-.62L11 7.716v.618L6.333 13l-.618-.619 4.357-4.357z"/></svg>"#;
                Container::new(
                    Stack::new((
                        Stack::new((
                            Label::new(title).style(|s| s.selectable(false)),
                            svg(submenu_svg).style(move |s| {
                                s.size(20.0, 20.0)
                                    .color(Color::from_rgb8(201, 201, 201))
                                    .margin_right(10.0)
                                    .margin_left(20.0)
                                    .apply_if(!has_submenu, |s| s.hide())
                            }),
                        ))
                        .on_event_stop(EventListener::PointerEnter, move |_| {
                            if has_submenu {
                                show_submenu.set(true);
                            }
                        })
                        .on_event_stop(EventListener::PointerLeave, move |_| {
                            if has_submenu {
                                show_submenu.set(false);
                            }
                        })
                        .on_resize(move |rect| {
                            let width = rect.width();
                            if menu_width.get_untracked() != width {
                                menu_width.set(width);
                            }
                        })
                        .on_event_stop(EventListener::PointerDown, move |_| {
                            context_menu.update(|context_menu| {
                                if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                                    *had_pointer_down = true;
                                }
                            });
                        })
                        .on_event_stop(EventListener::PointerUp, move |_| {
                            if has_submenu {
                                // don't handle the click if there's submenu
                                return;
                            }
                            context_menu.set(None);
                            if let Some(id) = id.clone() {
                                add_app_update_event(AppUpdateEvent::MenuAction {
                                    action_id: id.into(),
                                });
                            }
                        })
                        .style(move |s| {
                            s.width(100.pct())
                                .min_width(100.pct())
                                .padding_horiz(20.0)
                                .justify_between()
                                .items_center()
                                .hover(|s| {
                                    s.border_radius(10.0)
                                        .background(Color::from_rgb8(65, 65, 65))
                                })
                                .active(|s| {
                                    s.border_radius(10.0)
                                        .background(Color::from_rgb8(92, 92, 92))
                                })
                                .set_disabled(!enabled)
                                .disabled(|s| s.color(Color::from_rgb8(92, 92, 92)))
                        }),
                        dyn_stack(
                            move || children.clone().unwrap_or_default(),
                            move |s| s.clone(),
                            move |menu| view_fn(menu, context_menu, on_child_submenu),
                        )
                        .on_event_stop(EventListener::KeyDown, move |event| {
                            if let Event::Key(KeyboardEvent { key, .. }) = event
                                && *key == Key::Named(NamedKey::Escape)
                            {
                                context_menu.set(None);
                            }
                        })
                        .on_event_stop(EventListener::PointerEnter, move |_| {
                            if has_submenu {
                                on_submenu.set(true);
                                on_child_submenu_for_parent.set(true);
                            }
                        })
                        .on_event_stop(EventListener::PointerLeave, move |_| {
                            if has_submenu {
                                on_submenu.set(false);
                                on_child_submenu_for_parent.set(false);
                            }
                        })
                        .style(move |s| {
                            s.absolute()
                                .focusable(true)
                                .min_width(200.0)
                                .margin_top(-5.0)
                                .margin_left(menu_width.get() as f32)
                                .flex_col()
                                .border_radius(10.0)
                                .background(Color::from_rgb8(44, 44, 44))
                                .padding(5.0)
                                .cursor(CursorStyle::Default)
                                .box_shadow_blur(5.0)
                                .box_shadow_color(palette::css::BLACK)
                                .apply_if(
                                    !show_submenu.get()
                                        && !on_submenu.get()
                                        && !on_child_submenu.get(),
                                    |s| s.hide(),
                                )
                        }),
                    ))
                    .style(|s| s.min_width(100.pct())),
                )
                .style(|s| s.min_width(100.pct()))
                .into_any()
            }

            MenuDisplay::Separator(_) => Container::new(().style(|s| {
                s.width(100.pct())
                    .height(1.0)
                    .margin_vert(5.0)
                    .background(Color::from_rgb8(92, 92, 92))
            }))
            .style(|s| s.min_width(100.pct()).padding_horiz(20.0))
            .into_any(),
        }
    }

    let on_child_submenu = RwSignal::new(false);
    let view = dyn_stack(
        move || context_menu_items.get().unwrap_or_default(),
        move |s| s.clone(),
        move |menu| view_fn(menu, context_menu, on_child_submenu),
    )
    .on_resize(move |rect| {
        context_menu_size.set(rect.size());
    })
    .on_event_stop(EventListener::PointerDown, move |_| {
        context_menu.update(|context_menu| {
            if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                *had_pointer_down = true;
            }
        });
    })
    .on_event_stop(EventListener::PointerUp, move |_| {
        context_menu.update(|context_menu| {
            if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                *had_pointer_down = false;
            }
        });
    })
    .on_event_stop(EventListener::PointerMove, move |_| {})
    .on_event_stop(EventListener::KeyDown, move |event| {
        if let Event::Key(KeyboardEvent { key, .. }) = event
            && *key == Key::Named(NamedKey::Escape)
        {
            context_menu.set(None);
        }
    })
    .style(move |s| {
        let window_size = window_size.get();
        let menu_size = context_menu_size.get();
        let is_active = context_menu.with(|m| m.is_some());
        let mut pos = context_menu.with(|m| m.as_ref().map(|(_, pos, _)| *pos).unwrap_or_default());
        if pos.x + menu_size.width > window_size.width {
            pos.x = window_size.width - menu_size.width;
        }
        if pos.y + menu_size.height > window_size.height {
            pos.y = window_size.height - menu_size.height;
        }
        s.absolute()
            .min_width(200.0)
            .flex_col()
            .border_radius(10.0)
            .focusable(true)
            .background(Color::from_rgb8(44, 44, 44))
            .color(Color::from_rgb8(201, 201, 201))
            .z_index(999)
            .line_height(2.0)
            .padding(5.0)
            .margin_left(pos.x as f32)
            .margin_top(pos.y as f32)
            .cursor(CursorStyle::Default)
            .apply_if(!is_active, |s| s.hide())
            .box_shadow_blur(5.0)
            .box_shadow_color(palette::css::BLACK)
    });

    let id = view.id();

    Effect::new(move |_| {
        if context_menu.with(|m| m.is_some()) {
            id.request_focus();
        }
    });

    view
}
