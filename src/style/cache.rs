//! Style computation cache for optimizing style resolution.
//!
//! This module implements a cache for style resolution results, inspired by
//! Chromium's MatchedPropertiesCache. When multiple views have identical styles
//! and interaction states, they can share the same resolved style object.
//!
//! ## Cache Key
//! The cache key consists of:
//! - Style identity (hash of the style's properties)
//! - Interaction state (hover, focus, disabled, etc.)
//! - Screen size breakpoint (for responsive styles)
//! - Classes applied
//!
//! ## Usage
//! The cache is stored in `WindowState` and used during style resolution
//! in `resolve_nested_maps`.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use rustc_hash::FxHasher;

use super::Style;
use crate::layout::responsive::ScreenSizeBp;
use crate::style::{InteractionState, StyleClassRef};

/// Maximum number of entries in the style cache.
/// This is a tunable parameter - larger values use more memory but improve hit rates.
const MAX_CACHE_SIZE: usize = 256;

/// A cache key for style resolution results.
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
    /// Hash of the parent context (inherited properties).
    context_hash: u64,
}

impl StyleCacheKey {
    /// Create a new cache key from the style resolution inputs.
    pub fn new(
        style: &Style,
        interact_state: &InteractionState,
        screen_size_bp: ScreenSizeBp,
        classes: &[StyleClassRef],
        context: &Style,
    ) -> Self {
        Self {
            style_hash: style.content_hash(),
            interaction_bits: interact_state.to_bits(),
            screen_size: screen_size_bp,
            classes_hash: hash_classes(classes),
            context_hash: context.content_hash(),
        }
    }
}

impl PartialEq for StyleCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.style_hash == other.style_hash
            && self.interaction_bits == other.interaction_bits
            && self.screen_size == other.screen_size
            && self.classes_hash == other.classes_hash
            && self.context_hash == other.context_hash
    }
}

impl Eq for StyleCacheKey {}

impl Hash for StyleCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.style_hash.hash(state);
        self.interaction_bits.hash(state);
        self.screen_size.hash(state);
        self.classes_hash.hash(state);
        self.context_hash.hash(state);
    }
}

/// Cache for style resolution results.
///
/// This cache stores resolved styles keyed by their inputs, allowing
/// views with identical styling to share the same resolved style object.
pub struct StyleCache {
    /// The cached style entries.
    cache: HashMap<StyleCacheKey, CacheEntry>,
    /// Simple LRU tracking - entry access count for eviction.
    access_counter: u64,
}

struct CacheEntry {
    /// The resolved style.
    style: Rc<Style>,
    /// Whether classes were applied during resolution.
    classes_applied: bool,
    /// Last access time for LRU eviction.
    last_access: u64,
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
            cache: HashMap::with_capacity(MAX_CACHE_SIZE),
            access_counter: 0,
        }
    }

    /// Look up a cached style resolution result.
    ///
    /// Returns `Some((style, classes_applied))` if found, `None` otherwise.
    pub fn get(&mut self, key: &StyleCacheKey) -> Option<(Rc<Style>, bool)> {
        if let Some(entry) = self.cache.get_mut(key) {
            self.access_counter += 1;
            entry.last_access = self.access_counter;
            Some((entry.style.clone(), entry.classes_applied))
        } else {
            None
        }
    }

    /// Insert a style resolution result into the cache.
    pub fn insert(&mut self, key: StyleCacheKey, style: Style, classes_applied: bool) {
        // Evict oldest entries if at capacity
        if self.cache.len() >= MAX_CACHE_SIZE {
            self.evict_oldest();
        }

        self.access_counter += 1;
        self.cache.insert(
            key,
            CacheEntry {
                style: Rc::new(style),
                classes_applied,
                last_access: self.access_counter,
            },
        );
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_counter = 0;
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Evict the oldest (least recently used) entries.
    fn evict_oldest(&mut self) {
        // Remove ~25% of entries to avoid frequent evictions
        let to_remove = MAX_CACHE_SIZE / 4;

        // Find the oldest entries
        let mut entries: Vec<_> = self
            .cache
            .iter()
            .map(|(k, v)| (k.clone(), v.last_access))
            .collect();
        entries.sort_by_key(|(_, access)| *access);

        // Remove the oldest ones
        for (key, _) in entries.into_iter().take(to_remove) {
            self.cache.remove(&key);
        }
    }

    /// Get cache statistics for debugging/profiling.
    #[allow(dead_code)]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            capacity: MAX_CACHE_SIZE,
        }
    }
}

/// Statistics about the style cache.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Number of entries currently in the cache.
    pub entries: usize,
    /// Maximum capacity of the cache.
    pub capacity: usize,
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
    fn to_bits(&self) -> u16 {
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
                StyleKeyInfo::Selector(_)
                | StyleKeyInfo::Class(_)
                | StyleKeyInfo::ContextMappings
                | StyleKeyInfo::ContextSelectors => {
                    // These contain nested Style maps - hash recursively
                    if let Some(nested_style) = value.downcast_ref::<Style>() {
                        nested_style.content_hash().hash(&mut hasher);
                    }
                }
                StyleKeyInfo::Transition => {
                    // Transitions don't affect computed style output
                    // Use pointer hash for identity
                    std::ptr::hash(std::rc::Rc::as_ptr(value), &mut hasher);
                }
            }
        }

        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let key = StyleCacheKey {
            style_hash: 123,
            interaction_bits: 0,
            screen_size: ScreenSizeBp::Xs,
            classes_hash: 0,
            context_hash: 0,
        };

        cache.insert(key.clone(), style, false);

        let result = cache.get(&key);
        assert!(result.is_some());
        assert!(!result.unwrap().1); // classes_applied = false
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = StyleCache::new();

        // Fill the cache beyond capacity
        for i in 0..(MAX_CACHE_SIZE + 10) {
            let key = StyleCacheKey {
                style_hash: i as u64,
                interaction_bits: 0,
                screen_size: ScreenSizeBp::Xs,
                classes_hash: 0,
                context_hash: 0,
            };
            cache.insert(key, Style::new(), false);
        }

        // Should have evicted some entries
        assert!(cache.len() <= MAX_CACHE_SIZE);
    }
}
