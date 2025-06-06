// src/interner.rs

/// A trait for abstracting string interning operations.
/// This allows for benchmarking different interning strategies.
pub trait StringInternerTrait {
    /// Represents the type of the interned string symbol or reference.
    /// For no interning, this could be String itself or Arc<String>.
    /// For `string-interner`, this would be its Symbol type.
    type Symbol: Clone + Eq + std::hash::Hash + std::fmt::Debug;

    /// Interns a string slice and returns a symbol or reference.
    ///
    /// # Arguments
    /// * `s`: The string slice to intern.
    ///
    /// # Returns
    /// An interned representation of the string.
    fn intern(&mut self, s: &str) -> Self::Symbol;

    /// Resolves/retrieves the original string from its interned representation.
    ///
    /// # Arguments
    /// * `symbol`: The interned symbol or reference.
    ///
    /// # Returns
    /// The original string corresponding to the symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String;
}

// use crate::drains::simple::INTERNER as SHARED_INTERNER; // Access the global interner - REMOVED
use parking_lot::RwLock; // This is still used by other interners if they were here, but SharedStringInterner is removed
use std::collections::hash_map::RandomState; // Used by other interners
use std::sync::Arc; // Used by other interners (potentially)
use string_interner::backend::{BucketBackend, BufferBackend, StringBackend}; // Import backends
use string_interner::DefaultSymbol;
use string_interner::StringInterner; // Import RandomState

/*
// SharedStringInterner is commented out as it relied on the global INTERNER from drains::simple
pub struct SharedStringInterner {
    interner_arc: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>>,
}

impl SharedStringInterner {
    pub fn new() -> Self {
        Self {
            interner_arc: SHARED_INTERNER.clone(),
        }
    }
}

impl StringInternerTrait for SharedStringInterner {
    type Symbol = DefaultSymbol;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner_arc.write().get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner_arc
            .read()
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_string()
    }
}

// Add a default impl for SharedStringInterner
impl Default for SharedStringInterner {
    fn default() -> Self {
        Self::new()
    }
}
*/

/// An implementation of `StringInternerTrait` that does no actual interning.
/// It stores and returns owned Strings. This serves as a baseline.
#[derive(Default)]
pub struct NoInterningBaseline {
    // No shared state needed
}

impl NoInterningBaseline {
    pub fn new() -> Self {
        Self {}
    }
}

impl StringInternerTrait for NoInterningBaseline {
    type Symbol = String;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        s.to_string()
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        symbol.clone()
    }
}

// 1. StringBackendInterner (explicit, fresh instance)
pub struct StringBackendInterner {
    interner: StringInterner<StringBackend, RandomState>,
}

impl StringBackendInterner {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::<StringBackend, RandomState>::new(),
        }
    }
}

impl Default for StringBackendInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for StringBackendInterner {
    type Symbol = DefaultSymbol;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_string()
    }
}

// 2. BucketBackendInterner
pub struct BucketBackendInterner {
    interner: StringInterner<BucketBackend, RandomState>,
}

impl BucketBackendInterner {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::<BucketBackend, RandomState>::new(),
        }
    }
}

impl Default for BucketBackendInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for BucketBackendInterner {
    type Symbol = DefaultSymbol;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_string()
    }
}

// 3. BufferBackendInterner
pub struct BufferBackendInterner {
    interner: StringInterner<BufferBackend, RandomState>,
}

impl BufferBackendInterner {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::<BufferBackend, RandomState>::new(),
        }
    }
}

impl Default for BufferBackendInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for BufferBackendInterner {
    type Symbol = DefaultSymbol;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_string()
    }
}
