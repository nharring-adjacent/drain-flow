// src/intern_benchmark_harness/mod.rs

/// A trait for abstracting string interning operations.
///
/// This trait allows for benchmarking different string interning strategies
/// by providing a common interface for interning and resolving strings.
pub trait StringInternerTrait {
    /// The type representing an interned string symbol or reference.
    ///
    /// For no interning, this could be `String` itself or `Arc<String>`.
    /// For the `string-interner` crate, this would be its `Symbol` type.
    type Symbol: Clone + Eq + std::hash::Hash + std::fmt::Debug;

    /// Interns a string slice and returns its symbolic representation.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The interned representation of the string.
    fn intern(&mut self, s: &str) -> Self::Symbol;

    /// Resolves an interned symbol back to its original string value.
    ///
    /// This method is crucial for verifying correctness and for certain interning
    /// patterns, although not all "no interning" strategies would strictly need
    /// it for performance benchmarks.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The interned symbol or reference to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` containing the original value corresponding to the symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String;
}

use crate::drains::simple::INTERNER as SHARED_INTERNER; // Access the global interner
use parking_lot::RwLock;
use std::collections::hash_map::RandomState;
use std::sync::Arc;
use string_interner::backend::{BucketBackend, BufferBackend, StringBackend}; // Import backends
use string_interner::DefaultSymbol;
use string_interner::StringInterner; // Import RandomState

// New crates for benchmarking
use interned_string::{IString, Intern};
use lasso::{Rodeo, Spur};
// For intern_string v0.1.0, use Intern and InternId
use intern_string::{Intern as InternStringIntern, InternId as InternStringInternId};
// For arc-string-interner
use arc_string_interner::StringInterner as ArcStringInternerImpl;
use arc_string_interner::Sym as ArcSym;

/// An implementation of `StringInternerTrait` using the project's shared `string-interner`.
///
/// This struct provides a way to interact with the global, shared string interner
/// (`simple::INTERNER`) for benchmarking purposes.
pub struct SharedStringInterner {
    /// A reference to the global `StringInterner` instance.
    interner_arc: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>>,
}

impl SharedStringInterner {
    /// Creates a new `SharedStringInterner` instance.
    ///
    /// This constructor clones the `Arc` to the global interner, allowing multiple
    /// `SharedStringInterner` instances to share the same underlying interner.
    ///
    /// # Returns
    ///
    /// A new `SharedStringInterner` instance.
    pub fn new() -> Self {
        Self {
            interner_arc: SHARED_INTERNER.clone(),
        }
    }
}

impl StringInternerTrait for SharedStringInterner {
    type Symbol = DefaultSymbol;

    /// Interns a string slice using the shared `string-interner`.
    ///
    /// This method acquires a write lock on the shared interner to perform the
    /// interning operation.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `DefaultSymbol` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner_arc.write().get_or_intern(s)
    }

    /// Resolves a `DefaultSymbol` back to its original string using the shared `string-interner`.
    ///
    /// This method acquires a read lock on the shared interner to perform the
    /// resolution operation.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `DefaultSymbol` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    ///
    /// # Panics
    ///
    /// Panics if the symbol cannot be resolved, which indicates an inconsistency
    /// (e.g., a symbol was created by an interner other than the shared one).
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        let guard = self.interner_arc.read();
        guard
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_owned()
    }
}

// Implementations for the new string interning crates

/// An implementation of `StringInternerTrait` using the `lasso` crate's `Rodeo` interner.
pub struct LassoInterner {
    interner: Rodeo,
}

impl LassoInterner {
    /// Creates a new `LassoInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `LassoInterner` instance.
    pub fn new() -> Self {
        Self {
            interner: Rodeo::new(),
        }
    }
}

impl Default for LassoInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for LassoInterner {
    type Symbol = Spur;

    /// Interns a string slice using the `lasso` interner.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `Spur` symbol representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    /// Resolves a `Spur` symbol back to its original string using the `lasso` interner.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `Spur` symbol to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner.resolve(symbol).to_string()
    }
}

/// An implementation of `StringInternerTrait` using the `interned-string` crate's `IString`.
pub struct InternedStringInterner;

impl InternedStringInterner {
    /// Creates a new `InternedStringInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `InternedStringInterner` instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for InternedStringInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for InternedStringInterner {
    type Symbol = IString;

    /// Interns a string slice using `IString::intern()`.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `IString` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        s.intern()
    }

    /// Resolves an `IString` back to its original string.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `IString` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        symbol.as_ref().to_string()
    }
}

/// An implementation of `StringInternerTrait` using the `intern-string` crate.
pub struct InternStringImplInterner {
    interner: InternStringIntern<'static>,
}

impl InternStringImplInterner {
    /// Creates a new `InternStringImplInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `InternStringImplInterner` instance.
    pub fn new() -> Self {
        Self {
            interner: InternStringIntern::new(),
        }
    }
}

impl Default for InternStringImplInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for InternStringImplInterner {
    type Symbol = InternStringInternId;

    /// Interns a string slice using the `intern-string` interner.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `InternStringInternId` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.intern(s)
    }

    /// Resolves an `InternStringInternId` back to its original string.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `InternStringInternId` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner.lookup(*symbol).to_string()
    }
}

/// An implementation of `StringInternerTrait` using the `arc-string-interner` crate.
pub struct ArcStringInternerImplInterner {
    interner: ArcStringInternerImpl<ArcSym, std::collections::hash_map::RandomState, 10>,
}

impl ArcStringInternerImplInterner {
    /// Creates a new `ArcStringInternerImplInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `ArcStringInternerImplInterner` instance.
    pub fn new() -> Self {
        Self {
            interner: ArcStringInternerImpl::with_capacity(1024), 
        }
    }
}

impl Default for ArcStringInternerImplInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for ArcStringInternerImplInterner {
    type Symbol = ArcSym; 

    /// Interns a string slice using the `arc-string-interner`.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `ArcSym` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s.to_string())
    }

    /// Resolves an `ArcSym` back to its original string.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `ArcSym` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        let arc_str_val: Arc<str> = self
            .interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner");
        arc_str_val.to_string()
    }
}

// Add a default impl for SharedStringInterner
impl Default for SharedStringInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// An implementation of `StringInternerTrait` that does no actual interning.
///
/// This struct serves as a baseline for benchmarking, as it simply stores and
/// returns owned `String`s without any interning optimization.
#[derive(Default)]
pub struct NoInterningBaseline {
    // No shared state needed for this baseline, as each "interned" string
    // is just an owned copy. If we needed to resolve symbols that are not
    // &'a str themselves, we might need a Vec<String> here, but since
    // Self::Symbol is String, resolve can just return a ref to the symbol.
}

impl NoInterningBaseline {
    /// Creates a new `NoInterningBaseline` instance.
    ///
    /// # Returns
    ///
    /// A new `NoInterningBaseline` instance.
    pub fn new() -> Self {
        Self {}
    }
}

impl StringInternerTrait for NoInterningBaseline {
    /// The "symbol" type for this baseline is `String` itself, as no interning occurs.
    type Symbol = String;

    /// "Interns" a string slice by creating an owned copy.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to "intern".
    ///
    /// # Returns
    ///
    /// An owned `String` copy of the input slice.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        s.to_string()
    }

    /// Resolves a `String` symbol by simply cloning it.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `String` to resolve.
    ///
    /// # Returns
    ///
    /// A cloned `String` corresponding to the input symbol.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        symbol.clone()
    }
}

// 1. StringBackendInterner (explicit, fresh instance)
/// An implementation of `StringInternerTrait` using `string_interner::backend::StringBackend`.
///
/// This interner uses a `StringBackend` for storage, which is suitable for general-purpose
/// string interning where strings are stored directly.
pub struct StringBackendInterner {
    interner: StringInterner<StringBackend, RandomState>,
}

impl StringBackendInterner {
    /// Creates a new `StringBackendInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `StringBackendInterner` instance.
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

    /// Interns a string slice using the `StringBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `DefaultSymbol` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    /// Resolves a `DefaultSymbol` back to its original string using the `StringBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `DefaultSymbol` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    ///
    /// # Panics
    ///
    /// Panics if the symbol cannot be resolved, which indicates an inconsistency.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_owned()
    }
}

/// An implementation of `StringInternerTrait` using `string_interner::backend::BucketBackend`.
///
/// This interner uses a `BucketBackend` for storage, which is optimized for scenarios
/// where strings are grouped into buckets based on their hash, potentially reducing
/// collision and improving lookup times for certain data distributions.
pub struct BucketBackendInterner {
    interner: StringInterner<BucketBackend, RandomState>,
}

impl BucketBackendInterner {
    /// Creates a new `BucketBackendInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `BucketBackendInterner` instance.
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

    /// Interns a string slice using the `BucketBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `DefaultSymbol` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    /// Resolves a `DefaultSymbol` back to its original string using the `BucketBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `DefaultSymbol` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    ///
    /// # Panics
    ///
    /// Panics if the symbol cannot be resolved, which indicates an inconsistency.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_owned()
    }
}

/// An implementation of `StringInternerTrait` using `string_interner::backend::BufferBackend`.
///
/// This interner uses a `BufferBackend` for storage, which is designed for efficient
/// storage of strings in a contiguous buffer, potentially offering better cache locality
/// and performance for certain access patterns.
pub struct BufferBackendInterner {
    interner: StringInterner<BufferBackend, RandomState>,
}

impl BufferBackendInterner {
    /// Creates a new `BufferBackendInterner` instance.
    ///
    /// # Returns
    ///
    /// A new `BufferBackendInterner` instance.
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

    /// Interns a string slice using the `BufferBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `s` - The string slice to intern.
    ///
    /// # Returns
    ///
    /// The `DefaultSymbol` representing the interned string.
    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    /// Resolves a `DefaultSymbol` back to its original string using the `BufferBackend` interner.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `DefaultSymbol` to resolve.
    ///
    /// # Returns
    ///
    /// An owned `String` corresponding to the resolved symbol.
    ///
    /// # Panics
    ///
    /// Panics if the symbol cannot be resolved, which indicates an inconsistency.
    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_owned()
    }
}
