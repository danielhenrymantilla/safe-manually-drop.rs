# The following snippets fail to compile

### Missing `Self` type param on `SafeManuallyDrop<FieldTy>`.

```rust ,compile_fail
use ::safe_manually_drop::prelude::*;

struct Defer<F>(
    SafeManuallyDrop<F>,
);
```

### Missing `impl<…> DropManually<FieldTy> for StructName<…> {`

```rust ,compile_fail
use ::safe_manually_drop::prelude::*;

struct Defer<F>(
    SafeManuallyDrop<F, Self>,
);
```

<!-- Templated by `cargo-generate` using https://github.com/danielhenrymantilla/proc-macro-template -->
