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
    /// The original string slice corresponding to the symbol.
    /// For "no interning" with `String` as symbol, it might return `&symbol`.
    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str;
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        // Acquire the read lock to resolve.
        // This is tricky because the StringInterner.resolve method returns a &str
        // that is tied to the lifetime of the &StringInterner, which is itself
        // behind an RwLockReadGuard. We cannot return a reference that outlives the guard.
        // This is a common issue with trying to wrap interners that return temporary refs.
        //
        // For a benchmark, one option is to make resolve return String.
        // Or, for this specific case where resolve is mostly for verification/debug,
        // and the benchmark focuses on `intern`, we might accept a limitation or
        // use unsafe code if we were sure about lifetimes (but let's avoid unsafe).
        //
        // A practical solution for benchmarking might be that `resolve` is not called
        // in the hot loop of the benchmark, or it returns an owned string.
        // Let's try to make it work by ensuring the returned str is from a stable location
        // that the guard protects. The string data is owned by the interner's allocator.
        //
        // The problem:
        // let guard = self.interner_arc.read();
        // guard.resolve(*symbol).expect("Symbol should exist") // This returns a &str tied to `guard`
        //
        // This will require careful handling. A common pattern is to use a crate like `owning_ref`
        // or to accept that `resolve` might be less performant or return `String`.
        //
        // Given the constraints and the goal (benchmarking `intern`), let's make `resolve`
        // also acquire the lock and return a string slice. The lifetime 'a will be tied
        // to the lifetime of `&'a self` and `&'a Self::Symbol`.
        // The `string-interner::StringInterner`'s `resolve` method itself returns `Option<&str>`.
        // The lifetime of the returned `&str` is tied to the `&self` of the `resolve` call,
        // which is the `StringInterner` instance.
        //
        // This means we need a way to hold the RwLockReadGuard and return a reference tied to it.
        // This is not directly possible if `resolve` must return `&'a str` where `'a` is tied to `&'a self`.
        //
        // Workaround for the benchmark:
        // The simplest way to satisfy the trait's current signature for `resolve`
        // and avoid lifetime issues with the lock guard is to leak the string or use a static buffer.
        // Leaking is bad. A static buffer is also not great.
        //
        // Let's reconsider the `resolve` signature or its usage in the benchmark.
        // If `resolve` is primarily for setup/verification, we could have it return `String`.
        //
        // For now, to make it compile and be usable, we can temporarily use a method that
        // might not be ideal for high-performance resolve, but works.
        // The string_interner stores strings in a backend. When resolve is called,
        // it provides a reference to that stored string. The guard must be held.
        //
        // A simple (but potentially problematic if the string is immediately dropped) way:
        // This is UNSAFE if the guard is dropped and the &str is still used.
        // To do this safely, resolve would need to return something like `OwningRef<RwLockReadGuard<...>, str>`.
        //
        // Let's make a simplifying assumption for the benchmark: `resolve` is not on the critical path.
        // We can clone the string and leak it, returning a 'static str. This is bad practice generally
        // but unblocks for the benchmark trait.
        // A better approach for the trait would be for resolve to take `&'a self` and return `Cow<'a, str>`
        // or for the benchmark to accept that `resolve` might allocate.
        //
        // Given the current trait: `fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str;`
        // This implies the returned `&str` lives as long as `self` or `symbol`.
        // This is possible if `Self::Symbol` itself contains the `&str` or `String`.
        // But `DefaultSymbol` is usually a number.
        //
        // The most straightforward way to fulfill the trait for `string-interner`
        // is to recognize that the `resolve` method of the `StringInterner` object
        // returns a `&str` whose lifetime is tied to the `StringInterner` instance itself
        // (because the strings are stored within it).
        // So, if we hold a read lock, the reference is valid as long as the lock is held.
        // The issue is returning that reference *outside* the scope of the lock.
        //
        // This is a fundamental challenge with this trait signature for `resolve`.
        // Let's try to return a string that is effectively 'static by leaking memory.
        // This is ONLY for the benchmark context to satisfy the trait.
        // **This is generally a bad idea for production code.**
        let guard = self.interner_arc.read();
        let resolved_str = guard
            .resolve(*symbol)
            .expect("Symbol should exist in interner");
        // "Leak" the string to get a 'static reference.
        // This is not truly 'a, but 'static. It will satisfy the compiler for 'a.
        Box::leak(resolved_str.to_string().into_boxed_str())
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        // Rodeo's resolve method returns &'a str where 'a is tied to &self.interner
        // This matches the trait signature's requirement if we consider 'a to be tied to &'a self.
        self.interner.resolve(symbol)
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
    interner: ArcStringInternerImpl<ArcSym, std::collections::hash_map::RandomState, 0>,
}

impl ArcStringInternerImplInterner {
    pub fn new() -> Self {
        Self {
            interner: ArcStringInternerImpl::new(), // This will create StringInterner<Sym, RandomState, 0>
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        // resolve(symbol: S) -> Option<Arc<T>> where T is str by default for arc_string_interner
        let arc_str_val: Arc<str> = self.interner.resolve(*symbol)
            .expect("Symbol should exist in interner");
        // To use into_boxed_str() for Box::leak, we need a String.
        let owned_string: String = arc_str_val.to_string();
        Box::leak(owned_string.into_boxed_str())
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        // The "symbol" is the string itself, so we just return a reference to it.
        // The lifetime 'a is tied to the input 'a Self::Symbol, which is &'a String.
        // So returning &'a str (from &'a String) is valid.
        symbol.as_str()
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        let resolved_str = self
            .interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner");
        Box::leak(resolved_str.to_string().into_boxed_str())
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        let resolved_str = self
            .interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner");
        Box::leak(resolved_str.to_string().into_boxed_str())
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

    fn resolve<'a>(&'a self, symbol: &'a Self::Symbol) -> &'a str {
        let resolved_str = self
            .interner
            .resolve(*symbol)
            .expect("Symbol should exist in interner");
        Box::leak(resolved_str.to_string().into_boxed_str())
    }
}
