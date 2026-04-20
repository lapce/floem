//! Standalone integration test exercising `floem_style` through its public
//! API surface — as an external consumer (e.g. `floem-native`) would.
//!
//! Everything here is reachable via `floem_style::...` paths only, with no
//! dependency on the `floem` crate. If this file compiles and passes, the
//! extracted engine can be driven by a non-floem host.

use floem_style::builtin_props::{Background, Disabled, FontSize, TextColor, Width};
use floem_style::props::StyleClass;
use floem_style::responsive::ScreenSizeBp;
use floem_style::selectors::{StyleSelector, StyleSelectors};
use floem_style::style_class;
use floem_style::{
    resolve_nested_maps, InteractionState, InheritedInteractionCx, Style, StyleCache,
    StyleCacheKey,
};
use peniko::color::palette::css;

fn empty_state() -> InteractionState {
    InteractionState::default()
}

fn xs() -> ScreenSizeBp {
    ScreenSizeBp::Xs
}

#[test]
fn resolves_plain_style_untouched() {
    let style = Style::new().background(css::RED).width(100.0);
    let mut state = empty_state();
    let (resolved, selectors) = resolve_nested_maps(
        style,
        &mut state,
        xs(),
        &[],
        &Style::new(),
        &Style::new(),
    );
    assert_eq!(resolved.get(Background), Some(css::RED.into()));
    // Width uses LengthAuto; compare the variant.
    assert!(matches!(
        resolved.get(Width),
        floem_style::unit::LengthAuto::Pt(v) if v == 100.0
    ));
    assert_eq!(selectors, StyleSelectors::empty());
}

#[test]
fn hover_selector_activates_only_when_hovered() {
    let style = Style::new()
        .background(css::RED)
        .hover(|s| s.background(css::BLUE));

    let mut state = empty_state();
    let (resolved, _) = resolve_nested_maps(
        style.clone(),
        &mut state,
        xs(),
        &[],
        &Style::new(),
        &Style::new(),
    );
    assert_eq!(resolved.get(Background), Some(css::RED.into()));

    let mut state = InteractionState {
        is_hovered: true,
        ..Default::default()
    };
    let (resolved, selectors) = resolve_nested_maps(
        style,
        &mut state,
        xs(),
        &[],
        &Style::new(),
        &Style::new(),
    );
    assert_eq!(resolved.get(Background), Some(css::BLUE.into()));
    assert!(selectors.has(StyleSelector::Hover));
}

#[test]
fn inherited_props_flow_through_context() {
    let inherited = Style::new()
        .set(TextColor, Some(css::RED))
        .set(FontSize, 18.0);
    let style = Style::new().background(css::BLUE);
    let mut state = empty_state();
    let (resolved, _) = resolve_nested_maps(
        style,
        &mut state,
        xs(),
        &[],
        &inherited,
        &Style::new(),
    );
    assert_eq!(resolved.get(TextColor), Some(css::RED));
    assert_eq!(resolved.get(FontSize), 18.0);
    assert_eq!(resolved.get(Background), Some(css::BLUE.into()));
}

#[test]
fn classes_apply_from_class_context() {
    style_class!(pub Button);

    let class_ctx = Style::new().class(Button, |s| {
        s.background(css::GREEN)
            .hover(|s| s.background(css::BLUE))
    });

    let base = Style::new();
    let mut state = InteractionState {
        is_hovered: true,
        ..Default::default()
    };
    let (resolved, selectors) = resolve_nested_maps(
        base,
        &mut state,
        xs(),
        &[Button::class_ref()],
        &Style::new(),
        &class_ctx,
    );
    assert_eq!(resolved.get(Background), Some(css::BLUE.into()));
    assert!(selectors.has(StyleSelector::Hover));
}

#[test]
fn disabled_short_circuits_other_selectors() {
    let style = Style::new()
        .background(css::RED)
        .hover(|s| s.background(css::BLUE))
        .disabled(|s| s.background(css::GRAY));

    // Hovered AND disabled → cascade enters the disabled branch, skipping hover.
    let mut state = InteractionState {
        is_hovered: true,
        is_disabled: true,
        ..Default::default()
    };
    let (resolved, _) = resolve_nested_maps(
        style,
        &mut state,
        xs(),
        &[],
        &Style::new(),
        &Style::new(),
    );
    assert_eq!(resolved.get(Background), Some(css::GRAY.into()));
}

#[test]
fn disabled_as_prop_also_triggers_disabled_selector() {
    // Even without interact_state.is_disabled, setting Disabled=true in the
    // style itself opens the :disabled branch.
    let style = Style::new()
        .set(Disabled, true)
        .background(css::RED)
        .disabled(|s| s.background(css::GRAY));
    let mut state = empty_state();
    let (resolved, _) = resolve_nested_maps(
        style,
        &mut state,
        xs(),
        &[],
        &Style::new(),
        &Style::new(),
    );
    assert_eq!(resolved.get(Background), Some(css::GRAY.into()));
}

#[test]
fn style_cache_hits_on_matching_inputs() {
    let mut cache = StyleCache::new();
    let style = Style::new().background(css::RED);
    let parent = Style::new();

    let key = StyleCacheKey::new(&style, &empty_state(), xs(), &[], &Style::new());

    // Miss first.
    assert!(cache.get(&key, &parent).is_none());
    assert_eq!(cache.stats().misses, 1);

    // Insert, then hit.
    let post = InheritedInteractionCx::default();
    cache.insert(key.clone(), &style, None, post, &parent);
    assert!(cache.get(&key, &parent).is_some());
    assert_eq!(cache.stats().hits, 1);
}

#[test]
fn style_content_hash_is_deterministic_and_order_independent() {
    let a = Style::new().background(css::RED).set(FontSize, 16.0);
    let b = Style::new().set(FontSize, 16.0).background(css::RED);
    // Insertion order doesn't matter.
    assert_eq!(a.content_hash(), b.content_hash());

    // And the hash is stable across calls.
    let h1 = a.content_hash();
    let h2 = a.content_hash();
    assert_eq!(h1, h2);
}

#[test]
fn inherited_equal_distinguishes_inherited_values() {
    let a = Style::new().set(TextColor, Some(css::RED));
    let b = Style::new().set(TextColor, Some(css::BLUE));
    let c = Style::new().set(TextColor, Some(css::RED));
    assert!(!a.inherited_equal(&b));
    assert!(a.inherited_equal(&c));
}

#[test]
fn inherited_equal_ignores_non_inherited_props() {
    // `Background` is NOT inherited, so it should not affect inherited_equal.
    let a = Style::new().background(css::RED);
    let b = Style::new().background(css::BLUE);
    assert!(a.inherited_equal(&b));
}
