# Event Handling Requirements

This document specifies the complete requirements for event handling in Floem. These requirements define what must be implemented to maintain 100% compatibility when replacing the current event system implementation.

## Architecture Overview

The event system must support a view tree where events flow from the operating system through the window to individual views. Events must be properly transformed, routed, and handled according to web standards (W3C DOM Events, Pointer Events) and modern UI framework patterns (similar to React, Flutter, and Chromium).

## Core Event Types

### Event Classification
- **Pointer Events**: Mouse, touch, and stylus input (`PointerEvent`)
- **Keyboard Events**: Key presses and releases (`KeyboardEvent`)
- **Window Events**: Resize, focus, theme changes, etc.
- **File Drag Events**: File drop operations (`FileDragEvent`)
- **Focus Events**: Focus gained/lost for individual views
- **Capture Events**: Pointer capture gained/lost

### Event Propagation Control
The system must support `EventPropagation` enum with:
- `Stop`: Halt event bubbling immediately (event fully processed)
- `Continue`: Allow event to continue bubbling to ancestors

## Event Flow Requirements

### 1. Hit Testing and Target Selection

**Z-Index Aware Hit Testing**:
- Hit test must consider view stacking order (highest z-index first)
- Overlays must be tested before regular view tree
- Children are bounded by their parent's clip rectangle
- Hidden (`is_hidden()`) and disabled (`is_disabled()`) views must be skipped
- Views with `pointer-events: none` style must be skipped for pointer events but still checked for children

**Hit Test Caching**:
- Implement 2-entry cache for repeated hit tests at same coordinates
- Cache must be invalidated on layout changes or view tree modifications
- Cache key: `(root_id, point_coordinates_exact_bits)`

**Target Selection Priority**:
1. Check overlays in reverse z-order (highest first)
2. Recursively check view tree in reverse stacking context order
3. Return deepest valid target (leaf node in successful path)

### 2. Event Path Construction

**Path Building**:
- Construct path from hit target to root view
- Path represents DOM-like ancestry chain
- Pre-compute per-node data to avoid RefCell borrowing during dispatch:
  - View transform matrices
  - Event listener presence
  - Style properties (focusable, pointer-events, etc.)
  - Menu presence (context, popout)
  - Drag capability

**Path Immutability**:
- Path must remain immutable during dispatch
- View tree modifications during dispatch must not affect current event path
- Coordinate transformations must be pre-computed

### 3. Two-Phase Event Dispatch

**Phase 1: Capturing (Root → Target)**:
- Call `event_before_children()` on each view from root toward target
- Apply coordinate transformations (absolute → local coordinates)
- Stop if any view returns `EventPropagation::Stop`
- Handle focus changes if view processes pointer down event

**Phase 2: Bubbling (Target → Root)**:
- Call `event_after_children()` on each view from target toward root
- Process built-in behaviors for ALL nodes in path (not just target)
- Call registered event listeners if not disabled
- Stop if any handler returns `EventPropagation::Stop`
- Apply coordinate transformations (absolute → local coordinates)

### 4. Coordinate Transformation

**Window Scale Factor**:
- All incoming events must be scaled by window scale factor
- Scaling applied once at entry point, not per-view

**View-Local Coordinates**:
- Each view receives events in its local coordinate space
- Apply inverse of view's `visual_transform` matrix
- Transform point coordinates for pointer events, file drag events

**Absolute Coordinates**:
- Hit testing operates in window-absolute coordinates
- Layout rectangles and clip rectangles in absolute coordinates
- Transform to local coordinates only for view event handlers

## Event-Specific Requirements

### Pointer Events

**Pointer Down Behavior**:
- Track clicking state (`window_state.clicking.insert(view_id)`)
- Update focus for focusable views within bounds
- Initialize drag tracking for draggable views
- Store `last_pointer_down` state for click detection
- Show popout menus (platform-specific timing)

**Pointer Move Behavior**:
- Clear previous hover state, rebuild from scratch
- Track hover state (`window_state.hovered`)
- Update cursor from view styles
- Handle drag state transitions (start dragging when moved > 1px)
- Track drag-over state for drop targets

**Pointer Up Behavior**:
- Clear drag state and trigger drag end events
- Show context menus for secondary button
- Handle drop events for drag operations
- **Important**: Click events are dispatched AFTER PointerUp completes

**Click Event Dispatch**:
- Click events are synthetic events generated after PointerUp
- Must bubble through entire path (target → root)
- Requires both pointer down and up on same view
- Double-click requires `state.count == 2`
- Secondary click for right mouse button

**Hover State Management**:
- Send `PointerEnter` to newly hovered views
- Send `PointerLeave` to no longer hovered views
- Optimize: only update styles for views with `:hover` selectors
- Handle both normal hover and drag-over states separately

### Pointer Capture

**W3C Pointer Events Compliance**:
- Support `set_pointer_capture(pointer_id, view_id)` API
- Support `release_pointer_capture(pointer_id)` API
- Implement two-phase capture model (pending → active)
- Fire `GotPointerCapture` and `LostPointerCapture` events

**Capture Processing Order**:
1. Process pending captures before dispatching pointer events
2. Fire `LostPointerCapture` to old target (if any)
3. Fire `GotPointerCapture` to new target (if any)
4. Route subsequent events to capture target

**Implicit Touch Capture**:
- Touch pointers automatically capture on PointerDown
- Set AFTER event dispatch (allows handlers to prevent capture)
- Release on PointerUp automatically

**Capture Event Routing**:
- Captured events bypass hit testing
- Transform to captured view's coordinate space
- Maintain capture until explicitly released or pointer up

### Keyboard Events

**Focus-Based Dispatch**:
- Keyboard events route to focused view first
- Call `dispatch_to_view(focused_id, event, directed: true)`
- Fall back to main view if not processed

**Built-in Navigation**:
- Tab navigation (forward/backward with Shift+Tab)
- Arrow navigation with Alt+Arrow keys
- Enter/Space activation for focused elements

**Activation Behavior**:
- Enter, NumpadEnter, and Space trigger click events on focused view
- Track `keyboard_navigation` state for visual feedback
- Update active styles for triggered views

### File Drag Events

**File Hover Tracking**:
- Track `window_state.file_hovered` for styling
- Clear on each drag move, rebuild like pointer hover
- Apply file hover styles only to views within bounds

**Drop Handling**:
- File drop events route through normal hit testing
- Transform coordinates like pointer events
- `DragDropped` events trigger style updates

### Window Events

**Global Event Dispatch**:
- Window events (resize, focus, theme) don't use hit testing
- Dispatch through stacking context to all views
- Views can opt-in to handle specific window events

**Responsive Style Updates**:
- `WindowResized` triggers style recalculation for views with responsive styles
- Check `has_style_selectors.has_responsive()` before updating

## Built-in Behaviors

### Focus Management

**Focus Conditions**:
- Only focusable views (`computed_style.get(Focusable)`) can receive focus
- Pointer must be within view bounds
- Focus changes trigger `FocusGained`/`FocusLost` events
- Keyboard navigation sets `keyboard_navigation` flag

**Focus Change Handling**:
- Compare old vs new focus state
- Fire focus events after event dispatch completes
- Update focus-related styles (`:focus` selector)

### Drag and Drop

**Drag Initiation**:
- Track potential drag with `drag_start: Option<(ViewId, Point)>`
- Start actual drag when moved > 1px from start point
- Set `window_state.active` for drag duration
- Fire `DragStart` event

**Drag State Tracking**:
- `dragging: Option<DragState>` with position and timing
- Update drag offset on pointer move
- Track `dragging_over` for drop target highlighting

**Drop Handling**:
- Fire `Drop` event on pointer up over valid target
- Fire `DragEnd` event on dragging view
- Clean up drag state

### Menu Systems

**Context Menus**:
- Secondary button up triggers context menu
- Position at pointer coordinates
- Only if view has context menu configured

**Popout Menus**:
- Primary button down triggers popout menu (timing varies by platform)
- Position at view's bottom-left corner
- Only if view has popout menu configured

## State Management Requirements

### Window State Tracking

**Hover State**:
- `hovered: SmallVec<[ViewId; 4]>` for normal hover
- `dragging_over: SmallVec<[ViewId; 4]>` for drag hover
- `file_hovered: HashSet<ViewId>` for file drag hover

**Interaction State**:
- `clicking: HashSet<ViewId>` for views receiving pointer down
- `active: Option<ViewId>` for drag operations
- `focus: Option<ViewId>` for keyboard focus
- `keyboard_navigation: bool` for keyboard interaction mode

**Drag State**:
- `drag_start: Option<(ViewId, Point)>` for tracking potential drags
- `dragging: Option<DragState>` for active drag operations
- `cursor: Option<CursorStyle>` for cursor management

**Pointer Capture State**:
- `pointer_capture_target: HashMap<PointerId, ViewId>` for active captures
- `pending_pointer_capture_target: HashMap<PointerId, Option<ViewId>>` for pending changes

### Style Updates

**Selector-Aware Updates**:
- Only request style updates for views with relevant selectors
- `:hover` updates for hover changes
- `:active` updates for clicking/drag state
- `:focus` updates for focus changes

**Performance Optimizations**:
- Batch style updates where possible
- Use `request_style_for_selector_recursive()` for targeted updates
- Avoid full tree updates for localized changes

## Error Handling and Edge Cases

### View Tree Modifications**:
- Views may be removed during event handling
- Hidden views must be skipped but children still checked
- Parent clip rectangles must be respected
- Disabled views skip most events but allow some (hover leave, capture lost)

### Coordinate Edge Cases**:
- Handle events outside window bounds gracefully
- Negative coordinates (e.g., over window decorations)
- Very large coordinates (infinity handling)
- Floating-point precision in hit testing

### Event Ordering Requirements**:
- Pointer capture events fire before other pointer events
- Click events fire after PointerUp completes
- Focus events fire after main event processing
- Hover enter/leave events fire after main event processing
- Style updates happen after event processing

## Testing Requirements

### Critical Test Scenarios**:
- Pointer down on A, move to B, up on B (no click on either)
- Pointer down on A, move to B, move back to A, up on A (click on A)
- Hover state updates with overlapping views
- Pointer capture routing and automatic release
- Keyboard navigation between focusable elements
- Drag and drop between different views
- Context menu positioning and triggering
- File drag hover styling

### Performance Test Cases**:
- Large view trees (1000+ views)
- Rapid pointer movement over many views
- Nested scrolling containers
- Complex transform hierarchies
- Frequent layout changes during interaction

## Compatibility Requirements

Any new implementation must:
1. Pass all existing event tests without modification
2. Maintain exact same event timing and ordering
3. Support all current event listener types
4. Preserve all built-in behaviors (focus, hover, drag, etc.)
5. Handle all coordinate transformations correctly
6. Maintain performance characteristics for large view trees
7. Support all platform-specific behaviors (macOS vs other OS menu timing)

This specification serves as the contract for event system compatibility. Any implementation satisfying these requirements should be a drop-in replacement for the current system.

## Baseline Performance Benchmarks

The following benchmark results were captured from the current event handling implementation to establish performance baselines that any new implementation should meet or exceed. All measurements are in nanoseconds (ns) or microseconds (µs).

### Event Dispatch Performance

**Flat Tree Events (overlapping views):**
- 10 views: ~390ns pointer_down, ~398ns pointer_move, ~905ns click
- 50 views: ~391ns pointer_down, ~399ns pointer_move, ~904ns click  
- 100 views: ~391ns pointer_down, ~401ns pointer_move, ~906ns click
- 500 views: ~391ns pointer_down, ~410ns pointer_move, ~906ns click

**Deep Tree Events (nested views):**
- 5 levels: ~887ns pointer_down, ~926ns pointer_move
- 10 levels: ~1.38µs pointer_down, ~1.50µs pointer_move
- 20 levels: ~2.45µs pointer_down, ~2.72µs pointer_move
- 50 levels: ~5.70µs pointer_down, ~6.52µs pointer_move

**Wide Tree Events (breadth):**
- 12 nodes (3x3+3): ~487ns pointer_down
- 30 nodes (5x5+5): ~486ns pointer_down  
- 110 nodes (10x10+10): ~485ns pointer_down
- 155 nodes (5³): ~486ns pointer_down

### Hit Testing Performance

**Hit Test Cache Effectiveness:**
- Hit testing with cache: ~2.25ns (cache hit, extremely fast)
- Same location 20x: ~400ns/event (high cache hit ratio)
- Alternating 2 locations: ~405ns/event (good cache performance)
- Alternating 3 locations: ~480ns/event (cache thrashing)

### Event Sequence Performance

**Complex Interactions:**
- Move+Click sequence: ~1.33µs (pointer_move + pointer_down + pointer_up)
- Drag sequence (10 moves): ~5.88µs total
- Scroll events: ~297ns per scroll event

### Path Dispatch vs Active View

**No Listeners Path (pure overhead):**
- Flat trees: ~910ns (regardless of width)
- Deep trees: ~3.28µs (10 levels), ~26.2µs (100 levels)

**Active View Dispatch (slider interaction):**
- Direct click: ~653ns (0 levels), ~3.07µs (10 levels)  
- Drag interaction: ~4.55µs (0 levels), ~12.9µs (5 levels)

### Key Performance Insights

1. **Hit Testing is Extremely Fast**: 2.25ns when cached, showing excellent cache effectiveness
2. **Tree Depth Impact**: Linear scaling ~100-120ns per depth level
3. **Sibling Count Impact**: Minimal - flat trees of 10-500 views perform identically
4. **Cache Effectiveness**: 2-entry cache handles alternating locations well, thrashes at 3+ locations
5. **Event Dispatch Overhead**: ~300-400ns base cost regardless of tree complexity
6. **Click Events Cost More**: ~2.3x cost vs pointer_down due to synthetic event generation
7. **No Listeners Early Exit**: Path building still has cost even when no processing needed

### Performance Requirements for New Implementation

Any spatial index-based replacement should:

1. **Maintain hit testing speed**: Target <5ns for cache hits, <100ns for cache misses
2. **Preserve dispatch performance**: Event dispatch should remain <500ns for typical trees  
3. **Scale better with depth**: Ideally sublinear scaling vs current linear O(depth)
4. **Improve cache design**: Consider larger cache or LRU replacement for >2 locations
5. **Optimize path building**: Pre-computed spatial index should reduce path building overhead
6. **Maintain click event timing**: Preserve current 2-phase click dispatch architecture

### Benchmark Reproduction

Run with: `cargo bench --bench event_dispatch`

These benchmarks were run on:
- **Date**: 2026-01-07
- **Hardware**: macOS (Darwin 25.1.0) 
- **Rust**: Release mode with optimizations
- **Criterion**: 100 samples per benchmark with statistical analysis

The benchmarks include comprehensive scenarios covering flat trees, deep trees, mixed stacking contexts, hit testing patterns, event sequences, cache effectiveness, and active view dispatch paths.