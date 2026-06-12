# Found Zend PHP Disparities

Confirmed against native elephc on Linux x86_64 and the local Zend PHP CLI.

## `log2()` Exists In elephc, Not In Zend PHP

Status: confirmed disparity.

Zend PHP:

```bash
php -r 'var_dump(function_exists("log2"));'
# bool(false)
```

Native elephc:

```php
<?php echo log2(8.0) . "\n";
```

Observed elephc output:

```text
3
```

Source pointers:

- `src/types/checker/builtins/catalog.rs` lists `log2`.
- `src/types/checker/builtins/numeric.rs` type-checks `log2`.
- `src/codegen/builtins/math/log2.rs` emits native `log2`.
- `docs/php/math.md` currently documents `log2()`.

Compatibility note: this should either be treated as an elephc extension and documented outside PHP syntax, or removed/renamed from the PHP-compatible builtin surface.

## `sprintf("%i", ...)` Works In elephc, Fails In Zend PHP

Status: confirmed disparity.

Zend PHP:

```bash
php -r 'echo sprintf("%i", 42), "\n";'
# Fatal error: Uncaught ValueError: Unknown format specifier "i"
```

Native elephc:

```php
<?php echo sprintf("%i", 42) . "\n";
```

Observed elephc output:

```text
42
```

Likely cause: native elephc's `sprintf` runtime routes integer-like format specifiers through C `snprintf`, where `%i` is accepted, while Zend PHP's formatter rejects `%i`.

Source pointers:

- `src/codegen/runtime/strings/sprintf.rs` scans format specifiers and falls through unknown non-float/non-string specifiers to integer formatting.
- `src/codegen/builtins/strings/sprintf.rs` lowers `sprintf()` to `__rt_sprintf`.

Compatibility note: native `sprintf` should reject `%i` with a PHP-compatible error instead of forwarding it to `snprintf`.

## `trim()` Character Masks Do Not Expand `..` Ranges In elephc

Status: confirmed disparity.

Zend PHP:

```bash
php -r 'echo trim("abcwebxyz", "a..z"), "\n";'
# prints an empty line
```

Native elephc:

```php
<?php echo trim("abcwebxyz", "a..z") . "\n";
```

Observed elephc output:

```text
bcwebxy
```

Likely cause: native trim-mask helpers treat the mask as a set of literal bytes, so `a..z` matches only `a`, `.`, and `z`. Zend PHP treats valid incrementing `x..y` sequences as character ranges in trim masks.

Source pointers:

- `src/codegen/runtime/strings/trim_mask.rs`
- `src/codegen/runtime/strings/ltrim_mask.rs`
- `src/codegen/runtime/strings/rtrim_mask.rs`

Compatibility note: wasm currently follows native elephc's byte-mask behavior. Full PHP compatibility should expand valid trim-mask ranges and handle invalid ranges with PHP-compatible warnings/errors.

## Nested Associative Array Mutation With String Keys Is Rejected By elephc

Status: confirmed disparity.

Zend PHP:

```bash
php -r '$outer = ["user" => []]; $outer["user"]["settings"]["items"] = [3, 4, 5]; $outer["user"]["settings"]["items"][1] = 9; echo $outer["user"]["settings"]["items"][0], ":", $outer["user"]["settings"]["items"][1], "\n";'
# 3:9
```

Native elephc:

```php
<?php
function section(string $value): string { return $value; }
$outer = ["user" => []];
$outer["user"][section("settings")]["items"] = [3, 4, 5];
$outer["user"][section("settings")]["items"][1] = 9;
echo $outer["user"][section("settings")]["items"][0]; echo ":";
echo $outer["user"][section("settings")]["items"][1]; echo "\n";
```

Observed elephc output:

```text
error[4:15]: Array index must be integer
error[5:15]: Array index must be integer
error[6:20]: Array index must be integer
error[7:20]: Array index must be integer
```

Likely cause: the shared frontend/type checker still rejects these nested associative array accesses as non-integer array indexes before native or wasm codegen can lower them. Native elephc does accept deeper integer-index nested mutation, so the gap is specifically the PHP associative string-key path.

Source pointers:

- `src/types/` array-access validation emits `Array index must be integer` for this shape.
- `src/parser/ast/expr.rs` represents both indexed and associative lookups as `ExprKind::ArrayAccess`.
- wasm-only compatibility work under `src/codegen/wasm/` must not claim full native-elephc compatibility for this shape until the shared frontend/type metadata accepts it.

Compatibility note: this is a native elephc versus Zend PHP disparity, not just a wasm backend gap. Full PHP compatibility needs the shared checker/type metadata to allow string-key nested associative access and mutation where PHP permits it, while still rejecting unsupported cases with `CompileError` instead of silently miscompiling.

## `final` Properties Work In elephc, Fail In Zend PHP 8.3

Status: confirmed disparity.

Zend PHP 8.3.6:

```bash
php -r 'final class Box { final public $value = 1; } echo (new Box())->value, "\n";'
# Fatal error: Cannot use the final modifier on a property
```

Native elephc:

```bash
cargo test test_final_property_reads_normally_without_override -- --nocapture
# test codegen::oop::modifiers_and_properties::test_final_property_reads_normally_without_override ... ok
```

Source pointers:

- `tests/codegen/oop/modifiers_and_properties.rs` covers native final-property reads.
- `src/parser/ast/oop.rs` stores `ClassProperty::is_final`.
- `src/types/checker/schema/classes/mod.rs` and related class-schema checks track final-property metadata.

Compatibility note: PHP 8.3 rejects `final` properties, so PHP-oracle wasm tests must not use them as PHP-compatible fixtures. If elephc keeps this behavior, document it as an extension or gate it by PHP-version compatibility.

## Typed Local Declarations Work In elephc, Fail In Zend PHP

Status: confirmed disparity.

Zend PHP:

```bash
php /tmp/typed.php
# PHP Parse error: syntax error, unexpected variable "$name"
```

Native elephc:

```php
<?php
string $name = "web";
bool $ok = strlen($name) === 3;
float $score = 1.25;
echo $name . ":" . ($ok ? 1 : 0) . ":" . $score . "\n";
```

Observed elephc output:

```text
web:1:1.25
```

Source pointers:

- `src/parser/stmt/assign/locals.rs` parses typed local declarations into `StmtKind::TypedAssign`.
- `src/types/checker/stmt_check/assignments.rs` type-checks typed local assignments.
- `src/codegen/wasm/stmt.rs` lowers `TypedAssign` alongside ordinary assignment.

Compatibility note: typed local declarations are not Zend PHP syntax, so PHP-oracle wasm tests must not use them. Treat this as an elephc extension unless the syntax is removed or gated.
