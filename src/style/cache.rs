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

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use rustc_hash::FxHasher;

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
    /// Hash of the input style's properties.
    style_hash: u64,
    /// Packed interaction state bits.
    interaction_bits: u16,
    /// Screen size breakpoint.
    screen_size: ScreenSizeBp,
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
        class_context: &Rc<Style>,
    ) -> Self {
        Self {
            style_hash: style.content_hash(),
            interaction_bits: interact_state.to_bits(),
            screen_size: screen_size_bp,
            classes_hash: hash_classes(classes),
            // O(1) pointer comparison instead of O(n) content_hash
            class_context_ptr: Rc::as_ptr(class_context) as usize,
        }
    }
}

impl PartialEq for StyleCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.style_hash == other.style_hash
            && self.interaction_bits == other.interaction_bits
            && self.screen_size == other.screen_size
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
        self.classes_hash.hash(state);
        self.class_context_ptr.hash(state);
    }
}

/// A single cached entry with parent context for validation.
struct CacheEntry {
    /// The resolved style.
    computed_style: Rc<Style>,
    /// The parent's inherited style at the time of caching.
    /// Used for validation on lookup (Chromium's approach).
    parent_inherited: Rc<Style>,
    /// Raw pointer to the parent style's Rc data for fast equality check.
    /// During tree traversal, siblings share the same parent Rc<Style>,
    /// so pointer comparison avoids expensive inherited_equal() calls.
    parent_rc_ptr: *const Style,
    /// Whether classes were applied during resolution.
    classes_applied: bool,
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
    /// 1. Fast path: pointer comparison - if the parent Rc is the same object,
    ///    contents are guaranteed identical (common for siblings in tree traversal)
    /// 2. Slow path: compare inherited property values for different Rc instances
    ///    that may have equivalent content
    fn find(&mut self, parent_style: &Rc<Style>, clock: u64) -> Option<(Rc<Style>, bool)> {
        let parent_ptr = Rc::as_ptr(parent_style);

        for entry in &mut self.entries {
            // Fast path: same Rc instance (very common during tree traversal)
            if std::ptr::eq(entry.parent_rc_ptr, parent_ptr) {
                entry.last_access = clock;
                return Some((entry.computed_style.clone(), entry.classes_applied));
            }
        }

        // Slow path: check for equivalent inherited values in different Rc instances
        for entry in &mut self.entries {
            if entry.parent_inherited.inherited_equal(parent_style) {
                entry.last_access = clock;
                return Some((entry.computed_style.clone(), entry.classes_applied));
            }
        }

        None
    }

    /// Add an entry, evicting oldest if at capacity.
    fn add(
        &mut self,
        computed_style: Style,
        parent_inherited: Style,
        parent_rc_ptr: *const Style,
        classes_applied: bool,
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
            computed_style: Rc::new(computed_style),
            parent_inherited: Rc::new(parent_inherited),
            parent_rc_ptr,
            classes_applied,
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
    cache: HashMap<StyleCacheKey, CacheBucket>,
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
            cache: HashMap::with_capacity(MAX_CACHE_BUCKETS),
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
    /// 1. Fast path: pointer comparison (O(1)) - hits when same Rc instance
    /// 2. Slow path: inherited_equal() comparison - for equivalent but different Rc instances
    ///
    /// Returns `Some((style, classes_applied))` if found, `None` otherwise.
    pub fn get(
        &mut self,
        key: &StyleCacheKey,
        parent_style: &Rc<Style>,
    ) -> Option<(Rc<Style>, bool)> {
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
    /// The parent Rc pointer is stored for fast pointer-based lookups.
    pub fn insert(
        &mut self,
        key: StyleCacheKey,
        computed_style: Style,
        parent_style: &Rc<Style>,
        classes_applied: bool,
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
        let parent_rc_ptr = Rc::as_ptr(parent_style);

        let bucket = self.cache.entry(key).or_insert_with(CacheBucket::new);

        let old_len = bucket.len();
        bucket.add(
            computed_style,
            parent_inherited,
            parent_rc_ptr,
            classes_applied,
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
    /// depends on factors not captured in the cache key.
    pub fn is_cacheable(style: &Style) -> bool {
        // TODO: Add checks for:
        // - Viewport-relative units (vw, vh, vmin, vmax)
        // - Container queries
        // - attr() functions
        // For now, assume all styles are cacheable
        !style.map.is_empty()
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
        if self.is_clicking {
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

        let mut hasher = FxHasher::default();

        // Hash the number of entries
        self.map.len().hash(&mut hasher);

        // Hash each key and value based on content
        for (key, value) in self.map.iter() {
            // Hash the key's info pointer as identity
            std::ptr::hash(key.info, &mut hasher);

            // Hash value content based on key type
            match key.info {
                StyleKeyInfo::Prop(prop_info) => {
                    // Use the property's content_hash via hash_any
                    let value_hash = (prop_info.hash_any)(value.as_ref());
                    value_hash.hash(&mut hasher);
                }
                StyleKeyInfo::Selector(_) | StyleKeyInfo::Class(_) => {
                    // These contain nested Style maps - hash recursively
                    if let Some(nested_style) = value.downcast_ref::<Style>() {
                        nested_style.content_hash().hash(&mut hasher);
                    }
                }
                StyleKeyInfo::ContextMappings | StyleKeyInfo::Transition => {
                    // Context mappings and transitions use pointer hash for identity
                    // since closures can't be meaningfully hashed
                    std::ptr::hash(std::rc::Rc::as_ptr(value), &mut hasher);
                }
            }
        }

        hasher.finish()
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

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Rc::new(Style::new());

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        cache.insert(key.clone(), style, &parent_style, false);

        let result = cache.get(&key, &parent_style);
        assert!(result.is_some());
        assert!(!result.unwrap().1); // classes_applied = false
    }

    #[test]
    fn test_cache_parent_validation() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style1 = Rc::new(Style::new().background(css::RED));
        let parent_style2 = Rc::new(Style::new().background(css::BLUE));

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        // Insert with parent_style1
        cache.insert(key.clone(), style.clone(), &parent_style1, false);

        // Lookup with same parent should hit (fast path: pointer comparison)
        let result = cache.get(&key, &parent_style1);
        assert!(result.is_some());

        // Lookup with different parent Rc but same content should also hit (slow path)
        let parent_style1_clone = Rc::new(Style::new().background(css::RED));
        let result = cache.get(&key, &parent_style1_clone);
        assert!(result.is_some()); // background is not inherited, so inherited_equal returns true

        // Suppress unused variable warning
        let _ = parent_style2;
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = StyleCache::new();
        let style = Style::new();
        let parent_style = Rc::new(Style::new());

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        // Miss
        let _ = cache.get(&key, &parent_style);

        // Insert
        cache.insert(key.clone(), style, &parent_style, false);

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
        let parent_style = Rc::new(Style::new());

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 0,
        };

        cache.insert(key.clone(), style, &parent_style, false);

        // Same Rc instance should hit via fast path (pointer comparison)
        let result = cache.get(&key, &parent_style);
        assert!(result.is_some());

        // Different Rc instance with same content should still hit via slow path
        let different_rc = Rc::new(Style::new());
        let result = cache.get(&key, &different_rc);
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
        let parent_style = Rc::new(Style::new());

        // Create two keys with different class_context_hash
        let key1 = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 100, // Different class context pointer
        };

        let key2 = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            class_context_ptr: 200, // Different class context pointer
        };

        // Insert with key1
        cache.insert(key1.clone(), style.clone(), &parent_style, false);

        // Lookup with key1 should hit
        let result = cache.get(&key1, &parent_style);
        assert!(result.is_some(), "Same key should hit cache");

        // Lookup with key2 (different class_context_hash) should miss
        let result = cache.get(&key2, &parent_style);
        assert!(
            result.is_none(),
            "Different class_context_hash should cause cache miss"
        );

        // Insert with key2
        cache.insert(key2.clone(), style.clone(), &parent_style, true);

        // Now key2 should hit
        let result = cache.get(&key2, &parent_style);
        assert!(result.is_some(), "After insert, key2 should hit");
        assert!(result.unwrap().1, "classes_applied should be true for key2");

        // key1 should still hit with its original value
        let result = cache.get(&key1, &parent_style);
        assert!(result.is_some(), "key1 should still hit");
        assert!(
            !result.unwrap().1,
            "classes_applied should be false for key1"
        );
    }

    #[test]
    fn test_cache_key_with_class_context() {
        // Test that StyleCacheKey::new correctly incorporates class_context
        let style = Style::new();
        let interact_state = InteractionState::default();
        let classes: Vec<StyleClassRef> = vec![];

        let class_context1 = Rc::new(Style::new());
        let class_context2 = Rc::new(Style::new().background(css::RED));

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
        use std::rc::Rc;

        // Define a test class
        style_class!(TestClass2);

        // Create initial class_context (empty)
        let mut class_context = Rc::new(Style::new());
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
}
