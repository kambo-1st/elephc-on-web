<?php
function triple(int $value): int {
    return $value * 3;
}

function add_with_default(int $value, int $extra = 5): int {
    return $value + $extra;
}

$target = "web";
$answer = 40 + 2;
$ratio = 3.5 + 0.5;

echo "Hello from elephc on " . $target . "!\n";
echo "The answer is " . $answer . ".\n";
echo "Ratio is " . $ratio . ".\n";
echo "Triple is " . triple(14) . ".\n";
echo "Default arg is " . add_with_default(7) . ".\n";
echo "Named arg is " . add_with_default(extra: 8, value: 4) . ".\n";
echo "Target name length is " . strlen($target) . ".\n";

$i = 0;
while ($i < 3) {
    echo "loop " . $i . "\n";
    $i = $i + 1;
}

for ($outer = 0; $outer < 2; $outer++) {
    for ($inner = 0; $inner < 2; $inner++) {
        if ($inner == 1) {
            continue 2;
        }
        echo "nested " . $outer . ":" . $inner . "\n";
    }
}

for ($j = 0; $j < 2; $j++) {
    if ($j == 1 && $answer > 0) {
        echo "continue\n";
        continue;
    }
    echo "for " . $j . "\n";
}

echo "ternary " . ($answer > 0 ? 1 : 0) . "\n";
echo "scalar ternary " . ($answer > 0 ? 1.5 : 2.5) . "," . ($answer ?: 7) . "\n";
echo "coalesce " . (null ?? 7) . "," . ($target ?? "fallback") . "\n";

switch ($answer) {
    case 41:
        echo "switch missed\n";
        break;
    case 42:
        echo "switch hit\n";
        break;
    default:
        echo "switch default\n";
}

echo "match " . match ($answer) {
    41 => 0,
    42 => 1,
    default => 2,
} . "\n";
echo "match scalar " . match ($answer) {
    42 => 1.5,
    default => 2.5,
} . "," . match ($answer) {
    42 => true,
    default => false,
} . "\n";

echo "numeric " . abs(-42) . "," . intdiv(7, 2) . "," . min(3, 7, 2) . "," . max(1.5, 2.5) . "\n";
echo "casts " . intval(3.9) . "," . floatval(3) . "," . boolval(42) . "\n";
echo "types " . (is_int($answer) ? 1 : 0) . "," . (is_string($target) ? 1 : 0) . "\n";
echo "gettype " . gettype($answer) . "," . gettype($ratio) . "," . gettype($target) . "\n";
echo "ord " . ord("A") . "," . ord($target) . "\n";
echo "case " . strtolower("WEB") . "," . strtoupper("web") . "," . lcfirst("Web") . "," . ucfirst("web") . "\n";
echo "string funcs " . strrev("abc") . "," . trim(" web ") . "," . ltrim(" web") . "," . rtrim("web ") . "\n";
echo "repeat/substr " . str_repeat("ab", 2) . "," . substr("abcdef", 1, 3) . "\n";
echo "more strings " . chr(65) . "," . bin2hex("Az") . "," . hex2bin("417a") . "," . ucwords("web target") . "\n";
echo "replace/pad " . str_replace("web", "wasm", "web target") . "," . str_pad("web", 6, ".") . "," . substr_replace("abcdef", "XY", 2, 3) . "\n";
echo "encoding " . urlencode("web target~") . "," . rawurlencode("web target~") . "," . base64_encode("web") . "\n";
echo "search " . (str_contains("web target", "tar") ? 1 : 0) . "," . strpos("web target", "tar") . "," . strcmp("abc", "abd") . "\n";
echo "ctype/math " . (ctype_alnum("Web123") ? 1 : 0) . "," . pow(2, 3) . "," . hypot(3, 4) . "," . round(3.6) . "\n";
echo "format " . number_format(1234.56, 2) . "," . sprintf("%s:%d", "web", 7) . "\n";
printf("printf %s %d\n", "web", 42);
echo "path " . basename("/tmp/web.php", ".php") . "," . dirname("/tmp/web.php") . "," . pathinfo("/tmp/web.php", PATHINFO_EXTENSION) . "\n";
echo "json " . json_encode("a/b") . "," . json_encode("a/b", JSON_UNESCAPED_SLASHES) . "," . (json_validate("{}") ? 1 : 0) . "\n";
echo "exists " . (function_exists("strlen") ? 1 : 0) . "," . (is_callable("strlen") ? 1 : 0) . "," . (is_iterable("web") ? 1 : 0) . "\n";
echo "empty " . (empty("0") ? 1 : 0) . "," . (empty($target) ? 1 : 0) . "\n";
echo "numeric? " . (is_numeric(42) ? 1 : 0) . "," . (is_numeric("12.5") ? 1 : 0) . "," . (is_numeric("web") ? 1 : 0) . "\n";
echo "float predicates " . (is_finite(3.5) ? 1 : 0) . "," . (is_nan(sqrt(-1)) ? 1 : 0) . "," . (is_infinite(3.5) ? 1 : 0) . "\n";
echo "fdiv " . fdiv(7, 2) . "," . (is_infinite(fdiv(1, 0)) ? 1 : 0) . "\n";
echo "bits " . (6 & 3) . "," . (4 | 1) . "," . (7 ^ 3) . "," . (2 << 3) . "," . (16 >> 2) . "\n";
$printed = print "printed\n";
echo "print returned " . $printed . "\n";
echo "cast syntax " . ((int) 3.9) . "," . ((float) 3) . "," . ((bool) 42) . "," . ((string) 7) . "\n";
echo "float math " . (7 / 2) . "," . floor(3.9) . "," . ceil(3.1) . "," . sqrt(16) . "," . (pi() > 3 ? 1 : 0) . "\n";
echo "string bool " . (boolval("0") ? 1 : 0) . "," . (boolval($target) ? 1 : 0) . "\n";
echo "string eq " . ($target === "web" ? 1 : 0) . "," . ($target !== "wasm" ? 1 : 0) . "\n";
$compound = 4;
$compound += 3;
$compound *= 2;
$compound &= 14;
echo "compound " . $compound . "\n";
