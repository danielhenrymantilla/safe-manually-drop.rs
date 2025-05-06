use ::safe_manually_drop::prelude::*;

pub
struct Defer<F : FnOnce()>(
    SafeManuallyDrop<F, Self>,
);

impl<F : FnOnce()> Defer<F> {
    pub
    fn new(f: F) -> Self {
        Self(SafeManuallyDrop::new(f))
    }
}

#[cfg(any())]
impl<F : FnOnce()> OverrideDropGlue for SafeManuallyDrop<F, /* in */ Defer<F>> {
    fn overridden_drop_glue(f: F) {
        f();
    }
}

impl<F : FnOnce()> DropManually<F> for Defer<F> {
    fn drop_manually(f: F) {
        f();
    }
}

#[test]
fn check_drop_for_defer() {
    let counter = &::core::cell::Cell::new(0);
    let incr = || counter.set(counter.get() + 1);
    {
        assert_eq!(counter.get(), 0);
        let a = Defer(SafeManuallyDrop::new(incr));
        assert_eq!(counter.get(), 0);
        let b = Defer(SafeManuallyDrop::new(|| {
            incr();
            drop(a);
        }));
        assert_eq!(counter.get(), 0);
        let _b = b.0.into_inner_defusing_impl_Drop();
        assert_eq!(counter.get(), 0);
    }
    // `_b`, and thus its contained `a`, get dropped, thus `a`'s drop glue gets called.
    assert_eq!(counter.get(), 1);
}

pub
struct ScopeGuard<State, F : FnOnce(State)>(
    SafeManuallyDrop<ScopeGuardFields<State, F>, Self>,
);

pub
struct ScopeGuardFields<State, F : FnOnce(State)> {
    pub state: State,
    pub on_drop: F,
}

impl<State, F : FnOnce(State)> ScopeGuardFields<State, F> {
    pub
    fn arm(self) -> ScopeGuard<State, F> {
        ScopeGuard(SafeManuallyDrop::new(self))
    }
}

impl<State, F : FnOnce(State)>
    DropManually<ScopeGuardFields<State, F>>
for
    ScopeGuard<State, F>
{
    fn drop_manually(ScopeGuardFields { state, on_drop }: ScopeGuardFields<State, F>) {
        on_drop(state);
    }
}

impl<State, F : FnOnce(State)> ScopeGuard<State, F> {
    pub
    fn defuse(self) -> ScopeGuardFields<State, F> {
        self.0.into_inner_defusing_impl_Drop()
    }
}

impl<State, F : FnOnce(State)> ::core::ops::Deref for ScopeGuard<State, F> {
    type Target = State;

    fn deref(&self) -> &State {
        &self.0.state
    }
}

impl<State, F : FnOnce(State)> ::core::ops::DerefMut for ScopeGuard<State, F> {
    fn deref_mut(&mut self) -> &mut State {
        &mut self.0.state
    }
}

#[test]
fn check_drop_for_scopeguard() {
    let counter =  ::core::cell::Cell::new(0);
    let scope_guard = ScopeGuardFields {
        state: &counter,
        on_drop: |counter| {
            counter.set(counter.get() + 1);
        },
    }.arm();
    assert_eq!(scope_guard.get(), 0);
    assert_eq!(counter.get(), 0);
    drop(scope_guard);
    assert_eq!(counter.get(), 1);

    let counter =  ::core::cell::Cell::new(0);
    let scope_guard = ScopeGuardFields {
        state: &counter,
        on_drop: |counter| {
            counter.set(counter.get() + 1);
        },
    }.arm();
    assert_eq!(scope_guard.get(), 0);
    assert_eq!(counter.get(), 0);
    let ScopeGuardFields { state, on_drop: _dropper } = scope_guard.defuse();
    assert_eq!(state.get(), 0);
    assert_eq!(counter.get(), 0);
}
