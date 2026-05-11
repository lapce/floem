//! Multi-frame render-loop integration test.
//!
//! The existing `host_integration.rs` exercises each public surface in
//! isolation. This file drives the engine the way a real host would:
//! a frame loop that builds a fresh [`CascadeInputs`] each pass, runs
//! [`StyleTree::compute_style`], drains the engine's post-cascade
//! outputs, advances time, mutates host state, and runs the next
//! frame. If the engine holds up under multi-frame driving from a
//! non-floem host — scheduling correctly, reacting to interaction
//! changes, ticking animations across passes, picking up structural
//! edits — the extraction is usable by a second consumer.
//!
//! Nothing here touches floem. Everything used is reachable via
//! `floem_style::...` plus generic support crates. If this compiles
//! and passes, a `floem-native`-style host can drive the engine.

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use std::collections::HashMap;

use floem_style::animation::{Animation, KeyFrame};
use floem_style::builtin_props::{Background, FontSize};
use floem_style::recalc::StyleReason;
use floem_style::responsive::{ScreenSize, ScreenSizeBp};
use floem_style::{
    CascadeInputs, NoAnimationBackend, PerNodeInteraction, Style, StyleNodeId, StyleTree,
};
use peniko::color::palette::css;

// ─────────────────────────────────────────────────────────────────────────
// MiniHost — the second-host reference driver. A real host (floem,
// floem-native) layers reactive state, taffy layout, paint, input on
// top; everything that matters for *driving the style engine* is here.
// ─────────────────────────────────────────────────────────────────────────

slotmap::new_key_type! { struct NodeKey; }

#[derive(Default)]
struct NodeData {
    style_node: Option<StyleNodeId>,
    children: Vec<NodeKey>,
    // Host-owned interaction facts the cascade reads through
    // CascadeInputs::interactions.
    hovered: bool,
    focused: bool,
    pressed: bool,
}

struct MiniHost {
    tree: StyleTree,
    nodes: slotmap::SlotMap<NodeKey, NodeData>,
    style_node_to_key: HashMap<StyleNodeId, NodeKey>,

    default_inherited: Style,
    default_classes: Style,
    screen_size_bp: ScreenSizeBp,
    is_dark_mode: bool,
    root_size_width: f64,
    keyboard_navigation: bool,

    frame_time: Instant,
    frame_count: u64,
}

impl MiniHost {
    fn new() -> Self {
        Self {
            tree: StyleTree::new(),
            nodes: slotmap::SlotMap::with_key(),
            style_node_to_key: HashMap::new(),
            default_inherited: Style::new(),
            default_classes: Style::new(),
            screen_size_bp: ScreenSizeBp::Md,
            is_dark_mode: false,
            root_size_width: 1024.0,
            keyboard_navigation: false,
            frame_time: Instant::now(),
            frame_count: 0,
        }
    }

    /// Allocate a host node (and its companion `StyleNodeId`) and wire
    /// it into the parent's child list + the style tree's edges.
    fn add_node(&mut self, parent: Option<NodeKey>) -> NodeKey {
        let key = self.nodes.insert(NodeData::default());
        let style_node = self.tree.new_node();
        self.nodes[key].style_node = Some(style_node);
        self.style_node_to_key.insert(style_node, key);

        if let Some(p) = parent {
            self.nodes[p].children.push(key);
            let parent_style_node = self.nodes[p].style_node;
            self.tree.set_parent(style_node, parent_style_node);
        }
        key
    }

    fn style_node(&self, key: NodeKey) -> StyleNodeId {
        self.nodes[key]
            .style_node
            .expect("host node missing style node")
    }

    fn set_direct_style(&mut self, key: NodeKey, style: Style) {
        let id = self.style_node(key);
        self.tree.set_direct_style(id, style);
    }

    fn mark_dirty(&mut self, key: NodeKey, reason: StyleReason) {
        let id = self.style_node(key);
        self.tree.mark_dirty(id, reason);
    }

    /// Run one frame: build inputs, cascade from `root`, drain
    /// post-cascade outputs. Returns the `StyleNodeId`s that the
    /// engine scheduled for re-cascade (transitions, animations,
    /// etc.) so tests can assert on scheduling behavior.
    fn run_frame(&mut self, root: NodeKey) -> Vec<StyleNodeId> {
        self.frame_count += 1;
        let root_node = self.style_node(root);

        let reverse = &self.style_node_to_key;
        let nodes = &self.nodes;
        let interactions = |node: StyleNodeId| -> PerNodeInteraction {
            let Some(&k) = reverse.get(&node) else {
                return PerNodeInteraction::default();
            };
            let d = &nodes[k];
            PerNodeInteraction {
                is_hovered: d.hovered,
                is_focused: d.focused,
                is_active: d.pressed,
                ..Default::default()
            }
        };

        let anim = NoAnimationBackend;
        let inputs = CascadeInputs {
            frame_start: self.frame_time,
            screen_size_bp: self.screen_size_bp,
            keyboard_navigation: self.keyboard_navigation,
            root_size_width: self.root_size_width,
            is_dark_mode: self.is_dark_mode,
            default_theme_classes: &self.default_classes,
            default_theme_inherited: &self.default_inherited,
            interactions: &interactions,
            animations: &anim,
        };
        self.tree.compute_style(root_node, &inputs);
        drop(inputs);

        // Drain what the engine wants re-run next frame.
        let scheduled: Vec<_> = self.tree.take_scheduled().map(|(id, _)| id).collect();
        // Drop dirtied-this-pass (host would use these to invalidate
        // downstream caches; MiniHost has none).
        let _: Vec<_> = self.tree.take_dirtied_this_pass().collect();
        scheduled
    }

    fn advance_time(&mut self, delta: Duration) {
        self.frame_time += delta;
    }

    fn computed_background(&self, key: NodeKey) -> Option<peniko::Brush> {
        self.tree.computed_style(self.style_node(key))?.get(Background)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests — each drives multiple frames.
// ─────────────────────────────────────────────────────────────────────────

/// A steady-state loop (no host changes between frames) should settle:
/// after the initial cascade, the second pass shouldn't schedule any
/// work the engine isn't required to repeat.
#[test]
fn steady_state_loop_settles_after_initial_cascade() {
    let mut host = MiniHost::new();
    let root = host.add_node(None);
    let child = host.add_node(Some(root));
    host.set_direct_style(root, Style::new().font_size(14.0).background(css::RED));
    host.set_direct_style(child, Style::new());

    let _ = host.run_frame(root);
    host.advance_time(Duration::from_millis(16));
    let scheduled = host.run_frame(root);

    assert!(
        scheduled.is_empty(),
        "steady-state frame should not schedule any follow-up work, got {:?}",
        scheduled
    );
    // Inheritance still holds after the second pass.
    assert_eq!(
        host.tree
            .computed_style(host.style_node(child))
            .unwrap()
            .get(FontSize),
        14.0
    );
}

/// Host flips `hovered` between frames; the interactions closure sees
/// fresh state on the second pass and the cascade re-resolves the
/// `:hover` branch accordingly. Siblings stay unaffected.
#[test]
fn hover_change_across_frames_restyles_affected_node_only() {
    let mut host = MiniHost::new();
    let root = host.add_node(None);
    let a = host.add_node(Some(root));
    let b = host.add_node(Some(root));

    host.set_direct_style(root, Style::new());
    let hover_style = Style::new()
        .background(css::RED)
        .hover(|s| s.background(css::BLUE));
    host.set_direct_style(a, hover_style.clone());
    host.set_direct_style(b, hover_style);

    let _ = host.run_frame(root);
    assert_eq!(host.computed_background(a), Some(css::RED.into()));
    assert_eq!(host.computed_background(b), Some(css::RED.into()));

    // Host flips hover on `a`, marks it dirty, runs frame 2.
    host.nodes[a].hovered = true;
    host.mark_dirty(a, StyleReason::style_pass());
    host.advance_time(Duration::from_millis(16));
    host.run_frame(root);

    assert_eq!(host.computed_background(a), Some(css::BLUE.into()));
    assert_eq!(
        host.computed_background(b),
        Some(css::RED.into()),
        "sibling should not have restyled",
    );
}

/// Push a tree-native animation, drive several frames, confirm the
/// engine fires a `started` event on the first tick and schedules
/// itself for re-cascade while active. The animation stays
/// `is_in_progress` across passes.
#[test]
fn tree_animation_across_frames_schedules_and_progresses() {
    let mut host = MiniHost::new();
    let root = host.add_node(None);
    host.set_direct_style(root, Style::new());

    let anim = Animation::new()
        .duration(Duration::from_millis(500))
        .keyframe(0, |f: KeyFrame| {
            f.ease_linear().style(|s| s.background(css::RED))
        })
        .keyframe(100, |f: KeyFrame| {
            f.ease_linear().style(|s| s.background(css::BLUE))
        });
    let slot = host.tree.push_animation(host.style_node(root), anim);

    // Frame 1: Idle → PassInProgress, `started` event fires, cascade
    // schedules the node for the next frame.
    let scheduled = host.run_frame(root);
    let root_node = host.style_node(root);
    let events = host.tree.take_animation_events(root_node);
    assert!(events.iter().any(|(s, ev)| *s == slot && ev.started));
    assert!(
        scheduled.contains(&root_node),
        "active animation must schedule its node for the next frame"
    );
    assert!(host.tree.animations(root_node)[slot].is_in_progress());

    // Frames 2-3: animation stays in progress, engine keeps
    // scheduling while it's active.
    for _ in 0..2 {
        host.advance_time(Duration::from_millis(50));
        host.mark_dirty(root, StyleReason::animation());
        let scheduled = host.run_frame(root);
        assert!(
            scheduled.contains(&root_node),
            "animation in progress should keep scheduling"
        );
        assert!(host.tree.animations(root_node)[slot].is_in_progress());
    }
}

/// Add a child mid-loop. After marking the parent dirty and running
/// the next frame, the new child is cascaded and picks up the
/// inherited `font-size` from its parent.
#[test]
fn adding_child_mid_loop_propagates_inheritance() {
    let mut host = MiniHost::new();
    let root = host.add_node(None);
    host.set_direct_style(root, Style::new().font_size(22.0));

    let _ = host.run_frame(root);

    // Host mutates the tree: new child, wire up edges, run next frame.
    let child = host.add_node(Some(root));
    host.set_direct_style(child, Style::new());
    host.mark_dirty(root, StyleReason::style_pass());

    host.advance_time(Duration::from_millis(16));
    host.run_frame(root);

    assert_eq!(
        host.tree
            .computed_style(host.style_node(child))
            .unwrap()
            .get(FontSize),
        22.0,
        "late-added child should inherit parent font-size on next cascade"
    );
}

/// Flip the window width between frames: a responsive rule attached
/// to `ScreenSize::LG` matches on `window_width` (via
/// `GridBreakpoints::default().get_width_bp(width)`), so shrinking
/// the host-reported width to an Xs range inactivates it, and
/// restoring it to LG range reactivates it.
#[test]
fn responsive_width_flip_resolves_different_branch() {
    let mut host = MiniHost::new();
    // Start in Xs range (default grid: xs is < 576px).
    host.root_size_width = 400.0;

    let root = host.add_node(None);
    host.set_direct_style(
        root,
        Style::new()
            .background(css::RED)
            .responsive(ScreenSize::LG, |s| s.background(css::GREEN)),
    );

    let _ = host.run_frame(root);
    assert_eq!(host.computed_background(root), Some(css::RED.into()));

    // Widen into LG range (992..1200). Responsive branch activates.
    host.root_size_width = 1100.0;
    host.mark_dirty(root, StyleReason::style_pass());
    host.advance_time(Duration::from_millis(16));
    host.run_frame(root);

    assert_eq!(
        host.computed_background(root),
        Some(css::GREEN.into()),
        "responsive LG branch should activate once window_width is in the LG range"
    );
}

/// Removing a node mid-loop: the engine should drop its node from the
/// slotmap and no longer report a computed style for it.
#[test]
fn removing_node_mid_loop_drops_it_from_the_engine() {
    let mut host = MiniHost::new();
    let root = host.add_node(None);
    let child = host.add_node(Some(root));
    host.set_direct_style(root, Style::new().font_size(14.0));
    host.set_direct_style(child, Style::new());

    let _ = host.run_frame(root);
    assert!(host.tree.computed_style(host.style_node(child)).is_some());

    // Host removes the child: pull it from node tables + drop the
    // style-tree node.
    let child_style_node = host.style_node(child);
    host.nodes[root].children.retain(|&c| c != child);
    host.nodes.remove(child);
    host.style_node_to_key.remove(&child_style_node);
    host.tree.remove_node(child_style_node);

    host.advance_time(Duration::from_millis(16));
    host.run_frame(root);

    assert!(
        host.tree.computed_style(child_style_node).is_none(),
        "removed node should no longer resolve through the tree"
    );
    assert!(
        !host.tree.contains(child_style_node),
        "engine should report the removed node as gone"
    );
}
