---
title: "WASM Runtime Architecture Plan"
description: "Implementation plan for completing wasm32-web PHP string and array runtime support."
sidebar:
  order: 14
---

The `wasm32-web` backend must converge on a WASM-only runtime layer that mirrors
the native backend's approach: codegen should emit calls into `__rt_*` helpers
instead of duplicating PHP storage semantics inline. Native codegen remains
untouched.

## Goals

- Represent PHP values through a stable boxed value-cell ABI.
- Represent strings and arrays as heap objects with headers, refcounts, and COW.
- Keep direct WAT generation simple: codegen selects helper calls; helpers own
  layout, aliasing, and mutation details.
- Preserve conservative behavior while migrating: unsupported dynamic forms keep
  failing with `CompileError` until they have PHP-oracle coverage.

## Target Runtime Shapes

## Native Alignment Notes

The native runtime already uses a uniform heap object convention that WASM
should mirror behind `__rt_*` helpers:

- 16-byte header before each user payload
- payload size at header offset 0
- refcount at header offset 4
- heap kind word at header offset 8
- heap kinds: `1` string, `2` indexed array, `3` associative/hash array, `4`
  object, `5` boxed mixed

WASM codegen should keep passing payload-compatible pointers through existing
locals while the runtime helpers gradually introduce the native-compatible
header, kind, refcount, COW, and release behavior. That lets helper internals
change without broad codegen churn.

### Mixed value cell

The value cell is the universal carrier for dynamic PHP values:

- tag: `int`, `bool`, `null`, `string`, `array`, later `object`, `float`, and
  `callable`
- payload: scalar bits or a pointer to a heap object
- ownership: payload pointers must be retained/released through runtime helpers

Every array element, mixed local, callable argument pack, and dynamic return
should eventually use this shape.

### Heap string object

Strings need a heap header instead of raw pointer/length pairs:

- refcount
- length
- capacity
- flags for persistent/static data
- byte payload

String offset mutation must call an ensure-unique helper before writing. Static
data strings can start as persistent objects or be copied into heap objects when
mutation is possible.

### Heap array object

Arrays need one hash-table runtime shape for indexed, associative, mixed, and
nested arrays:

- refcount
- count
- capacity
- next auto integer key
- ordered entries
- key metadata for integer or string keys
- value cells

Indexed arrays are then a fast case of the same runtime object instead of a
separate codegen layout.

### Callable object

Callbacks need a runtime callable descriptor:

- user function symbol or builtin id
- closure capture pointer
- required/variadic metadata
- invocation helper for direct calls and array builtins

Array callback builtins should call this descriptor through one helper path.

## Migration Phases

1. Centralize allocation and address math.
   Add helpers such as `__rt_alloc_bytes`, value-cell accessors, assoc-entry
   accessors, and string/array header accessors. Route existing codegen through
   those helpers without changing behavior.

2. Introduce heap object headers.
   Add string and array allocation helpers that reserve header space and return
   payload-compatible pointers while existing codegen still works.

3. Add retain/release and COW helpers.
   Implement refcount increments on copies and ensure-unique helpers before
   mutation. Convert current eager deep-copy paths to helper calls one feature
   at a time.

4. Collapse array layouts.
   Move compact indexed, value-cell indexed, and assoc-entry arrays behind one
   runtime array object. Codegen should stop branching on layout except for
   compile-time optimization opportunities.

5. Generalize dynamic values.
   Use value cells for mixed locals, dynamic array reads, array returns, `mixed`
   builtin results, nullable storage, and broader scalar coercions.

6. Implement callable runtime support.
   Add callable descriptors, user-function and builtin invocation helpers, then
   enable `array_map`, `array_filter`, `array_reduce`, `array_walk`, and callback
   sort variants with PHP-oracle tests.

7. Expand nested/object integration.
   Reuse value cells and refcounting for object properties, nested arrays,
   JSON decode/encode, and PHP object support.

## Slice Rules

- Each slice must be WASM-only unless a concrete shared-frontend blocker exists.
- Each supported behavior needs a PHP-oracle e2e test.
- Each runtime helper needs a WAT-shape test when practical.
- Unsupported forms must keep returning `CompileError`.
- Before committing a slice, run:

```bash
cargo test --test wasm_tests -- --nocapture --test-threads=1 && cargo check && git diff --check
```
