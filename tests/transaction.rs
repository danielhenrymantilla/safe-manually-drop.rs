#![allow(unused_braces)]

mod txn_lib {
    use ::std::{
        sync::{Arc, Weak},
    };
    use ::safe_manually_drop::prelude::{
        DropManually,
        SafeManuallyDrop,
    };

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub
    enum DbState {
        Committed,
        RolledBack,
    }

    pub(crate)
    struct RawTransaction<'r> {
        db_state: &'r mut Option<DbState>,
    }

    impl<'r> RawTransaction<'r> {
        pub(crate)
        fn new(
            db_state: &'r mut Option<DbState>,
        ) -> Self
        {
            assert_eq!(*db_state, None);
            Self {
                db_state,
            }
        }

        /// Owned access for a safer, type-wise, API.
        pub
        fn commit(self) {
            *self.db_state = Some(DbState::Committed);
        }

        /// Owned access for a safer, type-wise, API.
        pub
        fn roll_back(self) {
            *self.db_state = Some(DbState::RolledBack);
        }
    }

    /// Convenience wrapper around a [`RawTransaction`], which automatically rolls back on drop.
    ///
    /// Users need to call [`.commit()`][`Self::commit()`] in order to instead, persist the changes.
    ///
    /// Explicit calls to [`.roll_back()`][`Self::roll_back()`] remain possible.
    pub
    struct Transaction<'r> {
        raw_txn: SafeManuallyDrop<RawTransaction<'r>, Self>,
        _some_other_owned_resource: Weak<()>,
    }

    impl<'r> DropManually<RawTransaction<'r>> for Transaction<'r> {
        fn drop_manually(raw_txn: RawTransaction<'r>) {
            raw_txn.roll_back();
        }
    }

    impl<'r> Transaction<'r> {
        pub
        fn roll_back(self) {
            // already done in drop (of `self.raw_txn`).
        }

        /// The main point of this example: writing this kind of body is subtle and error-prone
        /// when using `ManuallyDrop`:
        ///
        ///  1. In order to obtain the necessary owned `RawTransaction` access in drop, such a field
        ///     would have needed to be wrapped in a `ManuallyDrop`, alongside writing a `Drop` impl
        ///     which would be doing:
        ///     `let raw_txn = unsafe { ManuallyDrop::take(&mut self.raw_txn ) }`
        ///
        ///  2. Then, in this `fn commit` function, because now we have this extra, txn-rolling-back
        ///     logic in the `Drop` `impl`, we shall need to `ManuallyDrop::new()`-wrap *all of
        ///     `self` in order to ensure we do not roll_back when that function returns, and the
        ///     locals the function owned, such as `self`, get dropped.
        ///
        ///  3. But in doing so, now there is a risk of having disabled too much drop glue, such as
        ///     resource-legitimate drop glue (symbolized, in this code sketch, by the owned `Weak`
        ///     handle).
        ///
        ///  4. Hence why the `fn` body, here, would have had to do:
        ///
        ///     ```rust
        ///     fn commit(self) {
        ///         // Subtle! Destructure `self` into all of its owned fields, by bypassing/having
        ///         // defused the (preprended) `Drop` glue *only* (the rest of the drop glue, that
        ///         // of the fields, remains active by virtue of getting back said fields).
        ///         let (raw_txn, _some_other_owned_resource, _each, _other, _field) = unsafe {
        ///             // Granted, this whole block is yearning to be macro-ed.
        ///             // Or even better: "Rust, y u no offer `let Self { raw_txn, .. } = self;`??"
        ///
        ///             let this = ManuallyDrop::new(self);
        ///             let Self {
        ///                 ref raw_txn,
        ///                 ref _some_other_owned_resource,
        ///                 ref each,
        ///                 ref other,
        ///                 ref field
        ///             } = *this;
        ///             (
        ///                 <*const _>::read(raw_txn),
        ///                 <*const _>::read(_some_other_owned_resource),
        ///                 <*const _>::read(each),
        ///                 <*const _>::read(other),
        ///                 <*const _>::read(field),
        ///             )
        ///         };
        ///
        ///         // But remember, because of our inner `ManuallyDrop` around `raw_txn` itself
        ///         // (yes, there are *two* distinct `ManuallyDrop`s at play, here…) we need to
        ///         // unwrap this layer, now. Luckily, our so-obtained owned access makes it
        ///         // trivial:
        ///         let raw_txn: RawTransaction<'_> = ManuallyDrop::into_inner(raw_txn);
        ///
        ///         // At last:
        ///         raw_txn.commit(); // 😵‍💫
        ///     }
        ///     ```
        ///
        /// Pretty scary, huh?
        ///
        /// FWIW, the "awkwardness" of the pattern needed here cannot really be avoided (except
        /// maybe by, perhaps counter-intuitively, wrapping as many fields within the initial
        /// `ManuallyDrop`).
        ///
        ///   - That would have allowed rewriting the destructuring as follows:
        ///
        ///     ```rust
        ///     // 1. Defuse *all* the drop glue of `Self`, and thus, the prepended `Drop` logic.
        ///     let mut this = MD::new(self);
        ///     // 2. Take advantage of the other, inner, `MD`, to simply get owned access to the
        ///     //    inner `AllFields` by `MD::take()`ing it.
        ///     //    We get back *owned access* to the innermost fields; mainly, our `raw_txn`, but
        ///     //    also, implicitly, the drop glue of the remaining fields as well: we have
        ///     //    effectively re-infused that transitive drop glue, which means our prior
        ///     //    defusing has *only* applied to the preprended `Drop` logic.
        ///     let AllFields {
        ///         raw_txn,
        ///         // no need to worry about `_some_other_owned_resource`, it's implicitly dropped.
        ///         ..
        ///     } = unsafe {
        ///         MD::take(&mut this.all_fields)
        ///     };
        ///     ```
        ///
        /// But at least, when having to use `SafeManuallyDrop<>`, you shall not need to "come up"
        /// with the right usage of `ManuallyDrop` / not need to think of using it, and how.
        ///
        /// Instead, your end struct has no prepended `Drop` logic, only the special
        /// `SafeManuallyDrop` field does, so you can then trivially do exactly what we were trying
        /// to achieve here: call `SafeManuallyDrop::into_inner_defusing_impl_Drop()` so as to
        /// extract owned access to the fields whilst having defused this prepended `Drop` logic
        /// *only*. No *inherent*/deeper drop-glue casualties.
        pub
        fn commit(self) {
            // no need to worry about `_some_other_owned_resource`, it's naturally properly dropped.
            let raw_txn: RawTransaction<'_> = self.raw_txn.into_inner_defusing_impl_Drop();
            raw_txn.commit();
        }
    }

    impl<'r> Transaction<'r> {
        pub
        fn new(
            db_state: &'r mut Option<DbState>,
            ref_count: &Arc<()>,
        ) -> Self
        {
            Self {
                raw_txn: SafeManuallyDrop::new(RawTransaction::new(db_state)),
                _some_other_owned_resource: Arc::downgrade(ref_count),
            }
        }
    }
}

#[test]
fn test_txn_lib() {
    use ::std::{
        sync::Arc,
    };
    use self::{
        txn_lib::{DbState, Transaction},
    };

    let db_state = &mut None;
    let ref_count = &Arc::new(());

    *db_state = None;
    {
        assert_eq!(Arc::weak_count(ref_count), 0);
        let _txn = Transaction::new(db_state, ref_count);
        assert_eq!(Arc::weak_count(ref_count), 1);
    } // drop(_txn);
    assert_eq!(*db_state, Some(DbState::RolledBack));
    assert_eq!(Arc::weak_count(ref_count), 0);

    *db_state = None;
    {
        assert_eq!(Arc::weak_count(ref_count), 0);
        let txn = Transaction::new(db_state, ref_count);
        assert_eq!(Arc::weak_count(ref_count), 1);
        txn.commit();
    }
    assert_eq!(*db_state, Some(DbState::Committed));
    assert_eq!(Arc::weak_count(ref_count), 0);

    *db_state = None;
    {
        assert_eq!(Arc::weak_count(ref_count), 0);
        let txn = Transaction::new(db_state, ref_count);
        assert_eq!(Arc::weak_count(ref_count), 1);
        txn.roll_back();
    }
    assert_eq!(*db_state, Some(DbState::RolledBack));
    assert_eq!(Arc::weak_count(ref_count), 0);

    *db_state = None;
    {
        assert_eq!(Arc::weak_count(ref_count), 0);
        let txn = Transaction::new(db_state, ref_count);
        assert_eq!(Arc::weak_count(ref_count), 1);
        ::core::mem::forget(txn);
    }
    assert_eq!(*db_state, None);
    assert_eq!(Arc::weak_count(ref_count), 1); // Leak!

    // for the sake of test hygiene, let's manually undo the leak.
    unsafe { <*const _>::read(&Arc::downgrade(ref_count)); }
    assert_eq!(Arc::weak_count(ref_count), 0); // 🧙
}
