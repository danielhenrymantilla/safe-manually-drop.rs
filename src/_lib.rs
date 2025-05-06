#![doc = include_str!("../README.md")]
#![cfg_attr(not(doc), no_std)]
#![allow(unused_braces)]

use ::core::{
    marker::PhantomData as PD,
    mem::{ManuallyDrop, ManuallyDrop as MD},
};

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
///  1. with a (mandatory) <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>
///  1. shall be running the [`DropManually::drop_manually()`] logic on that, _owned_, `FieldTy`,
///  1. when it gets dropped / during its drop glue.
///
/// Note that this new drop glue logic for `FieldTy`, defined in [`DropManually::drop_manually()`],
/// shall _supersede_ / override its default drop glue.
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
        In order for a struct/enum to contain a `SafeManuallyDrop<FieldTy, …>` field:\n \
        1. `…`, the second type parameter, ought to be `Self`, i.e., the containing `struct/enum` \
          wherein the provided `DropManually` logic makes sense.\n \
          For instance:\n\n \
          ```rust\n \
          SafeManuallyDrop<FieldTy, Self>\n \
          ```\n\
          \n \
        2. you then have to provide an `impl<…> DropManually<FieldTy> for \
          <the containing struct/enum> {{`.\n \
          For instance:\n\n \
          ```rust\n \
          impl<…> DropManually<FieldTy> for StructName<…> {{\n \
          ```\n\
          \n\
    ",
)]
pub
trait DropManually<FieldTy> {
    fn drop_manually(_: FieldTy);
}

/// [`SafeManuallyDrop<FieldTy>`] is the safe counterpart of [`ManuallyDrop<FieldTy>`], and the
/// zero-runtime-overhead counterpart of [`Option<FieldTy>`].
///
/// ## Example
///
///   - Using [`ManuallyDrop<FieldTy>`] and `unsafe`:
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
///     ```
///
///   - Using [`Option<FieldTy>`] and [`.unwrap()`][`Option::unwrap()`]s everywhere…
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
///     ```
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
///         // Option<DeferGuardFields<T, F>>,
///         // ManuallyDrop<DeferGuardFields<T, F>>,
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
/// In practice, neither the API of this crate, nor that of any non-macro API for that matter, can
/// ever hope to check, impose, nor control that the `ContainingType` used for a
/// [`SafeManuallyDrop<FieldTy, ContainingType>`] do match that of the containing `struct`.
///
/// And, as a matter of fact, there may even be legitimate cases where you may do so on purpose.
///
/// Indeed, this extra type parameter is, at the end of the day, a mere `impl DropManually`
/// "identifier" / discriminant for it to be possible for anybody to write such impls for arbitrary
/// `FieldTy` types, even when the `FieldTy` is a fully unconstrained/blanket `<T>/<F>` generic
/// type, and/or when it stems from an upstream crate, or even when wanting to repeat
/// `SafeManuallyDrop<FieldTy, …>` multiple types within the same `struct`.
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
/// ### `repr()` guarantee.
///
/// This type is guaranteed to be a mere `#[repr(transparent)]` wrapper around its `FieldTy`.
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
    /// Main, `const`-friendly, way to construct a [`SafeManuallyDrop<FieldTy, …>`] instance.
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
        #[allow(non_local_definitions)]
        impl<FieldTy, ContainingType : DropManually<FieldTy>>
            From<FieldTy>
        for
            SafeManuallyDrop<FieldTy, ContainingType>
        {
            fn from(field: FieldTy) -> Self {
                Self::new(field)
            }
        }

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
