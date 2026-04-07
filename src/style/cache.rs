//! Style computation cache for optimizing style resolution.
//!
//! This module implements a cache for style resolution results, inspired by
//! Chromium's MatchedPropertiesCache. When multiple views have identical styles
//! and interaction states, they can share the same resolved style object.
//!
//! ## Key Design (matching Chromium's approach)
//!
//! The cache is keyed by a hash of:
//! - Style identity (hash of the style's properties)
//! - Interaction state (hover, focus, disabled, etc.)
//! - Screen size breakpoint (for responsive styles)
//! - Classes applied
//!
//! But critically, on **lookup**, we also validate that the parent's inherited
//! properties match. This is because two elements with identical style rules
//! can have different computed styles if their parents have different inherited
//! values (e.g., font-size, color).
//!
//! ## Cacheability
//!
//! Not all styles are cacheable. We skip caching for:
//! - Styles with viewport-relative units
//! - Styles with container queries (future)
//! - Styles that depend on element-specific attributes

use std::hash::{Hash, Hasher};

use super::cx::InheritedInteractionCx;
use super::selectors::StyleSelectors;

use rustc_hash::{FxHashMap, FxHasher};

use super::Style;
use crate::layout::responsive::ScreenSizeBp;
use crate::style::{InteractionState, StyleClassRef};

/// Maximum number of hash buckets in the cache.
const MAX_CACHE_BUCKETS: usize = 256;

/// Maximum entries per bucket (handles hash collisions).
const MAX_ENTRIES_PER_BUCKET: usize = 4;

/// Target number of entries when pruning.
const PRUNE_TARGET: usize = 192;

/// A cache key for style resolution results.
///
/// This is used as the HashMap key. The actual validation happens
/// on lookup by comparing parent inherited styles.
#[derive(Clone, Debug)]
pub struct StyleCacheKey {
    /// Content hash of the input style's properties.
    /// Enables cross-view cache sharing for styles with identical content.
    style_hash: u64,
    /// Packed interaction state bits.
    interaction_bits: u16,
    /// Screen size breakpoint.
    screen_size: ScreenSizeBp,
    /// Window width in bits — needed for responsive selectors with exact pixel thresholds
    /// (e.g. `max_window_width(700.0)`) that aren't captured by the discrete breakpoint.
    window_width_bits: u64,
    /// Hash of the applied classes.
    classes_hash: u64,
    /// Pointer to the class context Rc (used as identity).
    /// Using pointer instead of content_hash() for O(1) key creation.
    /// This works because the same logical class_context shares the same Rc instance
    /// across siblings, and Rc::make_mut creates a new Rc when content changes.
    class_context_ptr: usize,
}

impl StyleCacheKey {
    /// Create a new cache key from the style resolution inputs.
    pub fn new(
        style: &Style,
        interact_state: &InteractionState,
        screen_size_bp: ScreenSizeBp,
        classes: &[StyleClassRef],
        class_context: &Style,
    ) -> Self {
        Self::new_from_hash(
            style.content_hash(),
            interact_state,
            screen_size_bp,
            classes,
            class_context,
        )
    }

    /// Create a cache key from a pre-computed content hash.
    ///
    /// This avoids needing a `&Style` reference, allowing the caller to
    /// use a cached hash without cloning the full style.
    pub fn new_from_hash(
        style_hash: u64,
        interact_state: &InteractionState,
        screen_size_bp: ScreenSizeBp,
        classes: &[StyleClassRef],
        class_context: &Style,
    ) -> Self {
        Self {
            style_hash,
            interaction_bits: interact_state.to_bits(),
            screen_size: screen_size_bp,
            window_width_bits: interact_state.window_width.to_bits(),
            classes_hash: hash_classes(classes),
            class_context_ptr: class_context.map_ptr(),
        }
    }
}

impl PartialEq for StyleCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.style_hash == other.style_hash
            && self.interaction_bits == other.interaction_bits
            && self.screen_size == other.screen_size
            && self.window_width_bits == other.window_width_bits
            && self.classes_hash == other.classes_hash
            && self.class_context_ptr == other.class_context_ptr
    }
}

impl Eq for StyleCacheKey {}

impl Hash for StyleCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.style_hash.hash(state);
        self.interaction_bits.hash(state);
        self.screen_size.hash(state);
        self.window_width_bits.hash(state);
        self.classes_hash.hash(state);
        self.class_context_ptr.hash(state);
    }
}

/// The result of a cache hit, containing all outputs of `compute_combined()`.
pub struct CacheHit {
    /// The resolved combined style.
    pub combined_style: Style,
    /// The detected style selectors.
    pub has_style_selectors: Option<StyleSelectors>,
    /// View-local interaction flags derived from the combined style.
    pub post_interact: InheritedInteractionCx,
}

/// A single cached entry with parent context for validation.
struct CacheEntry {
    /// The resolved combined style.
    combined_style: Style,
    /// The detected style selectors.
    has_style_selectors: Option<StyleSelectors>,
    /// View-local interaction flags derived from the combined style.
    post_interact: InheritedInteractionCx,
    /// The parent's inherited style at the time of caching.
    /// Used for validation on lookup (Chromium's approach).
    parent_inherited: Style,
    /// Pointer to the parent's inner map Rc for fast equality check.
    /// During tree traversal, siblings share the same parent Style,
    /// so pointer comparison avoids expensive inherited_equal() calls.
    parent_map_ptr: usize,
    /// Last access time for LRU eviction.
    last_access: u64,
}

/// A bucket that can hold multiple entries (handles hash collisions).
struct CacheBucket {
    entries: Vec<CacheEntry>,
}

impl CacheBucket {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(2),
        }
    }

    /// Find an entry that matches the parent's inherited style.
    ///
    /// Uses a two-tier lookup strategy (inspired by Chromium):
    /// 1. Fast path: pointer comparison - if the parent's inner map Rc is the same,
    ///    contents are guaranteed identical (common for siblings in tree traversal)
    /// 2. Slow path: compare inherited property values for different instances
    ///    that may have equivalent content
    fn find(&mut self, parent_style: &Style, clock: u64) -> Option<CacheHit> {
        let parent_ptr = parent_style.map_ptr();

        for entry in &mut self.entries {
            // Fast path: same inner Rc instance (very common during tree traversal)
            if entry.parent_map_ptr == parent_ptr {
                entry.last_access = clock;
                return Some(CacheHit {
                    combined_style: entry.combined_style.clone(),
                    has_style_selectors: entry.has_style_selectors,
                    post_interact: entry.post_interact,
                });
            }
        }

        // Slow path: check for equivalent inherited values in different instances
        for entry in &mut self.entries {
            if entry.parent_inherited.inherited_equal(parent_style) {
                entry.last_access = clock;
                return Some(CacheHit {
                    combined_style: entry.combined_style.clone(),
                    has_style_selectors: entry.has_style_selectors,
                    post_interact: entry.post_interact,
                });
            }
        }

        None
    }

    /// Add an entry, evicting oldest if at capacity.
    fn add(
        &mut self,
        combined_style: Style,
        has_style_selectors: Option<StyleSelectors>,
        post_interact: InheritedInteractionCx,
        parent_inherited: Style,
        parent_map_ptr: usize,
        clock: u64,
    ) {
        // Evict oldest if at capacity
        if self.entries.len() >= MAX_ENTRIES_PER_BUCKET {
            let oldest_idx = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_access)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.entries.remove(oldest_idx);
        }

        self.entries.push(CacheEntry {
            combined_style,
            has_style_selectors,
            post_interact,
            parent_inherited,
            parent_map_ptr,
            last_access: clock,
        });
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Cache for style resolution results.
///
/// This cache stores resolved styles keyed by their inputs, allowing
/// views with identical styling to share the same resolved style object.
///
/// Unlike a simple hash cache, this validates parent inherited properties
/// on lookup to ensure correctness (matching Chromium's MatchedPropertiesCache).
pub struct StyleCache {
    /// The cached style buckets, keyed by style hash.
    cache: FxHashMap<StyleCacheKey, CacheBucket>,
    /// Virtual clock for LRU purposes.
    clock: u64,
    /// Total number of entries across all buckets.
    total_entries: usize,
    /// Statistics for monitoring.
    stats: CacheStatsMut,
}

#[derive(Default)]
struct CacheStatsMut {
    hits: u64,
    misses: u64,
    insertions: u64,
    evictions: u64,
}

impl Default for StyleCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleCache {
    /// Create a new empty style cache.
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::with_capacity_and_hasher(MAX_CACHE_BUCKETS, Default::default()),
            clock: 0,
            total_entries: 0,
            stats: CacheStatsMut::default(),
        }
    }

    /// Look up a cached style resolution result.
    ///
    /// This performs Chromium-style validation: even if the hash matches,
    /// we verify that the parent's inherited properties are equal.
    ///
    /// Uses a two-tier lookup for performance:
    /// 1. Fast path: pointer comparison (O(1)) - hits when same inner map Rc
    /// 2. Slow path: inherited_equal() comparison - for equivalent but different instances
    ///
    /// Returns `Some(CacheHit)` if found, `None` otherwise.
    pub fn get(
        &mut self,
        key: &StyleCacheKey,
        parent_style: &Style,
    ) -> Option<CacheHit> {
        self.clock += 1;

        if let Some(bucket) = self.cache.get_mut(key)
            && let Some(result) = bucket.find(parent_style, self.clock)
        {
            self.stats.hits += 1;
            return Some(result);
        }

        self.stats.misses += 1;
        None
    }

    /// Insert a style resolution result into the cache.
    ///
    /// We store the parent's inherited style so we can validate on lookup.
    /// The parent's inner map pointer is stored for fast pointer-based lookups.
    pub fn insert(
        &mut self,
        key: StyleCacheKey,
        combined_style: &Style,
        has_style_selectors: Option<StyleSelectors>,
        post_interact: InheritedInteractionCx,
        parent_style: &Style,
    ) {
        // Prune if we have too many entries
        if self.total_entries >= MAX_CACHE_BUCKETS * MAX_ENTRIES_PER_BUCKET {
            self.prune();
        }

        self.clock += 1;
        self.stats.insertions += 1;

        // Extract only inherited properties from parent for storage
        let parent_inherited = parent_style.inherited();
        // Store pointer for fast comparison during lookup
        let parent_map_ptr = parent_style.map_ptr();

        let bucket = self.cache.entry(key).or_insert_with(CacheBucket::new);

        let old_len = bucket.len();
        bucket.add(
            combined_style.clone(),
            has_style_selectors,
            post_interact,
            parent_inherited,
            parent_map_ptr,
            self.clock,
        );
        let new_len = bucket.len();

        // Update entry count
        if new_len > old_len {
            self.total_entries += 1;
        }
    }

    /// Check if a style is cacheable.
    ///
    /// Some styles cannot be safely cached because their computed value
    /// depends on factors not captured in the cache key:
    /// - Structural selectors (`:first-child`, `:nth-child`) depend on position
    /// - Context values resolve against inherited context, but hash to a constant
    pub fn is_cacheable(style: &Style) -> bool {
        !style.map.is_empty()
            && !style.has_structural_selectors()
            && !style.has_context_values()
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.total_entries = 0;
        self.clock = 0;
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.total_entries
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.total_entries == 0
    }

    /// Prune the cache using LRU eviction.
    fn prune(&mut self) {
        // Collect all entries with their access times
        let mut all_entries: Vec<(StyleCacheKey, usize, u64)> = Vec::new();
        for (key, bucket) in &self.cache {
            for (idx, entry) in bucket.entries.iter().enumerate() {
                all_entries.push((key.clone(), idx, entry.last_access));
            }
        }

        // Sort by access time (oldest first)
        all_entries.sort_by_key(|(_, _, access)| *access);

        // Remove entries until we hit the target
        let to_remove = self.total_entries.saturating_sub(PRUNE_TARGET);
        let mut removed = 0;

        for (key, _, _) in all_entries.into_iter().take(to_remove) {
            if let Some(bucket) = self.cache.get_mut(&key)
                && !bucket.is_empty()
            {
                // Remove oldest entry in this bucket
                let oldest_idx = bucket
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.last_access)
                    .map(|(i, _)| i);

                if let Some(idx) = oldest_idx {
                    bucket.entries.remove(idx);
                    removed += 1;
                    self.stats.evictions += 1;
                }
            }
        }

        self.total_entries = self.total_entries.saturating_sub(removed);

        // Remove empty buckets
        self.cache.retain(|_, bucket| !bucket.is_empty());
    }

    /// Get cache statistics for debugging/profiling.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.total_entries,
            buckets: self.cache.len(),
            hits: self.stats.hits,
            misses: self.stats.misses,
            insertions: self.stats.insertions,
            evictions: self.stats.evictions,
            hit_rate: if self.stats.hits + self.stats.misses > 0 {
                self.stats.hits as f64 / (self.stats.hits + self.stats.misses) as f64
            } else {
                0.0
            },
        }
    }
}

/// Statistics about the style cache.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Number of entries currently in the cache.
    pub entries: usize,
    /// Number of buckets (unique keys).
    pub buckets: usize,
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Number of insertions.
    pub insertions: u64,
    /// Number of evictions.
    pub evictions: u64,
    /// Hit rate (0.0 to 1.0).
    pub hit_rate: f64,
}

/// Hash a list of style classes.
fn hash_classes(classes: &[StyleClassRef]) -> u64 {
    let mut hasher = FxHasher::default();
    for class in classes {
        // Use the class key's pointer as identity
        std::ptr::hash(class.key.info, &mut hasher);
    }
    hasher.finish()
}

impl InteractionState {
    /// Pack interaction state into bits for efficient hashing.
    fn to_bits(self) -> u16 {
        let mut bits = 0u16;
        if self.is_hovered {
            bits |= 1 << 0;
        }
        if self.is_selected {
            bits |= 1 << 1;
        }
        if self.is_disabled {
            bits |= 1 << 2;
        }
        if self.is_focused {
            bits |= 1 << 3;
        }
        if self.is_active {
            bits |= 1 << 4;
        }
        if self.is_dark_mode {
            bits |= 1 << 5;
        }
        if self.is_file_hover {
            bits |= 1 << 6;
        }
        if self.using_keyboard_navigation {
            bits |= 1 << 7;
        }
        if self.is_focus_within {
            bits |= 1 << 8;
        }
        bits
    }
}

impl Style {
    /// Compute a content hash of this style for cache keying.
    ///
    /// This hash captures the identity of the style's contents using
    /// value-based hashing. Identical style values will produce identical
    /// hashes, enabling effective cache sharing.
    pub fn content_hash(&self) -> u64 {
        use crate::style::props::StyleKeyInfo;

        // Use XOR-based order-independent hashing to avoid Vec allocation + sort.
        // Each entry is independently hashed and XOR'd into the accumulator.
        // We mix in the entry count separately to distinguish e.g. {A} from {A, B}
        // when their entry hashes happen to cancel.
        let mut combined: u64 = 0;
        let mut count_hasher = FxHasher::default();
        self.map.len().hash(&mut count_hasher);

        for (key, value) in self.map.iter() {
            let mut entry_hasher = FxHasher::default();
            std::ptr::hash(key.info, &mut entry_hasher);

            match key.info {
                StyleKeyInfo::Prop(prop_info) => {
                    let value_hash = (prop_info.hash_any)(value.as_ref());
                    value_hash.hash(&mut entry_hasher);
                }
                StyleKeyInfo::Selector(_) | StyleKeyInfo::Class(_) => {
                    if let Some(nested_style) = value.downcast_ref::<Style>() {
                        nested_style.content_hash().hash(&mut entry_hasher);
                    }
                }
                StyleKeyInfo::StructuralSelectors
                | StyleKeyInfo::ResponsiveSelectors
                | StyleKeyInfo::DeferredEffects
                | StyleKeyInfo::DebugGroup(_)
                | StyleKeyInfo::Transition => {
                    std::ptr::hash(std::rc::Rc::as_ptr(value), &mut entry_hasher);
                }
            }

            combined ^= entry_hasher.finish();
        }

        combined ^ count_hasher.finish()
    }

    /// Check if the inherited properties of this style equal another's.
    ///
    /// This is the key comparison for cache validation (Chromium's approach).
    /// Two styles with different inherited values cannot share a cache entry
    /// even if their non-inherited properties are identical.
    pub fn inherited_equal(&self, other: &Style) -> bool {
        use crate::style::props::StyleKeyInfo;

        // Compare only inherited properties
        for (key, value) in self.map.iter() {
            if let StyleKeyInfo::Prop(prop_info) = key.info
                && prop_info.inherited
            {
                // Check if other has this property with equal value
                if let Some(other_value) = other.map.get(key) {
                    if !(prop_info.eq_any)(value.as_ref(), other_value.as_ref()) {
                        return false;
                    }
                } else {
                    // Other doesn't have this inherited property
                    return false;
                }
            }
        }

        // Check if other has inherited properties we don't have
        for (key, _) in other.map.iter() {
            if let StyleKeyInfo::Prop(prop_info) = key.info
                && prop_info.inherited
                && !self.map.contains_key(key)
            {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::color::palette::css;

    #[test]
    fn test_interaction_state_bits() {
        let state = InteractionState {
            is_hovered: true,
            is_focused: true,
            ..Default::default()
        };
        let bits = state.to_bits();
        assert_eq!(bits & 1, 1); // hovered (bit 0)
        assert_eq!(bits & (1 << 3), 1 << 3); // focused (bit 3)
        assert_eq!(bits & (1 << 2), 0); // not disabled (bit 2)
    }

    /// Default interaction values for cache test entries.
    const DEFAULT_POST_INTERACT: InheritedInteractionCx = InheritedInteractionCx {
        disabled: false,
        selected: false,
        hidden: false,
    };

    /// Helper to insert a style into the cache with default interaction/selector values.
    fn cache_insert(cache: &mut StyleCache, key: StyleCacheKey, style: &Style, parent: &Style) {
        cache.insert(key, style, None, DEFAULT_POST_INTERACT, parent);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Style::new();

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        cache_insert(&mut cache, key.clone(), &style, &parent_style);

        let result = cache.get(&key, &parent_style);
        assert!(result.is_some());
    }

    #[test]
    fn test_cache_parent_validation() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style1 = Style::new().background(css::RED);
        let parent_style2 = Style::new().background(css::BLUE);

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        // Insert with parent_style1
        cache_insert(&mut cache, key.clone(), &style, &parent_style1);

        // Lookup with same parent should hit (fast path: pointer comparison)
        let result = cache.get(&key, &parent_style1);
        assert!(result.is_some());

        // Lookup with different Style but same content should also hit (slow path)
        let parent_style1_clone = Style::new().background(css::RED);
        let result = cache.get(&key, &parent_style1_clone);
        assert!(result.is_some()); // background is not inherited, so inherited_equal returns true

        // Suppress unused variable warning
        let _ = parent_style2;
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Style::new();

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        // Miss
        let _ = cache.get(&key, &parent_style);

        // Insert
        cache_insert(&mut cache, key.clone(), &style, &parent_style);

        // Hit
        let _ = cache.get(&key, &parent_style);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.insertions, 1);
    }

    #[test]
    fn test_cache_pointer_fast_path() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Style::new();

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        cache_insert(&mut cache, key.clone(), &style, &parent_style);

        // Same Style instance should hit via fast path (pointer comparison on inner map Rc)
        let result = cache.get(&key, &parent_style);
        assert!(result.is_some());

        // Different Style instance with same content should still hit via slow path
        let different_style = Style::new();
        let result = cache.get(&key, &different_style);
        assert!(result.is_some()); // Empty styles have equal inherited props
    }

    #[test]
    fn test_inherited_equal() {
        let style1 = Style::new();
        let style2 = Style::new();

        // Two empty styles should be equal
        assert!(style1.inherited_equal(&style2));
    }

    #[test]
    fn test_different_class_context_causes_cache_miss() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Style::new();

        // Create two keys with different class_context_ptr
        let key1 = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 100,
        };

        let key2 = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            window_width_bits: 0,
            classes_hash: 0,
            class_context_ptr: 200,
        };

        // Insert with key1
        cache_insert(&mut cache, key1.clone(), &style, &parent_style);

        // Lookup with key1 should hit
        let result = cache.get(&key1, &parent_style);
        assert!(result.is_some(), "Same key should hit cache");

        // Lookup with key2 (different class_context_ptr) should miss
        let result = cache.get(&key2, &parent_style);
        assert!(
            result.is_none(),
            "Different class_context_ptr should cause cache miss"
        );

        // Insert with key2
        cache_insert(&mut cache, key2.clone(), &style, &parent_style);

        // Now key2 should hit
        let result = cache.get(&key2, &parent_style);
        assert!(result.is_some(), "After insert, key2 should hit");

        // key1 should still hit with its original value
        let result = cache.get(&key1, &parent_style);
        assert!(result.is_some(), "key1 should still hit");
    }

    #[test]
    fn test_cache_key_with_class_context() {
        // Test that StyleCacheKey::new correctly incorporates class_context
        let style = Style::new();
        let interact_state = InteractionState::default();
        let classes: Vec<StyleClassRef> = vec![];

        let class_context1 = Style::new();
        let class_context2 = Style::new().background(css::RED);

        let key1 = StyleCacheKey::new(
            &style,
            &interact_state,
            ScreenSizeBp::Xs,
            &classes,
            &class_context1,
        );

        let key2 = StyleCacheKey::new(
            &style,
            &interact_state,
            ScreenSizeBp::Xs,
            &classes,
            &class_context2,
        );

        // Keys with different class contexts should not be equal
        assert_ne!(
            key1, key2,
            "Keys with different class contexts should not be equal"
        );

        // Same class context should produce equal keys
        let key1_again = StyleCacheKey::new(
            &style,
            &interact_state,
            ScreenSizeBp::Xs,
            &classes,
            &class_context1,
        );
        assert_eq!(key1, key1_again, "Same inputs should produce equal keys");
    }

    #[test]
    fn test_content_hash_changes_with_class_map() {
        use crate::style_class;

        // Define a test class
        style_class!(TestClass);

        // Create a base style
        let base = Style::new().background(css::RED);
        let hash1 = base.content_hash();

        // Verify hash is consistent
        assert_eq!(base.content_hash(), hash1, "Hash should be consistent");

        // Clone the style and add a class map
        let cloned = base.clone();
        let cloned = cloned.class(TestClass, |s| s.background(css::BLUE));

        // Get the hash after adding class
        let hash2 = cloned.content_hash();

        // The hash should be different after adding a class map
        assert_ne!(
            hash1, hash2,
            "Hash should be different after adding class map"
        );
    }

    #[test]
    fn test_apply_only_class_maps_changes_hash() {
        use crate::style_class;

        // Define a test class
        style_class!(TestClass2);

        // Create initial class_context (empty)
        let mut class_context = Style::new();
        let initial_hash = class_context.content_hash();

        // Create a style with class maps (like parent's direct style would have)
        let direct_style = Style::new().class(TestClass2, |s| s.background(css::GREEN));

        // Apply class maps to class_context (simulating what cx.rs does)
        Style::apply_only_class_maps(&mut class_context, &direct_style);

        // The class_context hash should now be different (new Rc with modified content)
        let final_hash = class_context.content_hash();

        assert_ne!(
            initial_hash, final_hash,
            "Class context hash should change after applying class maps"
        );
    }

    #[test]
    fn test_content_hash_is_order_independent() {
        // Two styles with same properties added in different order
        let s1 = Style::new().background(css::RED).color(css::BLUE);
        let s2 = Style::new().color(css::BLUE).background(css::RED);
        assert_eq!(
            s1.content_hash(),
            s2.content_hash(),
            "Hash should be identical regardless of property insertion order"
        );
    }

    #[test]
    fn test_focus_within_affects_cache_key() {
        let style = Style::new();
        let classes: Vec<StyleClassRef> = vec![];
        let class_context = Style::new();

        let state_without = InteractionState {
            is_focus_within: false,
            ..Default::default()
        };
        let state_with = InteractionState {
            is_focus_within: true,
            ..Default::default()
        };

        let key1 = StyleCacheKey::new(&style, &state_without, ScreenSizeBp::Xs, &classes, &class_context);
        let key2 = StyleCacheKey::new(&style, &state_with, ScreenSizeBp::Xs, &classes, &class_context);

        assert_ne!(key1, key2, "is_focus_within should produce different cache keys");
    }

    #[test]
    fn test_structural_selectors_uncacheable() {
        let plain_style = Style::new().background(css::RED);
        assert!(
            StyleCache::is_cacheable(&plain_style),
            "Plain style should be cacheable"
        );

        let structural_style = plain_style.clone().first_child(|s| s.background(css::BLUE));
        assert!(
            !StyleCache::is_cacheable(&structural_style),
            "Style with structural selectors should not be cacheable"
        );
    }

    #[test]
    fn test_inherited_equal_with_actual_inherited_props() {
        use crate::style::TextColor;

        let style1 = Style::new().set(TextColor, Some(css::RED));
        let style2 = Style::new().set(TextColor, Some(css::BLUE));
        let style3 = Style::new().set(TextColor, Some(css::RED));

        assert!(
            !style1.inherited_equal(&style2),
            "Different inherited values should not be equal"
        );
        assert!(
            style1.inherited_equal(&style3),
            "Same inherited values should be equal"
        );
    }

    #[test]
    fn test_cache_eviction_under_pressure() {
        let mut cache = StyleCache::new();
        let parent_style = Style::new();

        // Insert more than MAX_CACHE_BUCKETS * MAX_ENTRIES_PER_BUCKET entries
        let limit = MAX_CACHE_BUCKETS * MAX_ENTRIES_PER_BUCKET + 100;
        for i in 0..limit {
            let key = StyleCacheKey {
                style_hash: i as u64,
                interaction_bits: 0,
                screen_size: ScreenSizeBp::Xs,
                window_width_bits: 0,
                classes_hash: 0,
                class_context_ptr: 0,
            };
            let style = Style::new().width(i as f64);
            cache_insert(&mut cache, key, &style, &parent_style);
        }

        // Total entries should be bounded (pruning should have occurred)
        let stats = cache.stats();
        assert!(
            stats.evictions > 0,
            "Evictions should have occurred under pressure"
        );
    }
}
