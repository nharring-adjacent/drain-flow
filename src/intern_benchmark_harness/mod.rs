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
use string_interner::DefaultSymbol;
use std::sync::Arc;
use parking_lot::RwLock;
use string_interner::StringInterner;

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
        let resolved_str = guard.resolve(*symbol).expect("Symbol should exist in interner");
        // "Leak" the string to get a 'static reference.
        // This is not truly 'a, but 'static. It will satisfy the compiler for 'a.
        Box::leak(resolved_str.to_string().into_boxed_str())
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
