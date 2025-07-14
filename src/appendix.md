# _Appendix_: `Drop impl` _vs._ drop glue _vs._ `drop()`

It is generally rather important to properly distinguish between these three notions, but especially
so in the context of this crate!

Only skip this section if you can confidently answer what `drop` means in the context of:

  - `trait Drop { fn drop(&mut self); }`
  - `mem::drop::<T>(…);`
  - `ptr::drop_in_place::<T>(…);`
  - `mem::needs_drop::<T>();`

and if it is obvious to you that `String` does _not_ `impl Drop`.

<details class="custom"><summary><span class="summary-box"><span>Click to show</span></span></summary>

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

  - imho, this function ought rather to be called `force_discard()` (or `move_out_of_scope()`) than
    `drop()`, should we be reaching a point where we mix up the different "drop" notions.

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
            drop_in_place::<T>(&mut value);
        }
        // now that the value has been dropped, we can consider the bits inside `value` to be
        // "exhausted"/empty, which I shall refer to as `Uninit<T>` (it probably won't be actually
        // uninit, but it *morally* / logically can be deemed as such, imho).
    }
    local_storage_dealloc!(value); // makes sure the backing `Uninit<T>` storage can be repurposed.
```

So this now requires knowing what _the drop glue of `T`_ is.

## The drop glue of some type `T`

[^drop_union]: this, in fact, is why Rust conservatively restricts `union` definitions to fields
known not to have any drop glue whatsover: `Copy` types, and `ManuallyDrop<T>`.

is defined "inductively" / structurally, as follows:

  - primitive types have their own, language-hardcoded, drop glue or lack thereof.

      - (most often than not, primitive types have no drop glue; the main and most notable exception
        being `dyn Trait`s).

  - the drop glue of tuples, arrays, and slices is that of its constituents (_inherent_ drop glue);

  - else, _i.e._, for `struct/enum/union`s:

      - (`union`s are `unsafe`, so they get no _inherent_ drop glue[^drop_union])

      - `enum`s get the _inherent_ drop glue of every field type of the active variant;

      - `struct`s get the _inherent_ drop glue of every one of its fields (_in well-guaranteed
        order!_ First its first fields gets `drop_in_place()`d, then its second, and so on);

    Given that most primitive types, such as pointer types, have no drop glue of their own, this
    mechanism, alone, would be unable to feature something as basic as the `free()`ing logic of the
    drop glue of a `Box`…

    Hence the need for the `PrependDropGlue` trait:

    ```rust
    trait PrependDropGlue {
        fn right_before_dropping_in_place_each_field_do(&mut self);
    }
    ```

    _e.g._,

    ```rust
    # use ::core::ptr::drop_in_place;
    #
    # macro_rules! reminder_of_what_the_compiler_does_afterwards {( $($tt:tt)* ) => ( )}
    #
    # trait PrependDropGlue {
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
    impl<T> PrependDropGlue for Box<T> {
        fn right_before_dropping_in_place_each_field_do(&mut self) {
          unsafe {
            // 1. make sure our well-init/valid pointee gets itself to be dropped;
            //    it's now or never!
            //    This takes care of the *transitively-owned* resources.
            //    e.g., when `T=String`, a `Box<String>` owns two heap resources:
            //      - some `str | [MaybeUninit<u8>]` byte buffer in the heap (what the `String`
            //        owns);
            //      - the `String { ptr: *mut u8, len: usize, cap: usize }` itself, which is behind
            //        `Box` indirection.
            //   This `1.` step is then taking care of freeing the byte buffer in the heap.
            drop_in_place::<T>(&mut *self.ptr);

            // 2. since our pointer stemmed from a call to `(m)alloc` or whatnot,
            //    we need to `free()` now, so that the `Uninit<T>` to which `self.ptr`
            //    points can be repurposed by the allocator.
            //
            //    Back to our `T` example, this `free()` call releases the 3 "words" in`String { … }`.
            //
            //    (we are papering over ZSTs).
            ::std::alloc::dealloc(self.ptr.cast(), ::std::alloc::Layout::new::<T>());
          }
        }
        // "now" the rest of inherent drop glue runs:
        reminder_of_what_the_compiler_does_afterwards! {
            drop_in_place::<*mut T>(&mut self.ptr);
            //              ^^^^^^            ^^^
            //             FieldType        field_name
            // and so on, for each field (here, no others)
        }
        // and finally, `local_storage_dealloc!(…)`, _i.e._, mark the by-value `Uninit<*mut T>`
        // as re-usable for other local storage.
    }
    ```

    It turns out that this trait, and method, have been showcased, in the `::core` standard library,
    **under a different name**, which may be the ultimate root / culprit / reason as to why all this
    "drop" terminology can get a bit confusing / easy for things to get mixed up when using the
    typical wave-handed terminology of the Rust book.

    The names used for these things in the standard library are the following:

      - `PrependDropGlue -> Drop`
      - `fn right_before_dropping_in_place_each_field_do(&mut self)` -> `fn drop(&mut self)`.

          - To clarify, I wouldn't intend this function to be named in such a long and unwieldy way,
            I have only done that for teaching purposes. A more fitting term w.r.t. a standard
            library _etiquette_ would rather be `fn drop_prelude(&mut self)`, or
            `fn before_drop(&mut self)`. I do like the `PrependDropGlue` name, though, but I would
            be amenable to it having been named `DropPrelude` instead. Anything but that bare,
            unqualified, and ambiguous, `Drop` name.

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

is basically the `PrependDropGlue` trait mentioned above.

## Having drop glue _vs._ `impl`ementing `Drop`

Notice how, since this is just about manually prepending custom drop glue at some layer type, the
moment the type gets wrapped within another one, that other one shall "inherit" this drop glue by
structural composition, and won't need, itself, to repeat that `impl PrependDropGlue`.

To better illustrate this, consider the following case study: the drop glue of [`Vec<_>`][`Vec`] &
[`String`]:

```rust
# macro_rules! reminder_of_what_the_compiler_does_afterwards {( $($tt:tt)* ) => ()}
# macro_rules! reminder_of_the_compiler_generated_drop_glue {( $($tt:tt)* ) => ()}
# use ::core::{mem::size_of, ptr::drop_in_place, slice};
#
# trait PrependDropGlue {
#     fn right_before_dropping_in_place_each_field_do(&mut self);
# }
#
struct Vec<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

impl<T> PrependDropGlue for Vec<T> {
    fn right_before_dropping_in_place_each_field_do(&mut self) {
        let Self { ptr, len, capacity } = *self;
        if size_of::<T>() == 0 || capacity == 0 {
            todo!("we paper over ZSTs and 0 capacity in this basic example");
        }
        unsafe {
            // 1. drop the `len` amount of init values/items;
            drop_in_place::<[T]>(&mut *slice::from_raw_parts_mut(ptr, len));

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
        drop_in_place::<*mut T>(&mut self.ptr); // does nothing
        drop_in_place::<usize>(&mut self.len); // does nothing
        drop_in_place::<usize>(&mut self.capacity); // does nothing
        // local_storage_dealloc!(…); // makes the by-value local storage for these 3 words be
                                      // re-usable.
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
    drop_in_place::<Vec<u8>>(&mut self.utf8_buffer);
    // i.e.
    {
        let vec: &mut Vec<u8> = &mut self.utf8_buffer;

        // 2.1. prepended drop glue, if any
        <Vec<u8> as PrependendDropGlue>::right_before_dropping_in_place_each_field_do(
            // this is where the meat of the resource reclaimation occurs, in this example.
            vec,
        );

        // 2.2. inherent / structurally inherited sub-drop-glues.
        drop_in_place::<*mut T>(&mut vec.ptr); // does nothing
        drop_in_place::<usize>(&mut vec.len); // does nothing
        drop_in_place::<usize>(&mut vec.capacity); // does nothing
    }

    // 3. `local_storage_dealloc!(self.utf8_buffer)`.
}
```

> So, in light of this, do we need to `impl PrependDropGlue for String {`?
>
> No!

Since it contains a `Vec<u8>` field, all of the drop glue of a `Vec<u8>`, including the
`PrependDropGlue for Vec<u8>` logic shall get invoked already.

And such logic already takes care of managing the resources of the `Vec`. Which means that such
resources get properly cleaned up / reclaimed assuming `Vec`'s own logic does (which it indeed
does). So `String` need not do anything, and in fact, ought very much not to be doing anything,
lest double-freeing ensue.

> ➡️ Artificially prepended drop glue logic becomes inherent / structurally inherited drop glue at
> the next layer of type wrapping.

This means we end up in a sitation wherein:

  - `Vec<…> : PrependDropGlue`
  - `String :! /* its _own_ layer of */ PrependDropGlue`.

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
standard library has used the "drop" word, alone, for three distinct things: `Drop` &
`fn drop(&mut self)`, `fn drop<T>(_: T)`, and `fn drop_in_place<T>(*mut T)`. As a reminder, the
first usage of "`Drop`/`fn drop(&mut self)` here refers to prepended drop glue; the second usage,
`fn drop<T>(_: T)`, refers rather about _discarding_ a value / forcing it to go out of scope,
and the third and last usage, `drop_in_place()` (alongside `needs_drop()`) finally refers to the
proper act of dropping a value, as in, running all of its drop glue (both the prepended one, if any,
and the structurally inherited one (but not running the `local_storage_dealloc!(self.utf8_buffer)`)).

## `&mut` or owned access in `Drop`?

With all that has been said, it should now be clear(er) why that trait only exposes _temporary_
`&mut self` access to `self`, rather than the naïvely expected _owned access_.

Indeed, if we received owned access to `self` in the `fn drop(…)` function, it would mean, as per
the ownership / move semantics of the language, running into a situation where the owned `self`
would run out of scope, get discarded, and thus, get _dropped_ (in place) / get its drop glue
invoked, which would mean invoking this (`Prepend`)`Drop`(`Glue`) first, which would discard this
owned `self`, _ad infinitum et nauseam_[^nauseam].

```rust ,ignore
impl DropOwned for Thing {
    fn drop_owned(self: Thing) {
        // stuff…
    } // <- `self` gets discarded here, so its drop glue gets invoked.
      //    Assuming `DropOwned` to be a magic trait to override this logic, it
      //    would mean that `Thing::drop_owned()` would get invoked here, *AGAIN*.
}
```

[^nauseam]: probably a stack overflow due to infinite recursion, else "just" a plain old
thread-hanging infinite loop.

 1. Which means every `DropOwned` impl, if it existed, would have to make sure to `forget(self)` at
    the end of its scope!

 1. But even if we did that (to avoid the infinite recursion problem), there is also the question
    of dropping each field:

      - we may want to be the ones doing it explicitly in the `fn` body (_e.g._,
      `drop(self.some_field);`);

          - so even more reason to make sure we are `mem::forget()`ting stuff at the end!

      - but if we are `mem::forget()`-ting `self` at the end of the `fn` body, then:

          - that would not be valid if we had done `drop(self.some_field);` beforehand;

          - and if we were to forget to eventually call `drop(self.field)` for every field before the
            end/final `mem::forget(self);`, then we'd be skipping the drop glue of all of these
            fields, which would be quite a footgun;

      - so the only way to conceive this happening properly would be to require the following `fn`
        prelude and epilogue:

        ```rust ,ignore
        // 1. we use `unsafe` to duplicate these fields, *and their ownership*.
        let (a, b, ..., field) = unsafe {
            (
                (&raw const self.a).read(),
                (&raw const self.b).read(),
                ...,
                (&raw const self.field).read(),
            )
        };

        /* 2. DROP OWNED FIELDS LOGIC HERE. */
        // We can:
        // drop(field);
        // as well as:
        // _overlooking_ to do so, since `a`, `b`, ... all go out of scope at the end of this `fn`
        // body anyways.

        // 3. Finally: disable the drop glue of self, and thus, of the original fields (so we don't
        // double-drop) anything.
        mem::forget(self);
        ```

        And to do this in a slightly more robust fashion (w.r.t. `panic!`-safety):

        ```rust ,ignore
        // 3. `defer! { mem::forget(self) }`, sort-to-speak; in a robust manner.
        let this = ::core::mem::ManuallyDrop::new(self);
        // 1.  we use `unsafe` to duplicate these fields, *and their ownership*.
        let (a, b, ..., field) = unsafe {
            (
                (&raw const this.a).read(),
                (&raw const this.b).read(),
                ...,
                (&raw const this.field).read(),
            )
        };

        /* 2. DROP OWNED FIELDS LOGIC HERE. */
        // We can:
        // drop(field);
        // as well as:
        // _overlooking_ to do so, since `a`, `b`, ... all go out of scope at the end of this `fn`
        // body anyways.
        ```

# TL,DR

</details>

<div style="max-width: 100%; overflow-x: auto; font-size: 0.68em;">

<table border="1" cellspacing="0" cellpadding="8">
  <!-- 3 -->
  <tr>
    <!-- B2:B4 -->
    <td rowspan="3">
<div style="text-align: center;">Discarding a <code>value: T</code></div>
<div class="example-wrap"><pre class="rust rust-example-rendered"><code><span class="comment">// When `value: T` goes
// out of scope:
</span><span class="macro">out_of_scope!</span>(value: T);</code></pre></div>
    </td>
    <!-- C2:C3 -->
    <td rowspan="2">
<div style="text-align: center;">"Drop <em>glue</em>" of <code>T</code></div>
<div class="example-wrap"><pre class="rust rust-example-rendered"><code><span class="comment">// if mem::needs_drop/*glue*/::&lt;T&gt;(),
// 1. Run the drop glue for T
</span>&lt;<span class="kw-2">*mut </span>T&gt;::drop_in_place(<span class="kw-2">&amp;mut </span>value);</code></pre><div class="button-holder"><button class="copy-button" title="Copy code to clipboard"></button></div></div>
    </td>
    <!-- D2 -->
    <td>
Firstly, when <code>T : PrependDropGlue / Drop</code>:
<div class="example-wrap"><pre class="rust rust-example-rendered"><code>&lt;T as PrependDropGlue&gt;::drop_prelude(<span class="kw-2">&amp;mut </span>value)
<span class="comment">// i.e., using actual stdlib terminology:
</span>&lt;T as Drop&gt;::drop(<span class="kw-2">&amp;mut </span>value);</code></pre><div class="button-holder"><button class="copy-button" title="Copy code to clipboard"></button></div></div>
    </td>
  </tr>
  <!-- 3 -->
  <tr>
    <!-- B done -->
    <!-- C done -->
    <!-- D3 -->
    <td>
And then, always:
<div class="example-wrap"><pre class="rust rust-example-rendered"><code>#<span class="kw">for </span>#field of T {
    <span class="comment">// if mem::needs_drop/*glue*/::&lt;FieldTy&gt;(),
    </span>&lt;<span class="kw-2">*mut </span>#FieldTy&gt;::drop_in_place(<span class="kw-2">&amp;mut </span>value.#field);
}</code></pre><div class="button-holder"><button class="copy-button" title="Copy code to clipboard"></button></div></div>
    </td>
  </tr>
  <!-- 4 -->
  <tr>
    <!-- B done -->
    <!-- C4 -->
    <td>
<div class="example-wrap"><pre class="rust rust-example-rendered"><code><span class="comment">// 2. StorageDead in MIR parlance
</span><span class="macro">dealloc_local_storage!</span>(value: MaybeUninit&lt;T&gt;);</code></pre></div>
    </td>
  </tr>
</table>

  - About `mem::drop::<T>(value)` _vs._ `out_of_scope!(value: T)`:

    ```rust
    # /*
    drop::<T>(value);
    // is the same as:
    {
        let new_value: T;  // alloc_local_storage!(); i.e. StorageLive in MIR parlance
        new_value = value; // 1. (shallow) bit-copy from `value` to `new_value`.
                           // 2. `value` registered as "moved out of" (when !Copy):
                           //      - further usage of `value` is forbidden
                           //      - `dealloc_local_storage!(value)` allowed to happen.
                           // Very likely compiler optimization: bitcopy elision, wherein
                           // `new_value` is to re-use the local storage of `value`.

    } // <- out_of_scope!(new_value: T);
    # */
    ```

    This is why `drop<T>(value)`, for a non-`Copy` value, ends up boiling down to _discarding_ such a
    `value` / to _forcing it out of scope_:

    ```rust
    # /*
    // Alternative names for `mem::drop()`:
    drop(v) == discard(v) == force_out_of_scope(v) == yeet_out_of_scope(v) == move_out_of_scope(v)
    # */
    ```

</div>

So, to the question "**what does _dropping_ mean in Rust?**", hopefully it is now clear that such
a question or definition is actually quite imprecise and ambiguous, since there are
_three_ things which use the _drop_ terminology in Rust; granted, deeply-related, but not quite
the same thing.

Those three things are the different columns of that first "row" of the table:

> 📝 Dropping a value, in Rust, can refer: 📝
>
>   - to what happens when a `value: T` goes out of scope, or quite relatedly, to a call to
>     `mem::drop::<T>(value)`: the act of discarding a value / forcing it to go out of scope;
>   - to running the _drop glue_ of `T` for that `value`, which can be (`unsafe`ly) done _via_
>     `ptr::drop_in_place::<T>(&mut value)`;
>   - when `T : Drop` (and only then), to this prepended extra drop logic that is to run, within
>     the drop glue of `T`, before each of its fields is being, itself, "dropped" / having its
>     drop glue invoked.
>
> `<T as Drop>::drop()` is part of the drop glue of `T` which is itself part of what happens
> when a `value: T` goes out of scope / gets discarded or `mem::drop()`ped.
>
> 📝

# What would it take to have owned access in custom drop glue / `drop_in_place()` logic?

Well, if we stare at those two previous points, we can see a path forward towards "the perfect
`DropGlue` trait".

  - Starting from the latter bullet, this part can be solved in one of two ways:

      - as a library abstraction, by having the fields be [`ManuallyDrop`]-wrapped, since that
        effectively disables the structurally inherited drop glue for that field (so, if it is done
        for each and every field, we will have effectively gotten rid of all of the structurally
        inherited drop glue);

      - as a language tweak, it would be trivial for the language to just stop injecting the
        structurally-inherited drop glue if the trait overriding the _whole_ drop glue were to be
        present;

  - Then, back to the former bullet, in order to avoid the infinite recursion of `self` drop issue,
    the solution is simply a matter of picking the owned types involved with a bit more _finesse_:
    rather than getting full, owned, access to `self: Self` on `drop()`, we'd be getting per field,
    owned access, to each field type.

So here is what such a trait could look like:

```rust ,ignore
// In pseudo-code.
trait OverrideDropGlue {
    fn drop(fields_of!(Self { .. }));
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
    _ever_[^hope] available to an `impl PrependDropGlue` (or an `impl OverrideDropGlue` type),
    would entail "defusing"/bypassing/skipping it, and instead, just extracting raw access to the
    fields; which is _exactly_ the desired behavior here! That _defusing_ (of the shallowmost layer
    of `PrependDropGlue`) is exactly what avoids the infinitely recursive drop logic in a neat and
    succint way!

[^hope]: I really think we should be given such a tool, even if it would entail memory leaks in
visibility-capable codebases doing things such as `let MyVec { ptr, len, cap } = my_vec;` (same as
`into_raw_parts`: leaky pattern).

# Back to this crate, or to `::drop_with_owned_fields`

Now, if you stare at either this crate, or even more so at the companion
[`::drop_with_owned_fields`] crate, you'll notice they're both trying to offer a user-library /
third-party-library powered way to offer such an API to users of these libraries.

[`::drop_with_owned_fields`]: https://docs.rs/drop-with-owned-fields/^0.1.1

## Using the [`::drop_with_owned_fields`] helper crate, for starters:

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

    But you 🫵, attentive reader of my whole diatribe, should know better: since `Drop` means
    `PrependDropGlue`, it's not the right trait to be using in this scenario. It should have
    been this imagined `OverrideDropGlue` trait instead.

    In fact, you can go and be more explicit about the "drop-related" `impl`, even in the
    context of this macro-based `::drop_with_owned_fields` crate, if you forgo its `drop_sugar`
    API and eponymous Cargo feature, and instead directly target _the real trait_,
    `DropWithOwnedFields`:

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
    `OverrideDropGlue` trait and language support, I'd say.

## And using this very crate rather than macros:

# Owned access to a type's field in custom drop logic

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

impl DropManually<Fields> for /* the drop glue of */ CustomDropOrder {
    fn drop_manually(Fields { a, b, c }: Fields) {
        drop(b);
        drop(a);
        drop(c);
    }
}
```

See the docs of [`SafeManuallyDrop`] for more info.
