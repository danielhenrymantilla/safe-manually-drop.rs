//! [SafeManuallyDrop]: `SafeManuallyDrop`
//! [`SafeManuallyDrop`]: `SafeManuallyDrop`
//! [`SafeManuallyDrop::into_inner_defusing_impl_Drop()`]: `SafeManuallyDrop::into_inner_defusing_impl_Drop()`
//! [`DropManually`]: `DropManually`
//! [`DropManually::drop_manually()`]: `DropManually::drop_manually()`
//! [`appendix`]: `appendix`
//!
//! [`ManuallyDrop`]: `ManuallyDrop`
//! [`::core::ops::Deref`]: `::core::ops::Deref`
//! [`::core::ops::DerefMut`]: `::core::ops::DerefMut`
//! [`Drop`]: `Drop`
//! [`Option`]: `Option`
//! [`From::from()`]: `From::from()`
#![doc = include_str!("../README.md")]
#![cfg_attr(not(doc), no_std)]
#![allow(unused_braces)]
#![cfg_attr(feature = "better-docs", feature(doc_cfg, negative_impls))]

use ::core::{
    marker::PhantomData as PD,
    mem::{ManuallyDrop, ManuallyDrop as MD},
};

#[cfg(doc)]
#[cfg_attr(feature = "better-docs", doc(cfg(doc)))]
#[doc = include_str!("appendix.md")]
pub mod appendix {}

/// The crate's prelude.
pub
mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        DropManually,
        SafeManuallyDrop,
    };
}

/// The main/whole point of this whole crate and design: to expose _owned_ access to a `FieldTy`
/// when drop glue is being run.
///
///  1. A [`SafeManuallyDrop<FieldTy, ContainingType>`],
///  1. with a (mandatory)
///     <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>,
///  1. once it gets dropped / during its drop glue (_e.g._, from within a `ContainingType`),
///  1. shall be running the [`DropManually::drop_manually()`] logic on that _owned_ `FieldTy`.
///
/// ```rust
/// # use drop as stuff;
/// use ::safe_manually_drop::SafeManuallyDrop;
///
/// struct Frobnicator {
///     // …
/// }
///
/// struct Example {
///     //    ^
///     //    +--------------------------------+
///     //                                     |
///     string: SafeManuallyDrop<Frobnicator, Self>,
/// } //                                       |
/// //                                         +-------------vvvvvvv
/// impl ::safe_manually_drop::DropManually<Frobnicator> for Example {
///     fn drop_manually(owned_frobnicator: Frobnicator) {
///         //  owned access! 👆
///         stuff(owned_frobnicator)
///     }
/// }
/// ```
///
/// In practice, this becomes _the_ handy, 0-runtime-overhead, non-`unsafe`, tool to get owned
/// access to a `struct`'s field (or group thereof) during drop glue.
///
/// Indeed, the recipe then becomes:
///
///  1. Use, instead of a `field: FieldTy`, a wrapped
///     <code>field: [SafeManuallyDrop]\<FieldTy, ContainingType\></code>,
///
///       - (Usually `Self` can be used instead of having to spell out, _verbatim_, the
///         `ContainingType`.)
///
///       - (This wrapper type offers transparent
///         <code>[Deref][`::core::ops::Deref`]{,[Mut][`::core::ops::DerefMut`]}</code>, as well as
///         [`From::from()`] and ["`.into()`"][`SafeManuallyDrop::into_inner_defusing_impl_Drop()`]
///         conversions.)
///
///  1. then, provide the companion, mandatory,
///     <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>
///
///  1. Profit™ from the owned access to `FieldTy` inside of [`DropManually::drop_manually()`]'s
///     body.
///
/// ### "`OverrideDropGlue`" rather than `PrependDropGlue`
///
/// Note that this new drop glue logic for `FieldTy`, defined in [`DropManually::drop_manually()`],
/// shall _supersede_ / override its default drop glue.
///
/// ```rust
/// # use drop as stuff;
/// use ::safe_manually_drop::prelude::*;
///
/// pub struct MyType(SafeManuallyDrop<String, Self>);
///
/// impl DropManually<String> for MyType {
///     fn drop_manually(s: String) {
///         // Notice the fully owned access to `s`.
///         stuff(s)
///         // Notably, if `s` is not moved out / "consumed",
///         // then `s`' own drop glue (e.g., here, that of `String`) will be automagically
///         // *implicitly* invoked when `s` goes out of scope at the end of the block.
///         //
///         // This may appear rather similar to the classic `Drop` trait, but there is
///         // a huge difference: this function body had to *allow* for the implicit
///         // drop and fall-out-of-scope to happen, by not having *consumed* the owned
///         // `s: String` in some other way.
///         //
///         // To illustrate, the body could very well, for instance, `mem::forget(s)`,
///         // and no `drop` of the `string` would happen, which is not something
///         // *directly* doable with the `Drop` trait.
///     }
/// }
///
/// fn example(it: MyType) {
///     drop(it); // invokes the drop glue of `MyType`,
///               // including the `Drop` impl `for SafeManuallyDrop<String, MyType>`
///               // i.e., `<MyType as DropManually<String>>::drop_manually()`.
/// }
/// ```
///
/// For instance: if, inside of [`DropManually::drop_manually()`], the `FieldTy` is
/// [`::core::mem::forget()`]ten, then `FieldTy`'s own drop glue shall never actually run, much like
/// when a [`ManuallyDrop<FieldTy>`] is [`drop()`]-ped/discarded.
///
/// With that being said, precisely because [`DropManually::drop_manually()`] receives an owned
/// instance of `FieldTy`, this behavior is rather "opt-out": that `FieldTy` owned instance runs
/// out of scope when the function completes, so it will almost always get "dropped" / have its own
/// drop glue being invoked.
///
/// The only exceptions are then when other, ownership-consuming, functions, get called on this
/// value.
///
/// Typically:
///
///   - [`::core::mem::forget()`] to skip/bypass all of the drop glue altogether.
///       - note that a direct [`ManuallyDrop<FieldTy>`] would probably be better in this instance;
///   - an `impl FnOnce()` getting called (the `()`-call consumes ownership);
///   - an owned argument `S` is fed to a function, such as `S` in an `impl FnOnce(S)`;
///   - types using owned type-state patterns, most notably `Transaction::{commit,roll_back}()`.
#[diagnostic::on_unimplemented(
    note = "\
In order for a struct/enum to contain a `SafeManuallyDrop<FieldTy, …>` field:

 1. `…`, the second type parameter, ought to be `Self`, i.e., the containing `struct/enum` \
    wherein the provided `DropManually` logic makes sense.

    For instance:

    ```rust
    field: SafeManuallyDrop<FieldTy, Self>,
    ```

 2. you then have to provide an \
    `impl<…> DropManually<FieldTy> for <the containing struct/enum> {{`.

    For instance:

    ```rust
    impl<…> DropManually<FieldTy> for StructName<…> {{
    ```
\
    ",
)]
pub
trait DropManually<FieldTy> {
    fn drop_manually(_: FieldTy);
}

/// [`SafeManuallyDrop<FieldTy>`] is the safe counterpart of [`ManuallyDrop<FieldTy>`], and the
/// zero-runtime-overhead counterpart of [`Option<FieldTy>`].
///
///   - A [`SafeManuallyDrop<FieldTy, ContainingType>`],
///   - with a (mandatory)
///     <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>,
///   - once it gets dropped / during its drop glue (_e.g._, from within a `ContainingType`),
///   - shall be running the [`DropManually::drop_manually()`] logic on that _owned_ `FieldTy`.
///
/// ```rust
/// # use drop as stuff;
/// use ::safe_manually_drop::SafeManuallyDrop;
///
/// struct Frobnicator {
///     // …
/// }
///
/// struct Example {
///     //    ^
///     //    +--------------------------------+
///     //                                     |
///     string: SafeManuallyDrop<Frobnicator, Self>,
/// } //                                       |
/// //                                         +-------------vvvvvvv
/// impl ::safe_manually_drop::DropManually<Frobnicator> for Example {
///     fn drop_manually(owned_frobnicator: Frobnicator) {
///         //  owned access! 👆
///         stuff(owned_frobnicator)
///     }
/// }
/// ```
///
/// ## Examples
///
///   - Using [`Option<FieldTy>`]: [`.unwrap()`][`Option::unwrap()`]s everywhere 🤢
///
///     <details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>
///
///     ```rust
///     #![forbid(unsafe_code)]
///
///     struct DeferGuardFields<T, F : FnOnce(T)> {
///         value: T,
///         on_drop: F,
///     }
///
///     pub
///     struct DeferGuard<T, F : FnOnce(T)>(
///         Option<DeferGuardFields<T, F>>,
///     );
///
///     impl<T, F : FnOnce(T)> Drop for DeferGuard<T, F> {
///         fn drop(&mut self) {
///             let DeferGuardFields {
///                 value,
///                 on_drop,
///             } = self.0.take().unwrap(); // 🤢
///             on_drop(value);
///         }
///     }
///
///     impl<T, F : FnOnce(T)> ::core::ops::Deref for DeferGuard<T, F> {
///         type Target = T;
///
///         fn deref(&self) -> &T {
///             &self
///                 .0
///                 .as_ref()
///                 .unwrap() // 🤮
///                 .value
///         }
///     }
///     // And `DerefMut`
///     ```
///
///     </details>
///
///   - Using [`ManuallyDrop<FieldTy>`]: `unsafe`! 😱
///
///     <details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>
///
///     ```rust
///     #![deny(unsafe_code)] // require visible `#[allow()]`s in subtle functions.
///
///     use ::core::mem::ManuallyDrop;
///
///     pub
///     struct DeferGuard<T, F : FnOnce(T)> {
///         value: ManuallyDrop<T>,
///         on_drop: ManuallyDrop<F>,
///     }
///
///     impl<T, F : FnOnce(T)> Drop for DeferGuard<T, F> {
///         fn drop(&mut self) {
///             #[allow(unsafe_code)] {
///                 let value = unsafe { // 😰
///                     ManuallyDrop::take(&mut self.value)
///                 };
///                 let on_drop = unsafe { // 😰
///                     ManuallyDrop::take(&mut self.on_drop)
///                 };
///                 on_drop(value);
///             }
///         }
///     }
///
///     impl<T, F : FnOnce(T)> ::core::ops::Deref for DeferGuard<T, F> {
///         type Target = T;
///
///         fn deref(&self) -> &T {
///             &self.value
///         }
///     }
///     // And `DerefMut`
///     ```
///
///     </details>
///
///   - Using [`SafeManuallyDrop<FieldTy, …>`][`SafeManuallyDrop`]: no `unsafe`, no `.unwrap()`s!
///
///     ```rust
///     #![forbid(unsafe_code)]
///
///     use ::safe_manually_drop::{DropManually, SafeManuallyDrop};
///
///     struct DeferGuardFields<T, F : FnOnce(T)> {
///         value: T,
///         on_drop: F,
///     }
///
///     pub
///     struct DeferGuard<T, F : FnOnce(T)>(
///      // rather than `Option<DeferGuardFields<T, F>>`,
///      // or `ManuallyDrop<DeferGuardFields<T, F>>`, use:
///         SafeManuallyDrop<DeferGuardFields<T, F>, Self>,
///     );
///
///     impl<T, F : FnOnce(T)>
///         DropManually<DeferGuardFields<T, F>>
///     for
///         DeferGuard<T, F>
///     {
///         fn drop_manually(
///             DeferGuardFields { value, on_drop }: DeferGuardFields<T, F>,
///         )
///         {
///             on_drop(value);
///         }
///     }
///
///     impl<T, F : FnOnce(T)> ::core::ops::Deref for DeferGuard<T, F> {
///         type Target = T;
///
///         fn deref(&self) -> &T {
///             &self.0.value
///         }
///     }
///     // And `DerefMut`
///     ```
///
/// ## Explanation
///
/// It manages to be non-`unsafe`, w.r.t. [`ManuallyDrop<FieldTy>`], by virtue of having a
/// significantly more restricted use case: that of being used as a `struct`[^or_enum]'s field,
/// and merely **exposing _owned_ access to the `FieldTy` on _drop_**.
///
/// [^or_enum]: (or `enum`, but for the remainder of the explanation, I will stick to talking of
/// `struct`s exclusively, since it's simpler.)
///
/// Such owned access, and _drop_ logic, is exposed and defined in the companion
/// [`DropManually<FieldTy>`] trait.
///
/// In such an `impl`, you shall only have access to that `FieldTy`:
///
///   - no access to sibling field types,
///
///     (this can be trivially worked around by bundling all the necessary fields together inside
///     the [`SafeManuallyDrop<_>`]; _c.f._ the example above with the `DeferGuardFields` helper
///     definition;)
///
///   - nor to the encompassing `struct` altogether.
///
/// The latter is kind of problematic, since the desired drop glue logic is probably strongly tied
/// to such encompassing `struct`.
///
/// Hence that second generic type parameter on [`SafeManuallyDrop<FieldTy, ContainingType>`].
///
/// As its name indicates, it is expected to be the containing/encompassing `struct`:
///
/// ```rust
/// use ::safe_manually_drop::SafeManuallyDrop;
///
/// struct Example {
///     //       ^
///     //       +-----------------------+
///     //                               |
///     string: SafeManuallyDrop<String, Self>,
/// }
/// #
/// # impl ::safe_manually_drop::DropManually<String> for Example {
/// #     fn drop_manually(_: String) {}
/// # }
/// ```
///
/// That way, this containing `struct` can be used as the `Self`/`impl`ementor type for the drop
/// glue:
///
/// ```rust
/// use ::safe_manually_drop::DropManually;
///
/// # struct Example {
/// #    //       ^
/// #    //       +-----------------------+
/// #    //                               |
/// #    string: ::safe_manually_drop::SafeManuallyDrop<String, Self>,
/// # }
/// #
/// impl DropManually<String> for Example {
///     fn drop_manually(s: String) {
///         // owned access to `s` here!
///         # let random = || true; // determined by faire dice roll.
///         if random() {
///             drop(s);
///         } else {
///             ::core::mem::forget(s);
///         }
///     }
/// }
/// ```
///
/// ## Going further
///
/// ### `repr()` guarantee.
///
/// This type is guaranteed to be a mere `#[repr(transparent)]` wrapper around its `FieldTy`.
///
/// ### The second type parameter is an orphan rules "named `impl` identifier"
///
/// This section may be a bit of a "brain fuck", and it is safe to skip.
///
/// <details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>
///
/// In practice, neither the API of this crate, nor that of any non-macro API for that matter, can
/// ever hope to check, impose, nor control that the `ContainingType` used for a
/// [`SafeManuallyDrop<FieldTy, ContainingType>`] do match that of the containing `struct`.
///
/// And, as a matter of fact, there may even be legitimate cases where you may do so on purpose.
///
/// Indeed, this extra type parameter is, at the end of the day, a mere `impl DropManually`
/// "identifier" / "named impl" / type-level discriminant for it to be possible for anybody to write
/// such impls for arbitrary `FieldTy` types, even when the `FieldTy` is a fully
/// unconstrained/blanket `<T>/<F>` generic type, and/or when it stems from an upstream crate,
/// or even when wanting to repeat `SafeManuallyDrop<FieldTy, …>` multiple types within the same
/// `struct`, _&&without running afoul the orphan rules**_.
///
/// In such a case, you may want distinct drop logic for one field _vs._ another.
///
/// If so, then consider/notice how what that `ContainingType` _actually_ is, is rather a
/// `DropImplIdentifier/DropImplDiscriminant/DropStrategy` mere `PhantomData`-like type parameter.
///
/// Which means that in this context, you will likely want to involve dedicated phantom types for
/// the `ContainingType, FieldIdentifier` pair:
///
/// ```rust
/// use ::safe_manually_drop::prelude::*;
///
/// use some_lib::Transaction;
/// // where `some_lib` has the following API, say:
/// mod some_lib {
///     pub struct Transaction(());
///
///     // Owned `self` receivers for stronger type-level guarantees.
///     impl Transaction {
///         pub fn commit(self) {}
///         pub fn roll_back(self) {}
///     }
/// }
///
/// enum MyType {
///     AutoCommitOnDrop {
///         txn: SafeManuallyDrop<Transaction, CommitOnDropStrategy>,
///     },
///
///     AutoRollBackOnDrop {
///         txn: SafeManuallyDrop<Transaction, RollBackOnDropStrategy>,
///     },
/// }
///
/// enum CommitOnDropStrategy {}
/// impl DropManually<Transaction> for CommitOnDropStrategy {
///     fn drop_manually(txn: Transaction) {
///         txn.commit();
///     }
/// }
///
/// enum RollBackOnDropStrategy {}
/// impl DropManually<Transaction> for RollBackOnDropStrategy {
///     fn drop_manually(txn: Transaction) {
///         txn.roll_back();
///     }
/// }
/// ```
///
/// </details>
///
/// ### A silly, but interesting example: DIY-ing our own `ManuallyDrop<T>`
///
/// ```rust
/// use ::safe_manually_drop::prelude::*;
///
/// pub
/// enum ForgetOnDropStrategy {}
///
/// impl<T> DropManually<T> for ForgetOnDropStrategy {
///     fn drop_manually(value: T) {
///         ::core::mem::forget(value);
///     }
/// }
///
/// pub
/// type ManuallyDrop<T> = SafeManuallyDrop<T, ForgetOnDropStrategy>;
/// ```
///
///   - Note: do not do this in actual code, since calling `forget()` temporarily asserts validity
///     of the `value`, which means the resulting type is completey unable to offer
///     [`ManuallyDrop::take()`]-like APIs of any sort, and whatnot.
#[repr(transparent)]
pub
struct SafeManuallyDrop<FieldTy, ContainingType = diagnostics::MissingSecondTypeParam>
where
    ContainingType : DropManually<FieldTy>,
{
    _phantom: PD<fn() -> ContainingType>,
    field: ManuallyDrop<FieldTy>,
}

/// The impl tying everything together.
///
/// The main reason why an <code>impl [DropManually]</code> Just Works™, thanks to the following
/// blanket `impl`:
///
/// <code>impl\<FieldTy\> Drop for SafeManuallyDrop\<FieldTy, …\> where … : DropManually\<FieldTy\> { </code>
impl<FieldTy, ContainingType : DropManually<FieldTy>>
    Drop
for
    SafeManuallyDrop<FieldTy, ContainingType>
{
    #[inline]
    fn drop(&mut self) {
        let owned: FieldTy = unsafe {
            MD::take(&mut self.field)
        };
        ContainingType::drop_manually(owned)
    }
}

impl<FieldTy, ContainingType : DropManually<FieldTy>> SafeManuallyDrop<FieldTy, ContainingType> {
    /// Main, `const`-friendly, way to construct a [`SafeManuallyDrop<FieldTy, _>`] instance.
    ///
    /// Alternatively, there is a <code>[From]\<FieldTy> impl</code> as well.
    ///
    /// Tangentially, there shall also be <code>[Deref] \& [DerefMut] impls</code> with
    /// `Target = FieldTy`.
    ///
    /// [Deref]: `::core::ops::Deref`
    /// [DerefMut]: `::core::ops::DerefMut`
    #[inline]
    pub
    const
    fn new(value: FieldTy) -> Self {
        Self {
            _phantom: PD,
            field: MD::new(value),
        }
    }

    /// The inverse / reverse operation of the [`Self::new()`] constructor: _deconstructs_ a
    /// [`SafeManuallyDrop<FieldTy, …>`][`SafeManuallyDrop`] back into a bare `FieldTy` type, which,
    /// by virtue of this operation, shall go back to its default drop glue (rather than the
    /// _overridden_ one of <code>impl [DropManually]\<FieldTy\> for … {</code>).
    ///
    /// Such a process is typically called _defusing_ the (extra or special) drop glue.
    #[inline]
    #[allow(nonstandard_style)]
    pub
    const
    fn into_inner_defusing_impl_Drop(self) -> FieldTy {
        union ConstUncheckedTransmuter<Src, Dst> {
            src: MD<Src>,
            dst: MD<Dst>,
        }
        unsafe {
            // Safety: `repr(transparent)`, and no extra validity nor safety invariants at play.
            MD::into_inner(
                ConstUncheckedTransmuter::<
                    SafeManuallyDrop<FieldTy, ContainingType>,
                    FieldTy,
                >
                {
                    src: MD::new(self),
                }
                .dst
            )
        }
    }
}

impl<FieldTy, ContainingType : DropManually<FieldTy>>
    ::core::ops::Deref
for
    SafeManuallyDrop<FieldTy, ContainingType>
{
    type Target = FieldTy;

    #[inline]
    fn deref(&self) -> &FieldTy {
        &self.field
    }
}

impl<FieldTy, ContainingType : DropManually<FieldTy>>
    ::core::ops::DerefMut
for
    SafeManuallyDrop<FieldTy, ContainingType>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut FieldTy {
        &mut self.field
    }
}

impl<FieldTy, ContainingType : DropManually<FieldTy>>
    From<FieldTy>
for
    SafeManuallyDrop<FieldTy, ContainingType>
{
    fn from(field: FieldTy) -> Self {
        Self::new(field)
    }
}

impl<FieldTy : Default, ContainingType : DropManually<FieldTy>>
    Default
for
    SafeManuallyDrop<FieldTy, ContainingType>
{
    #[inline]
    fn default() -> Self {
        FieldTy::default().into()
    }
}

mod deref_impls {
    #![deny(unconditional_recursion)]

    use ::core::{
        cmp,
        fmt,
        hash,
    };
    use super::*;

    JustDerefTM!(
        impl[..: fmt::Debug] fmt::Debug ,for SafeManuallyDrop<..> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
        }

        impl[..: hash::Hash] hash::Hash ,for SafeManuallyDrop<..> {
            fn hash[H: hash::Hasher](&self, state: &mut H);
        }

        impl[..: PartialEq] PartialEq ,for SafeManuallyDrop<..> {
            fn eq(&self, other: &Self) -> bool;
        }

        impl[..: Eq] Eq ,for SafeManuallyDrop<..> {}

        impl[..: PartialOrd] PartialOrd ,for SafeManuallyDrop<..> {
            fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering>;
        }

        impl[..: Ord] Ord ,for SafeManuallyDrop<..> {
            fn cmp(&self, other: &Self) -> cmp::Ordering;
        }
    );
    // where:
    macro_rules! JustDerefTM {(
        $(
            impl[..: $($Bounds:tt)*] $Trait:path ,for SafeManuallyDrop<..> {
                $(
                    fn $fname:ident $([$($generics:tt)*])? (
                        &$self:ident $(,
                        $rest:ident: $Rest:ty)* $(,)?
                    ) $(-> $Ret:ty)?;
                )*
            }
        )*
    ) => (
        $(
            impl<FieldTy: $($Bounds)*, ContainingType : DropManually<FieldTy>>
                $Trait
            for
                SafeManuallyDrop<FieldTy, ContainingType>
            {
                $(
                    #[inline]
                    fn $fname $(<$($generics)*>)? (
                        &$self $(,
                        $rest: $Rest )*
                    ) $(-> $Ret)?
                    {
                        FieldTy::$fname($self $(, $rest)*)
                    }
                )*
            }
        )*
    )} use JustDerefTM;
}

#[cfg(feature = "better-docs")]
/// Deliberately not implemented
impl<FieldTy, ContainingType : DropManually<FieldTy>>
    !Clone
for
    SafeManuallyDrop<FieldTy, ContainingType>
{}

#[cfg(not(feature = "better-docs"))]
#[doc(hidden)]
#[allow(warnings, clippy::all)]
impl<FieldTy, ContainingType : DropManually<FieldTy>>
    /* !*/Clone // deliberately not implemented
for
    SafeManuallyDrop<FieldTy, ContainingType>
where
    for<'never> dyn Drop : Clone,
{
    fn clone(&self) -> Self {
        const { unreachable!() }
    }
}

/// Some helper for a nicer diagnostic suggestion/nudge in case of a forgotten second type
/// parameter.
mod diagnostics {
    use super::*;

    pub enum MissingSecondTypeParam {}

    impl<FieldTy> DropManually<FieldTy> for MissingSecondTypeParam
    where
        for<'never_true> MissingSecondTypeParam : ExplicitlyProvided,
    {
        fn drop_manually(_: FieldTy) {
            unreachable!()
        }
    }

    #[diagnostic::on_unimplemented(
        message = "\
            missing second type parameter for `SafeManuallyDrop<FieldTy, …>`. \
            Please use the containing `struct/enum` for it, such as: `Self`.\
        ",
        label = "help: use `SafeManuallyDrop<FieldTy, Self>` instead.",
    )]
    pub trait ExplicitlyProvided {}
}

#[doc = include_str!("compile_fail_tests.md")]
mod _compile_fail_tests {}
