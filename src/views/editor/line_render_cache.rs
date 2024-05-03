use floem_editor_core::buffer::InvalLines;

/// Starts at a specific `base_line`, and then grows from there.  
/// This is internally an array, so that newlines and moving the viewport up can be easily handled.
#[derive(Debug, Clone)]
pub struct LineRenderCache<T> {
    base_line: usize,
    entries: Vec<Option<T>>,
}
impl<T> LineRenderCache<T> {
    pub fn new() -> Self {
        Self::default()
    }

    fn idx(&self, line: usize) -> Option<usize> {
        line.checked_sub(self.base_line)
    }

    pub fn base_line(&self) -> usize {
        self.base_line
    }

    pub fn min_line(&self) -> usize {
        self.base_line
    }

    pub fn max_line(&self) -> Option<usize> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.min_line() + self.entries.len() - 1)
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.base_line = 0;
        self.entries.clear();
    }

    pub fn get(&self, line: usize) -> Option<&T> {
        let idx = self.idx(line)?;
        self.entries.get(idx).map(|x| x.as_ref()).flatten()
    }

    pub fn get_mut(&mut self, line: usize) -> Option<&mut T> {
        let idx = self.idx(line)?;
        self.entries.get_mut(idx).map(|x| x.as_mut()).flatten()
    }

    pub fn insert(&mut self, line: usize, entry: T) {
        if line < self.base_line {
            let old_base = self.base_line;
            self.base_line = line;
            // Resize the entries at the start to fit the new count
            let new_count = old_base - line;
            self.entries
                .splice(0..0, std::iter::repeat_with(|| None).take(new_count));
        } else if self.entries.is_empty() {
            self.base_line = line;
            self.entries.push(None);
        } else if line >= self.base_line + self.entries.len() {
            let new_len = line - self.base_line + 1;
            self.entries.resize_with(new_len, || None);
        }
        let idx = self.idx(line).unwrap();
        let res = self.entries.get_mut(idx).unwrap();
        *res = Some(entry);
    }

    /// Invalidates the entries at the given `start_line` for `inval_count` lines.  
    /// `new_count` is used to know whether to insert new line entries or to remove them, such as
    /// for a newline.
    pub fn invalidate(
        &mut self,
        InvalLines {
            start_line,
            inval_count,
            new_count,
        }: InvalLines,
    ) {
        let ib_start_line = start_line.max(self.base_line);
        let start_idx = self.idx(ib_start_line).unwrap();

        if start_idx >= self.entries.len() {
            return;
        }

        let end_idx = if start_line >= self.base_line {
            start_idx + inval_count
        } else {
            // If start_line + inval_count isn't within the range of the entries then it'd just be 0
            let within_count = inval_count.saturating_sub(self.base_line - start_line);
            start_idx + within_count
        };
        let ib_end_idx = end_idx.min(self.entries.len());

        for i in start_idx..ib_end_idx {
            self.entries[i] = None;
        }

        if new_count == inval_count {
            return;
        }

        if new_count > inval_count {
            let extra = new_count - inval_count;
            self.entries.splice(
                ib_end_idx..ib_end_idx,
                std::iter::repeat_with(|| None).take(extra),
            );
        } else {
            // How many (invalidated) line entries should be removed.
            // (Since all of the lines in the inval lines area are `None` now, it doesn't matter if
            // they were some other line number originally if we're draining them out)
            let mut to_remove = inval_count;
            let mut to_keep = new_count;

            let oob_start = ib_start_line - start_line;

            // Shift the base line backwards by the amount outside the start
            // This allows us to not bother with removing entries from the array in some cases
            {
                let oob_start_remove = oob_start.min(to_remove);

                self.base_line -= oob_start_remove;
                to_remove = to_remove.saturating_sub(oob_start_remove);
                to_keep = to_keep.saturating_sub(oob_start_remove);
            }

            if to_remove == 0 {
                // There is nothing more to remove
                return;
            }

            let remove_start_idx = start_idx + to_keep;
            let remove_end_idx = (start_idx + to_remove).min(self.entries.len());

            self.entries.drain(remove_start_idx..remove_end_idx);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Option<&T>> {
        self.entries.iter().map(|x| x.as_ref())
    }

    pub fn iter_with_line(&self) -> impl Iterator<Item = (usize, Option<&T>)> {
        let base_line = self.base_line();
        self.entries
            .iter()
            .enumerate()
            .map(move |(i, x)| (i + base_line, x.as_ref()))
    }
}

impl<T> Default for LineRenderCache<T> {
    fn default() -> Self {
        LineRenderCache {
            base_line: 0,
            entries: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use floem_editor_core::buffer::InvalLines;

    use crate::views::editor::line_render_cache::LineRenderCache;

    #[test]
    fn line_render_cache() {
        let mut c = LineRenderCache::default();

        assert_eq!(c.base_line, 0);
        assert!(c.is_empty());

        c.insert(0, 0);
        assert_eq!(c.base_line, 0);
        assert_eq!(c.entries.len(), 1);

        c.insert(1, 1);
        assert_eq!(c.base_line, 0);
        assert_eq!(c.entries.len(), 2);

        c.insert(10, 2);
        assert_eq!(c.base_line, 0);
        assert_eq!(c.entries.len(), 11);

        let mut c = LineRenderCache::default();
        c.insert(10, 10);
        assert_eq!(c.base_line, 10);
        assert_eq!(c.entries.len(), 1);

        c.insert(8, 8);
        assert_eq!(c.base_line, 8);
        assert_eq!(c.entries.len(), 3);

        c.insert(5, 5);
        assert_eq!(c.base_line, 5);
        assert_eq!(c.entries.len(), 6);

        assert!(c.get(0).is_none());
        assert!(c.get(5).is_some());
        assert!(c.get(8).is_some());
        assert!(c.get(10).is_some());
        assert!(c.get(11).is_none());

        let mut c2 = c.clone();
        c2.invalidate(InvalLines::new(0, 1, 1));
        assert!(c2.get(0).is_none());
        assert!(c2.get(5).is_some());
        assert!(c2.get(8).is_some());
        assert!(c2.get(10).is_some());
        assert!(c2.get(11).is_none());

        let mut c2 = c.clone();
        c2.invalidate(InvalLines::new(5, 1, 1));
        assert!(c2.get(0).is_none());
        assert!(c2.get(5).is_none());
        assert!(c2.get(8).is_some());
        assert!(c2.get(10).is_some());
        assert!(c2.get(11).is_none());

        c.invalidate(InvalLines::new(0, 6, 6));
        assert!(c.get(5).is_none());
        assert!(c.get(8).is_some());
        assert!(c.get(10).is_some());
        assert!(c.get(11).is_none());

        let mut c = LineRenderCache::default();
        for i in 0..10 {
            c.insert(i, i);
        }

        assert_eq!(c.base_line, 0);
        assert_eq!(c.entries.len(), 10);

        c.invalidate(InvalLines::new(0, 10, 1));
        assert!(c.get(0).is_none());
        assert_eq!(c.len(), 1);

        let mut c = LineRenderCache::default();
        for i in 0..10 {
            c.insert(i, i);
        }

        c.invalidate(InvalLines::new(5, 800, 1));
        assert!(c.get(0).is_some());
        assert!(c.get(1).is_some());
        assert!(c.get(2).is_some());
        assert!(c.get(3).is_some());
        assert!(c.get(4).is_some());
        assert_eq!(c.len(), 6);

        let mut c = LineRenderCache::default();
        for i in 5..10 {
            c.insert(i, i);
        }

        assert_eq!(c.base_line, 5);

        c.invalidate(InvalLines::new(0, 7, 1));
        assert_eq!(c.base_line, 0);
        assert!(c.get(0).is_some()); // was line 7
        assert!(c.get(1).is_some()); // was line 8
        assert!(c.get(2).is_some()); // was line 9
        assert!(c.get(3).is_none());
        assert!(c.get(4).is_none());
        assert_eq!(c.len(), 3);

        let mut c = LineRenderCache::default();
        for i in 0..10 {
            c.insert(i, i);
        }

        c.invalidate(InvalLines::new(0, 800, 1));
        assert!(c.get(0).is_none());
        assert_eq!(c.len(), 1);

        // TODO: test the contents
    }
}
