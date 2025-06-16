// src/intern_benchmark_harness/mod.rs

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
    /// This is crucial for verifying correctness and for some interning patterns,
    /// though not all "no interning" strategies would strictly need it for performance benchmarks.
    ///
    /// # Arguments
    /// * `symbol`: The interned symbol or reference.
    ///
    /// # Returns
    /// An owned [`String`] containing the original value corresponding to the
    /// symbol.
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
use lasso::{Rodeo, Spur};
// For interned-string v0.2.0, use IString
// use interned_string::IString as InternedIString; // Alias to avoid confusion if IString is too generic
// For intern_string v0.1.0, use Intern and InternId
// use intern_string::{Intern as InternStringIntern, InternId as InternStringInternId};
// For arc-string-interner
use arc_string_interner::StringInterner as ArcStringInternerImpl;
use arc_string_interner::Sym as ArcSym;

/// An implementation of `StringInternerTrait` using the project's shared `string-interner`.
pub struct SharedStringInterner {
    // Keep a reference to the global interner.
    // The global INTERNER is Arc<RwLock<StringInterner<...>>>
    // We can clone the Arc for our struct.
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
        // Acquire the write lock to intern.
        self.interner_arc.write().get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        let guard = self.interner_arc.read();
        guard
            .resolve(*symbol)
            .expect("Symbol should exist in interner")
            .to_owned()
    }
}

// Implementations for the new string interning crates

// 1. Lasso Interner (using Rodeo)
pub struct LassoInterner {
    interner: Rodeo,
}

impl LassoInterner {
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

    fn intern(&mut self, s: &str) -> Self::Symbol {
        self.interner.get_or_intern(s)
    }

    fn resolve(&self, symbol: &Self::Symbol) -> String {
        self.interner.resolve(symbol).to_string()
    }
}

// 2. InternedString Interner (using interned_string::IString for v0.2.0) - COMMENTED OUT DUE TO COMPILATION ISSUES
// pub struct InternedStringInterner {
//     _marker: std::marker::PhantomData<()>,
// }
//
// impl InternedStringInterner {
//     pub fn new() -> Self {
//         Self { _marker: std::marker::PhantomData }
//     }
// }
//
// impl Default for InternedStringInterner {
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
// impl StringInternerTrait for InternedStringInterner {
//     type Symbol = InternedIString;
//
//     fn intern(&mut self, s: &str) -> Self::Symbol {
//         // This was failing with E0425: cannot find function `intern` in crate `interned_string`
//         interned_string::intern(s)
//     }
//
//     fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
//         symbol.as_ref()
//     }
// }

// 3. intern-string Interner (using intern_string::Intern and InternId for v0.1.0) - COMMENTED OUT DUE TO COMPILATION ISSUES
// pub struct InternStringImplInterner {
//     interner: InternStringIntern,
// }
//
// impl InternStringImplInterner {
//     pub fn new() -> Self {
//         Self {
//             interner: InternStringIntern::new(),
//         }
//     }
// }
//
// impl Default for InternStringImplInterner {
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
// impl StringInternerTrait for InternStringImplInterner {
//     type Symbol = InternStringInternId;
//
//     fn intern(&mut self, s: &str) -> Self::Symbol {
//         self.interner.intern(s)
//     }
//
//     fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
//         // This was failing with E0599: no method named `resolve` found (or `get`)
//         let resolved_str = self.interner.resolve(*symbol)
//             .expect("Symbol should exist in interner");
//         Box::leak(resolved_str.to_string().into_boxed_str())
//     }
// }

// 4. ArcStringInterner Interner
pub struct ArcStringInternerImplInterner {
    // S is the Symbol type (e.g., Sym), T is the string type (e.g., String)
    // The interner itself is StringInterner<SymbolType, Hasher, CONST_N>
    // It stores strings of type String by default if not specified otherwise via another generic arg not present here.
    // The methods like get_or_intern will be generic over T: Borrow<str> + Hash + Eq + ...
    // and T will be stored as String (or specified S in StringInterner<StringStored, Sym, H, N> if API was different)
    interner: ArcStringInternerImpl<ArcSym, std::collections::hash_map::RandomState, 10>,
}

impl ArcStringInternerImplInterner {
    pub fn new() -> Self {
        Self {
            interner: ArcStringInternerImpl::with_capacity(1024), // This will create StringInterner<Sym, RandomState, 0>
        }
    }
}

impl Default for ArcStringInternerImplInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInternerTrait for ArcStringInternerImplInterner {
    type Symbol = ArcSym; // The symbol type used by the interner

    fn intern(&mut self, s: &str) -> Self::Symbol {
        // arc_string_interner stores T (e.g. String), interns it, and returns S (e.g. Sym)
        // The method is get_or_intern(val: T) -> S
        // By default T is String if StringInterner is StringInterner<Sym, RandomState, N>
        self.interner.get_or_intern(s.to_string())
    }

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
/// It stores and returns owned Strings. This serves as a baseline.
#[derive(Default)]
pub struct NoInterningBaseline {
    // No shared state needed for this baseline, as each "interned" string
    // is just an owned copy. If we needed to resolve symbols that are not
    // &'a str themselves, we might need a Vec<String> here, but since
    // Self::Symbol is String, resolve can just return a ref to the symbol.
}

impl NoInterningBaseline {
    pub fn new() -> Self {
        Self {}
    }
}

impl StringInternerTrait for NoInterningBaseline {
    // For the no-interning baseline, the "symbol" is the String itself.
    type Symbol = String;

    fn intern(&mut self, s: &str) -> Self::Symbol {
        // "Interning" here simply means creating an owned copy of the string.
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
            .to_owned()
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
            .to_owned()
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
            .to_owned()
    }
}
