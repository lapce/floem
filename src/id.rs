use std::num::NonZeroU64;

thread_local! {
pub(crate)    static IDPATHS: std::cell::RefCell<std::collections::HashMap<Id,IdPath>>  = Default::default();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
pub struct Id(NonZeroU64);

#[derive(Clone, Default)]
pub struct IdPath(pub(crate) Vec<Id>);

impl IdPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_id(&self) -> Id {
        let id = Id::next();
        self.with_id(id);
        id
    }

    pub fn with_id(&self, id: Id) -> IdPath {
        let mut ids = self.clone();
        ids.0.push(id);
        IDPATHS.with(|id_paths| {
            id_paths.borrow_mut().insert(id, ids.clone());
        });
        ids
    }
}

impl Id {
    /// Allocate a new, unique `Id`.
    pub fn next() -> Id {
        use glazier::Counter;
        static WIDGET_ID_COUNTER: Counter = Counter::new();
        Id(WIDGET_ID_COUNTER.next_nonzero())
    }

    pub fn new(&self) -> Id {
        let mut id_path =
            IDPATHS.with(|id_paths| id_paths.borrow().get(self).cloned().unwrap_or_default());
        let new_id = Self::next();
        id_path.0.push(new_id);
        IDPATHS.with(|id_paths| {
            id_paths.borrow_mut().insert(new_id, id_path);
        });
        new_id
    }

    #[allow(unused)]
    pub fn to_raw(self) -> u64 {
        self.0.into()
    }

    pub fn to_nonzero_raw(self) -> NonZeroU64 {
        self.0
    }
}
