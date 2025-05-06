# `::safe-manually-drop`

[![Repository](https://img.shields.io/badge/repository-GitHub-brightgreen.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs)
[![Latest version](https://img.shields.io/crates/v/safe-manually-drop.svg)](
https://crates.io/crates/safe-manually-drop)
[![Documentation](https://docs.rs/safe-manually-drop/badge.svg)](
https://docs.rs/safe-manually-drop)
[![MSRV](https://img.shields.io/badge/MSRV-1.79.0-white)](
https://gist.github.com/danielhenrymantilla/9b59de4db8e5f2467ed008b3c450527b)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](
https://github.com/rust-secure-code/safety-dance/)
[![License](https://img.shields.io/crates/l/safe-manually-drop.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs/blob/master/LICENSE-ZLIB)
[![CI](https://github.com/danielhenrymantilla/safe-manually-drop.rs/workflows/CI/badge.svg)](
https://github.com/danielhenrymantilla/safe-manually-drop.rs/actions)
[![no_std compatible](https://img.shields.io/badge/no__std-compatible-success.svg)](
https://github.com/rust-secure-code/safety-dance/)

<!-- Templated by `cargo-generate` using https://github.com/danielhenrymantilla/proc-macro-template -->

---

# _Addendum_: `Drop impl` _vs._ drop glue _vs._ `drop()`

It is generally rather important to properly distinguish these three notions, but especially so
in the context of this crate!

## What does `drop(value)` do?

In a nutshell, _nothing_; at least within its function body, which is _utterly empty_. All of the
`drop(value)` semantics stem, merely, from `move` semantics, wherein the _scope of `value`_ is now
repurposed/changed/narrowed down to that of `fn drop()`'s body. Which is empty, so it just ends
right away and returns:

```rust
fn drop<T>(value: T) {
    // *move* semantics / all of Rust design makes it so `value` is now a local variable/binding
    // scoped to this function body, so:
} // <- it simply gets "discarded" here.
```

## What happens *exactly* when a `value: T` goes out of scope / gets "discarded"?

The story goes as follows: in Rust, when an "owned value" (`value: T`) / owned "variable" / owned
"binding" (even anonymous ones!) goes out of scope, it gets "_discarded_":

```rust ,ignore
    {
        let value: T = ...;
        ...
 // `value` gets discarded here.
 // v
    } // <- compiler emits: `discard!(value)`, sort to speak,
// where `discard!(value)` could be defined as:
if const { ::core::mem::needs_drop::<T>() } {
    unsafe {
        // Run *the drop glue of `T`* for that `value`.
        ptr::drop_in_place::<T>(&raw mut value);
    }
    // now that the value has been dropped, we can consider the bits inside `value` to be
    // "exhausted"/empty, which I shall refer to as `Uninit<T>` (it probably won't be actually
    // uninit, but it *morally* / logically can be deemed as such, imho).
}
local_storage_dealloc!(value); // makes sure the backing `Uninit<T>` storage can be repurposed.
```

So this now requires knowing what _the drop glue of `T`_ is.

## The drop glue of some type `T`

is defined "inductively" / structurally, as follows:

  - primitive types have their own, language-hardcoded, drop glue or lack thereof.

      - (most often than not, primitive types have no drop glue; the main and most notable exception
        being `dyn Trait`s).

  - the drop glue of tuples, arrays, and slices is that of its constituents (_inherent_ drop glue);

  - else, _i.e._, for `struct/enum/union`s:

      - (`union`s are `unsafe`, so they get no _inherent_ drop glue;)

      - `enum`s get the _inherent_ drop glue of every field type of the active variant;

      - `struct`s get the _inherent_ drop glue of every one of its fields (_in well-guaranteed
        order!_ First its first fields gets `drop_in_place()`d, then its second, and so on);

    Given that most primitive types, such as pointer types, have no drop glue of their own, this
    mechanism, alone, would be unable to feature something as basic as the `free()`ing logic of the
    drop glue of a `Box`…

    Hence the need for the `PrependedDropGlue` trait:

    ```rust
    trait PrependedDropGlue {
        fn right_before_dropping_in_place_each_field_do(&mut self);
    }
    ```

    _e.g._,

    ```rust
    # use ::core::ptr;
    #
    # macro_rules! reminder_of_what_the_compiler_does_afterwards {( $($tt:tt)* ) => ( )}
    #
    # trait PrependedDropGlue {
    #     fn right_before_dropping_in_place_each_field_do(&mut self);
    # }
    #
    struct Box<T> {
        ptr: *mut T, // <- output of `malloc()`/`alloc()`, say, with a
                     //    valid/initialized `T` value.
    }

    // Right now, if a `Box<T>` were to be dropped / if `drop_in_place::<Box<T>>()`
    // were to be called, it would just delegate to `drop_in_place::<*mut T>()`ing
    // its one field, but that one does *nothing*, so this `Box<T>`, when dropped,
    // so far does nothing.

    // Hence:
    impl<T> PrependedDropGlue for Box<T> {
        fn right_before_dropping_in_place_each_field_do(&mut self) {
          unsafe {
            // 1. make sure our well-init/valid pointee gets itself to be dropped;
            //    it's now or never!
            ptr::drop_in_place::<T>(self.ptr);

            // 2. since our pointer stemmed from a call to `(m)alloc` or whatnot,
            //    we need to `free()` now, so that the `Uninit<T>` to which `self.ptr`
            //    points can be repurposed by the allocator.
            //
            //    (we are papering over ZSTs).
            ::std::alloc::dealloc(self.ptr.cast(), ::std::alloc::Layout::new::<T>());
          }
        }
        // "now" the rest of inherent drop glue runs:
        reminder_of_what_the_compiler_does_afterwards! {
            ptr::drop_in_place::<*mut T>(&raw mut self.ptr);
            //                   ^^^^^^                ^^^
            //                  FieldType            field_name
            // and so on, for each field (here, no others)
        }
    }
    ```

    It turns out that this trait, and method, have been showcased, in the `::core` standard library,
    **under a different name**, which may be the ultimate root / culprit / reason as to why all this
    "drop" terminology can get a bit confusing / easy for things to get mixed up when using the
    typical wave-handed terminology of the Rust book.

    The names used for these things in the standard library are the following:

      - `PrependedDropGlue -> Drop`
      - `fn right_before_dropping_in_place_each_field_do(&mut self)` -> `fn drop(&mut self)`.

    Hence:

    ```rust
    trait Drop {
        fn drop(&mut self);
    }
    ```

    So, to conclude the inductive/structural definition of the drop glue of some type `T`, it's:

     1. First, running, if any, the "extra, prepended, custom drop glue" for the type `T`, defined
        within the `Drop` trait, or rather is `impl`ementation `for T`.

     1. Then, transitively running the drop glue for each and every (active) field of the type.

## The `Drop` trait

is basically the `PrependedDropGlue` trait mentioned above.

### Having drop glue _vs._ `impl`ementing `Drop`

Notice how, since this is just about manually prepending custom drop glue at some layer type, the
moment the type gets wrapped within another one, that other one shall "inherit" this drop glue by
structural composition, and won't need, itself, to repeat that `impl PrependedDropGlue`.

To better illustrate this, consider the following case study: the drop glue of [`Vec<_>`][`Vec`] &
[`String`]:

```rust
# macro_rules! reminder_of_what_the_compiler_does_afterwards {( $($tt:tt)* ) => ()}
# macro_rules! reminder_of_the_compiler_generated_drop_glue {( $($tt:tt)* ) => ()}
# use ::core::{mem::size_of, ptr};
#
# trait PrependedDropGlue {
#     fn right_before_dropping_in_place_each_field_do(&mut self);
# }
#
struct Vec<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

impl<T> PrependedDropGlue for Vec<T> {
    fn right_before_dropping_in_place_each_field_do(&mut self) {
        let Self { ptr, len, capacity } = *self;
        if size_of::<T>() == 0 || capacity == 0 {
            todo!("we paper over ZSTs and 0 capacity in this basic example");
        }
        unsafe {
            // 1. drop the `len` amount of init values/items;
            ptr::slice_from_raw_parts_mut(ptr, len).drop_in_place();

            // 2. deällocate the backing heap buffer.
            ::std::alloc::dealloc(
                ptr.cast(),
                ::std::alloc::Layout::array::<T>(capacity)
                    .expect("total size to be befitting, as per the very existence of the Vec")
                ,
            );
        }
    }
    reminder_of_what_the_compiler_does_afterwards! {
        ptr::drop_in_place::<*mut T>(&raw mut self.ptr); // does nothing
        ptr::drop_in_place::<usize>(&raw mut self.len); // does nothing
        ptr::drop_in_place::<usize>(&raw mut self.capacity); // does nothing
    }
}

/// A `String` is "just" a `Vec<u8>` but with the invariant that the `..len` bytes
/// are (the) valid UTF-8 (encoding of some abstract string value).
struct String {
    utf8_buffer: Vec<u8>,
}

reminder_of_the_compiler_generated_drop_glue! {
    // 1. prepended drop glue, if any:
    /* None! */

    // 2. inherent / structurally inherited sub-drop-glue(s).
    ptr::drop_in_place::<Vec<u8>>(&raw mut self.utf8_buffer);
    // i.e.
    {
        let vec: &mut Vec<u8> = &mut self.utf8_buffer;

        // 1. prepended drop glue, if any
        <Vec<u8> as PrependendDropGlue>::right_before_dropping_in_place_each_field_do(
            vec,
        );

        // 2. inherent / structurally inherited sub-drop-glues.
        ptr::drop_in_place::<*mut T>(&raw mut vec.ptr); // does nothing
        ptr::drop_in_place::<usize>(&raw mut vec.len); // does nothing
        ptr::drop_in_place::<usize>(&raw mut vec.capacity); // does nothing
    }
}
```

> So, in light of this, do we need to `impl PrependedDropGlue for String {`?
>
> No!

Since it contains a `Vec<u8>` field, all of the drop glue of a `Vec<u8>`, including the
`PrependedDropGlue for Vec<u8>` logic shall get invoked already.

And such logic already takes care of managing the resources of the `Vec`. Which means that such
resources get properly cleaned up / reclaimed assuming `Vec`'s own logic does (which it indeed
does). So `String` need not do anything, and in fact, ought very much not to be doing anything,
lest double-freeing ensues.

> ➡️ Artificially prepended drop glue logic becomes inherent / structurally inherited drop glue at
> the next layer of type wrapping.

This means we end up in a sitation wherein:

  - `Vec<…> : PrependedDropGlue`
  - `String :! /* its _own_ layer of */ PrependedDropGlue`.

And yet both have meaningful drop glue:

  - manually/explicitly prepended for `Vec`,
  - and structurally _inherent_/inherited for `String`.

In "drop/`Drop`" parlance:

  - <code>[Vec]\<_\> : [Drop]</code>;

  - <code>[String] :! [Drop]</code> (and yet [`::core::mem::needs_drop::<String>()`] is `true`);

      - ([`::core::mem::needs_drop()`] expresses whether a given type has any drop glue whatsoever.)

This is generally why having `T : Drop` kind of bounds in generics is an anti-pattern —an explicitly
linted one!—, and a big smell indicating that the person having written this has not properly
understood these nuances (which, again, is a very legitimate mistake to make, since the Rust
standard library has used the "drop" word, alone, for three distinct things, `Drop`,
`fn drop(&mut self)`, and `fn drop<T>(_: T)`, and that is without including the fourth occurrence of
the name, in `drop_in_place()`. As a reminder, the first two usages of "drop" here refer to
prepended drop glue, the third usage, `fn drop<T>(_: T)`, refers rather about _discarding_ a value /
forcing it to go out of scope, and the fourth and last usage, `drop_in_place()` (along
`needs_drop()`) finally refers to the proper act of dropping a value, as in, running all of its drop
glue (both the prepended one, if any, and the structurally inherited one)).

### `&mut` or owned access in `Drop`?

With all that has been said, it should now be clear(er) why that trait only exposes _temporary_
`&mut self` access to `self`, rather than the naïvely expected _owned access_.

Indeed, if we received owned access to `self` in the `fn drop(…)` function, it would mean, as per
the ownership / move semantics of the language, running into a situation where the owned `self`
would run out of scope, get discarded, and thus, get _dropped_ (in place) / get its drop glue
invoked, which would mean invoking this (`Prepended`)`Drop`(`Glue`) first, which would discard this
owned `self`, _ad infinitum et nauseam_[^nauseam].

[^nauseam]: probably a stack overflow due to infinite recursion, else "just" a plain old
thread-hanging infinite loop.

 1. Which means every `DropOwned` impl, if it existed, would have to make sure to `forget(self)` at
    the end of its scope!

 1. But even if we did that (to avoid the infinite recursion problem), we'd still have the problem
    of the inherent drop glue then dropping each field "again".

    Because, again, at the end of the day, `PrependedDropGlue` is not the full, drop-glue picture!
    Owned access to the being-dropped(-in-place) resource cannot be offered in `PrependedDropGlue`,
    since it represents just a _prelude_, after which access to the being-dropped(-in-place) value is
    relinquished, passed on to the rest of the [`drop_in_place()`][`::core::ptr::drop_in_place()`]
    machinery.

    So to avoid this we'd have to also `forget()` every field beforehand; at which point, what has
    been the point of that initial owned access to begin with?

## What would it take to have owned access in custom drop glue / `drop_in_place()` logic?

Well, if we stare at those two previous points, we can see a path forward towards "the perfect
`DropGlue` trait".

  - Starting from the latter bullet, this part can be solved in one of two ways:

      - as a library abstraction, by having the fields be [`ManuallyDrop`]-wrapped, since that
        effectively disables the structurally inherited drop glue for that field (so if it is done
        for each and every field we have effectively gotten rid of all of the structurally inherited
        drop glue);

      - as a language tweak, it would be trivial for the language to just stop injecting the
        structurally-inherited drop glue if the trait overriding the _whole_ drop glue were to be
        present;

  - Then, back to the former bullet, in order to avoid the infinite recursion of `self` drop issue,
    the solution is simply a matter of picking the owned types involved with a bit more of
    _finesse_: rather than getting full, owned, access to `self: Self` on `drop()`, we'd be getting
    per field, owned access, to each field type.

So here is what such a trait could look like:

```rust ,ignore
// In pseudo-code.
trait OverrideDropGlue {
    fn drop(Self { ..fields });
}

// A concrete example:
struct CustomDropOrder {
    a: A,
    b: B,
    c: C,
}

impl OverrideDropGlue for CustomDropOrder {
    fn drop(a: A, b: B, c: C) {
        drop(b);
        drop(c);
        drop(a);
    }
}
```

or, imagining some syntax sugar here to make it a tad clearer:

```rust ,ignore
impl OverrideDropGlue for CustomDropOrder {
    fn drop(Self { a, b, c }) {
        drop(b);
        drop(c);
        drop(a);
    }
}
```

  - What I like about this last syntax is that braced-`struct` destructuring, if it were
    _ever_[^hope] available to an `impl PrependedDropGlue` (or an `impl OverrideDropGlue` type),
    would entail "defusing"/bypassing/skipping it, and instead, just extracting raw access to the
    fields; which is _exactly_ the desired behavior here! That _defusing_ is exactly what avoids
    the infinitely recursive drop logic in a neat and succint way!

[^hope]: I really thing we should be given such a tool, even if it would entail memory leaks in
visibility-capable codebases doing things such as
```rust ,ignore
let MyVec { ptr, len, cap } = vec; // same as into_raw_parts, leaky pattern!
```

### Back to this crate, or to `::drop_with_owned_fields`

Now, if you stare at what either this crate, or even more so the companion
[`::drop_with_owned_fields`] crate, do, you'll notice it's all trying to offer a user-library /
third-party-library powered way to offer such an API to users of this library.

[`::drop_with_owned_fields`]: https://docs.rs/drop-with-owned-fields/^0.1.1

Using [`::drop_with_owned_fields`] for instance:

```rust
use ::drop_with_owned_fields::prelude::*;
#
# struct A; struct B; struct C;

#[drop_with_owned_fields(as _)]
struct CustomDropOrder {
    a: A,
    b: B,
    c: C,
}

#[drop_with_owned_fields]
impl Drop for CustomDropOrder {
    fn drop(Self { a, b, c }: _) {
        drop(b);
        drop(c);
        drop(a);
    }
}
#
# fn main() {}
```

  - Notice how, for the sake of being less jarring to users, that macro API asks for the `Drop`
    trait to be involved, since it's probably the least surprising choice for unaware users.

    But you 🫵 attentive reader of my whole diatribe, should know better: since `Drop` means
    `PrependedDropGlue`, it's not the right trait to be using in this scenario.

    You can go and be explicit even with that macro API, if you forgo its `drop_sugar` API and
    eponymous Cargo feature:

    ```rust
    use ::drop_with_owned_fields::prelude::*;
    #
    # struct A; struct B; struct C;

    #[drop_with_owned_fields(as struct Fields)]
    struct CustomDropOrder {
        a: A,
        b: B,
        c: C,
    }

    impl DropWithOwnedFields for CustomDropOrder {
        fn drop(Fields { a, b, c }: DestructuredFieldsOf<CustomDropOrder>) {
            drop(b);
            drop(c);
            drop(a);
        }
    }
    #
    # fn main() {}
    ```

    The resulting `DropWithOwnedFields` is the closest a library API can get to the dreamed
    `OverrideDropGlue` trait and language support.

___

And using this very crate rather than macros:

```rust
//! Look ma, no macros!

use ::safe_manually_drop::prelude::*;
#
# struct A; struct B; struct C;

struct CustomDropOrder(
    SafeManuallyDrop<Fields, Self>,
);

struct Fields {
    a: A,
    b: B,
    c: C,
}

impl DropManually<Fields> for CustomDropOrder {
    fn drop_manually(Fields { a, b, c }: Fields) {
        drop(b);
        drop(a);
        drop(c);
    }
}
```
