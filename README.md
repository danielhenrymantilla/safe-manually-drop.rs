# `::safe-manually-drop`

[![Repository](https://img.shields.io/badge/repository-GitHub-brightgreen.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs)
[![Latest version](https://img.shields.io/crates/v/safe-manually-drop.svg)](
https://crates.io/crates/safe-manually-drop)
[![Documentation](https://docs.rs/safe-manually-drop/badge.svg)](
https://docs.rs/safe-manually-drop)
[![MSRV](https://img.shields.io/badge/MSRV-1.79.0-white)](
https://gist.github.com/danielhenrymantilla/9b59de4db8e5f2467ed008b3c450527b)
[![unsafe used](https://img.shields.io/badge/unsafe-used-ffcc66.svg)](
https://github.com/rust-secure-code/safety-dance/)
[![so that you don't](https://img.shields.io/badge/so_that-you_dont-success.svg)](
https://github.com/rust-secure-code/safety-dance/)
[![License](https://img.shields.io/crates/l/safe-manually-drop.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs/blob/master/LICENSE-ZLIB)
[![CI](https://github.com/danielhenrymantilla/safe-manually-drop.rs/workflows/CI/badge.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs/actions)
[![no_std compatible](https://img.shields.io/badge/no__std-compatible-success.svg)](
https://github.com/rust-secure-code/safety-dance/)

<!-- Templated by `cargo-generate` using https://github.com/danielhenrymantilla/proc-macro-template -->

Convenience wrapper type —and `trait`!— to expose owned access to a field when customizing the drop
glue of your type.

Non-macro equivalent of
[`::drop_with_owned_fields`](https://docs.rs/drop_with_owned_fields).

---

To expose _owned_ access to a `FieldTy` when drop glue is being run:

  - a [`SafeManuallyDrop<FieldTy, ContainingType>`][`SafeManuallyDrop`],
  - with a (mandatory)
    <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>,
  - once it gets dropped / during its drop glue (_e.g._, from within a `ContainingType`),
  - shall be running the [`DropManually::drop_manually()`] logic on that _owned_ `FieldTy`.

In practice, this becomes _the_ handy, 0-runtime-overhead, non-`unsafe`, tool to get owned
access to a `struct`'s field (or group thereof) during drop glue.

Indeed, the recipe then becomes:

 1. Use, instead of a `field: FieldTy`, a wrapped
    <code>field: [SafeManuallyDrop]\<FieldTy, Self\></code>,

      - (This wrapper type offers transparent
        <code>[Deref][`::core::ops::Deref`]{,[Mut][`::core::ops::DerefMut`]}</code>, as well as
        [`From::from()`] and ["`.into()`"][`SafeManuallyDrop::into_inner_defusing_impl_Drop()`]
        conversions.)

 1. then, provide the companion, mandatory,
    <code>impl [DropManually\<FieldTy\>][`DropManually`] for ContainingType {</code>

 1. Profit™:

      - from the owned access to `FieldTy` inside of
        [`DropManually::drop_manually()`]'s body.
      - from the convenience
        [`.into_inner_defusing_impl_Drop()`][`SafeManuallyDrop::into_inner_defusing_impl_Drop()`]
        which shall "deconstruct" that `FieldTy` despite the `DropManually` impl
        (which shall get defused).

# Motivation: owned access to some field(s) on `Drop`

<details class="custom" open><summary><span class="summary-box"><span>Click to hide</span></span></summary>

Consider, for instance, the two following examples:

## `Defer`

This is basically a simpler [`::scopeguard::ScopeGuard`]. The idea is that you'd first want to
(re)invent some kind of `defer! { … }` mechanism _via_ an _ad-hoc_ `impl Drop` type:

[`::scopeguard::ScopeGuard`]: https://docs.rs/scopeguard/*/scopeguard/struct.ScopeGuard.html

```rust ,ignore
// desired usage:
fn example() {
    let _deferred = defer(|| {
        println!("Bye, world!");
    });

    println!("Hello, world!");

    // stuff… (even stuff that may panic!)

} // <- *finally* / either way, `Bye` is printed here.
```

Here is how we could implement it:

```rust ,compile_fail
fn defer(f: impl FnOnce()) -> impl Drop {
    return Wrapper(f);
    // where:
    struct Wrapper<F : FnOnce()>(F);

    impl<F : FnOnce()> Drop for Wrapper<F> {
        fn drop(&mut self) {
            self.0() // Error, cannot move out of `self`, which is behind a `&mut` reference.
        }
    }
}
```

But this fails to compile! Indeed, since `Drop` only exposes `&mut self` access on `drop()`, we only
get `&mut` access to the closure, so the closure can only, at most, be an `FnMut()`, not an
`FnOnce()`.

  - Error message:

    <details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>

    ```rust ,ignore
    # /*
    error[E0507]: cannot move out of `self` which is behind a mutable reference
      --> src/_lib.rs:44:13
       |
    10 |             self.0() // Error, cannot move out of `self`, which is behind a `&mut` reference.
       |             ^^^^^^--
       |             |
       |             `self.0` moved due to this call
       |             move occurs because `self.0` has type `F`, which does not implement the `Copy` trait
       |
    note: this value implements `FnOnce`, which causes it to be moved when called
      --> src/_lib.rs:44:13
       |
    10 |             self.0() // Error, cannot move out of `&mut` reference.
       |             ^^^^^^
    # */
    ```

    </details>

So we either have to forgo using `FnOnce()` here, and settle for a limited API, such as
`F : FnMut()` (as in, more limited than what we legitimately know we should be able to _soundly_
have here: `FnOnce()`). Or we have to find a way to get _owned access on drop to our `F` field_.

Another example of this problem would be the case of:

## `rollback`-on-`Drop` transaction wrapper type

Imagine having to deal with the following API:

```rust
mod some_lib {
    pub struct Transaction {
        // private fields…
    }

    // owned access in these methods for a stronger, type-state-based, API.
    impl Transaction {
        pub fn commit(self) {
            // …
        }

        pub fn roll_back(self) {
            // …
        }
    }

    // say this does not have a default behavior on `Drop`,
    // or one which we wish to override.
}
```

We'd now like to have our own `WrappedTransaction` type, wrapping this API, with the added
feature / functionality of it automagically rolling back the transaction when _implicitly_ dropped
(_e.g._, so that `?`-bubbled-up errors and panics trigger this rollback path), expecting the users
to explicitly `.commit()` it at the end of their happy paths.

```rust
# mod some_lib {
#     pub struct Transaction {}
#     impl Transaction {
#         pub fn commit(self) {}
#         pub fn roll_back(self) {}
#     }
# }
#
struct WrappedTransaction(some_lib::Transaction);

impl WrappedTransaction {
    fn commit(self) {
        self.0.commit(); // OK
    }
}

// TODO: Add `roll_back` on `Drop`
```

If we go with the naïve approach, we'd end up doing:

```rust ,compile_fail
# mod some_lib {
#     pub struct Transaction {}
#     impl Transaction {
#         pub fn commit(self) {}
#         pub fn roll_back(self) {}
#     }
# }
#
struct WrappedTransaction(some_lib::Transaction);

// 👇
impl Drop for WrappedTransaction {
    fn drop(&mut self) {
        // 💥 Error, cannot move out of `self`, which is behind `&mut`,
        // yadda yadda.
        self.0.roll_back();
    }
}

impl WrappedTransaction {
    fn commit(self) {
        // Not only that, but we now also get the following extra error:
        //
        // 💥 Error cannot move out of type `WrappedTransaction`,
        //    which implements the `Drop` trait
        self.0.commit();
    }
}
```

  - Error message:

    <details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>

    ```rust ,ignore
    # /*
    error[E0507]: cannot move out of `self` which is behind a mutable reference
      --> src/_lib.rs:162:9
       |
    16 |         self.0.roll_back();
       |         ^^^^^^ ----------- `self.0` moved due to this method call
       |         |
       |         move occurs because `self.0` has type `Transaction`, which does not implement the `Copy` trait
       |
    note: `Transaction::roll_back` takes ownership of the receiver `self`, which moves `self.0`
      --> src/_lib.rs:153:26
       |
    7  |         pub fn roll_back(self) {}
       |                          ^^^^
    note: if `Transaction` implemented `Clone`, you could clone the value
      --> src/_lib.rs:150:5
       |
    4  |     pub struct Transaction {}
       |     ^^^^^^^^^^^^^^^^^^^^^^ consider implementing `Clone` for this type
    ...
    16 |         self.0.roll_back();
       |         ------ you could clone this value

    error[E0509]: cannot move out of type `WrappedTransaction`, which implements the `Drop` trait
      --> src/_lib.rs:171:9
       |
    25 |         self.0.commit();
       |         ^^^^^^
       |         |
       |         cannot move out of here
       |         move occurs because `self.0` has type `Transaction`, which does not implement the `Copy` trait
       |
    note: if `Transaction` implemented `Clone`, you could clone the value
      --> src/_lib.rs:150:5
       |
    4  |     pub struct Transaction {}
       |     ^^^^^^^^^^^^^^^^^^^^^^ consider implementing `Clone` for this type
    ...
    25 |         self.0.commit();
       |         ------ you could clone this value
    # */
    ```

    </details>

The first error is directly related to the lack of owned access, and instead, the limited
`&mut self` access, which the `Drop` trait exposes in its `fn drop(&mut self)` function.

  - (and the second error is a mild corollary from it, as in, the only way to extract owned access
    to a field of a `struct` would be by _deconstructing_ it, which would entail _defusing its
    extra/prepended drop glue_, and that is something which Rust currently conservatively rejects
    (hard error, rather than some lint or whatnot…).)

# How rustaceans currently achieve owned access in drop

## Either `Option`-`{un,}wrap`ping the field

The developer would wrap the field in question in an `Option`, expected to always be `Some` for
the lifetime of every instance, but for those last-breath/deathrattle moments in `Drop`, wherein the
field can then be `.take()`n behind the `&mut`, thereby exposing, _if all the surrounding code
played ball_, owned access to that field.

Should some other code have a bug w.r.t. this property, the `.take()` would yield `None`, and a
`panic!` would ensue.

### `Defer`

```rust
fn defer(f: impl FnOnce()) -> impl Drop {
    return Wrapper(Some(f));
    //             +++++ +
    // where:
    struct Wrapper<F : FnOnce()>(Option<F>);
    //                           +++++++ +

    impl<F : FnOnce()> Drop for Wrapper<F> {
        fn drop(&mut self) {
            self.0.take().expect("🤢")()
            //    +++++++++++++++++++
        }
    }
}
```

### `Transaction`

```rust
# mod some_lib {
#     pub struct Transaction {}
#     impl Transaction {
#         pub fn commit(self) {}
#         pub fn roll_back(self) {}
#     }
# }
#
struct WrappedTransaction(Option<some_lib::Transaction>);
//                        +++++++                     +

impl Drop for WrappedTransaction {
    fn drop(&mut self) {
        self.0.take().expect("🤢").roll_back();
    //        ++++++++++++++++++++
    }
}

impl WrappedTransaction {
    /// 👇 overhauled.
    fn commit(self) {
        let mut this = ::core::mem::ManuallyDrop::new(self);
        if true {
            // naïve, simple, approach (risk of leaking *other* fields (if any))
            let txn = this.0.take().expect("🤢");
            txn.commit();
        } else {
            // better approach (it does yearn for a macro):
            let (txn, /* every other field here */) = unsafe { // 😰
                (
                    (&raw const this.0).read(),
                    // every other field here
                )
            };
            txn.expect("🤢").commit();
        };
    }
}
```

## Or `unsafe`-ly `ManuallyDrop`-wrapping the field

The developer would wrap the field in question in a `ManuallyDrop`, expected never to have been
`ManuallyDrop::drop()`ped already for the lifetime of every instance, but for those
last-breath/deathrattle moments in `Drop`, wherein the field can then be `ManuallyDrop::take()`n
behind the `&mut`, thereby exposing, _if all the surrounding code played ball_, owned access to that
field.

Should some other code have a bug w.r.t. this property, the `ManuallyDrop::take()` would be
accessing a stale/dropped value, and UB would be _very likely_ to ensue ⚠️😱⚠️

### `Defer`

```rust
fn defer(f: impl FnOnce()) -> impl Drop {
    return Wrapper(ManuallyDrop::new(f));
    //             ++++++++++++++++++ +
    // where:
    use ::core::mem::ManuallyDrop; // 👈

    struct Wrapper<F : FnOnce()>(ManuallyDrop<F>);
    //                           +++++++++++++ +

    impl<F : FnOnce()> Drop for Wrapper<F> {
        fn drop(&mut self) {
            unsafe { // 👈 😰
                ManuallyDrop::take(&mut self.0)()
            //  ++++++++++++++++++
            }
        }
    }
}
```

### `Transaction`

```rust
# mod some_lib {
#     pub struct Transaction {}
#     impl Transaction {
#         pub fn commit(self) {}
#         pub fn roll_back(self) {}
#     }
# }
use ::core::mem::ManuallyDrop; // 👈

struct WrappedTransaction(ManuallyDrop<some_lib::Transaction>);
//                        +++++++++++++                     +

impl Drop for WrappedTransaction {
    fn drop(&mut self) {
        unsafe { // 😰
            ManuallyDrop::take(&mut self.0).roll_back();
        //  +++++++++++++++++++           +
        }
    }
}

impl WrappedTransaction {
    /// 👇 overhauled.
    fn commit(self) {
        let mut this = ::core::mem::ManuallyDrop::new(self);
        if true {
            // naïve, simple, approach (risk of leaking *other* fields (if any))
            let txn = unsafe {
                ManuallyDrop::take(&mut this.0)
            };
            txn.commit();
        } else {
            // better approach (it does yearn for a macro):
            let (txn, /* every other field here */) = unsafe { // 😰
                (
                    (&raw const this.0).read(),
                    // every other field here
                )
            };
            ManuallyDrop::into_inner(txn).commit();
        };
    }
}
```

---

Both of these approaches are unsatisfactory, insofar **the type system does not prevent implementing
this pattern incorrectly**: bugs remain possible, leading to either crashes in the former
non-`unsafe` case, or to straight up UB in the latter `unsafe` case.

Can't we do better? Doesn't the `Drop` trait with its meager `&mut self` grant appear to be the
culprit here? What if we designed a better trait (with, potentially, helper types)?

# Enter this crate: `SafeManuallyDrop` and `DropManually`

This is exactly what the `DropManually` trait fixes: by being more clever about the signature of its
own "dropping function", it is able to expose, to some implementor type, owned access to one of its
(aptly wrapped) fields:

### `Defer`

```rust
fn defer(f: impl FnOnce()) -> impl Sized {
    return Wrapper(SafeManuallyDrop::new(f));
    // where:
    use ::safe_manually_drop::{SafeManuallyDrop, DropManually}; // 👈

    // 👇 1. instead of the `Drop` trait, use:
    impl<F : FnOnce()> DropManually<F> for Wrapper<F> {
        fn drop_manually(f: F) {
            // It is *that simple*, yes!
            f();
        }
    }

    // 2. `SafeManuallyDrop` shall use it on `Drop`
    struct Wrapper<F : FnOnce()>(SafeManuallyDrop<F, Self>);
    //                           +++++++++++++++++ +++++++
}
```

### `Transaction`

```rust
# mod some_lib {
#     pub struct Transaction {}
#     impl Transaction {
#         pub fn commit(self) {}
#         pub fn roll_back(self) {}
#     }
# }
use ::safe_manually_drop::{DropManually, SafeManuallyDrop};

struct WrappedTransaction(SafeManuallyDrop<some_lib::Transaction, Self>);
//                        +++++++++++++++++                     +++++++

impl DropManually<some_lib::Transaction> for WrappedTransaction {
    fn drop_manually(txn: some_lib::Transaction) {
        // It is *that simple*, yes!
        txn.roll_back();
    }
}

impl WrappedTransaction {
    fn commit(self) {
        // It is *that friggin' simple*, yes! (no risk to leak the other fields 🤓)
        let txn = self.0.into_inner_defusing_impl_Drop();
        txn.commit();
    }
}
```

And _voilà_ 😙👌

---

</details>

# _Addendum_: `Drop impl` _vs._ drop glue _vs._ `drop()`

  - See [the relevant module][`appendix`]

It is generally rather important to properly distinguish between these three notions, but especially
so in the context of this crate!

Only skip this section if you can confidently answer what `drop` means in the context of:

  - `trait Drop { fn drop(&mut self); }`
  - `mem::drop::<T>(…);`
  - `ptr::drop_in_place::<T>(…);`
  - `mem::needs_drop::<T>();`

and if it is obvious to you that `String` does _not_ `impl Drop`.

[SafeManuallyDrop]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/struct.SafeManuallyDrop.html
[`SafeManuallyDrop`]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/struct.SafeManuallyDrop.html
[`SafeManuallyDrop::into_inner_defusing_impl_Drop()`]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/struct.SafeManuallyDrop.html#method.into_inner_defusing_impl_Drop
[`DropManually`]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/trait.DropManually.html
[`DropManually::drop_manually()`]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/trait.DropManually.html#tymethod.drop_manually
[`appendix`]: https://docs.rs/safe-manually-drop/^0.1.0/safe_manually_drop/appendix/index.html

[`ManuallyDrop`]: https://doc.rust-lang.org/stable/core/mem/struct.ManuallyDrop.html
[`::core::ops::Deref`]: https://doc.rust-lang.org/stable/core/ops/trait.Deref.html
[`::core::ops::DerefMut`]: https://doc.rust-lang.org/stable/core/ops/trait.DerefMut.html
[`Drop`]: https://doc.rust-lang.org/stable/core/ops/trait.Drop.html
[`Option`]: https://doc.rust-lang.org/stable/core/option/enum.Option.html
[`From::from()`]: https://doc.rust-lang.org/stable/core/convert/trait.From.html#tymethod.from
