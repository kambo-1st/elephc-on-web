//! Purpose:
//! Lowers hash-oriented string builtins for wasm32-web expression codegen.
//! Owns md5(), sha1(), and hash() literal evaluation plus host-backed runtime hash emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Keeps supported literal digest algorithms and runtime host hash algorithm validation in one place.

use super::*;

pub(super) fn emit_hash_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if let Some(emitted) = emit_md5_sha1_dynamic_raw_value_to_stack(call, name, args, module)? {
        return Ok(emitted);
    }
    let Some((algorithm, data_arg, raw_output)) = hash_value_algorithm_and_data(call, name, args, module)? else {
        if name.eq_ignore_ascii_case("hash") {
            return emit_hash_dynamic_algorithm_value_to_stack(call, args, module);
        }
        return Ok(false);
    };
    if let Some(data) = static_or_tracked_string_value(data_arg, module) {
        if raw_output {
            let digest = eval_hash_digest_with_algorithm_code(call, algorithm, data.as_bytes())?;
            let (ptr, len) = module.intern_bytes(&digest);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
        } else {
            let value = eval_hash_with_algorithm_code(call, algorithm, data.as_bytes())?;
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
        }
        return Ok(true);
    }
    let Some(var) = string_arg_or_materialize(data_arg, "hash_value_arg", module)? else {
        return Ok(false);
    };
    if raw_output {
        emit_runtime_hash_raw_value_to_stack(&var, algorithm, module);
    } else {
        emit_runtime_hash_hex_value_to_stack(&var, algorithm, module);
    }
    Ok(true)
}

fn emit_md5_sha1_dynamic_raw_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<bool>, CompileError> {
    let algorithm = match name.to_ascii_lowercase().as_str() {
        "md5" => 1,
        "sha1" => 2,
        _ => return Ok(None),
    };
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects one or two arguments", name),
        ));
    }
    let Some(raw_arg) = args.get(1) else {
        return Ok(None);
    };
    if literal_bool_arg(raw_arg).is_ok() {
        return Ok(None);
    }
    let Some(var) = string_arg_or_materialize(&args[0], "hash_value_arg", module)? else {
        return Ok(Some(false));
    };
    let raw_output = runtime_bool_arg(raw_arg, module)?;
    emit_runtime_hash_value_to_stack(&var, algorithm, raw_output, module);
    Ok(Some(true))
}

fn emit_hash_dynamic_algorithm_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web hash() expects two or three arguments",
        ));
    }
    if let Some(algorithm) = static_or_tracked_string_value(&args[0], module) {
        let lower = algorithm.to_ascii_lowercase();
        if !host_hash_algorithm_supported(&lower) {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web hash() currently supports md2, md4, md5, sha1, sha224, sha256, sha384, sha512, sha512/224, sha512/256, sha3-224, sha3-256, sha3-384, sha3-512, ripemd128, ripemd160, ripemd256, ripemd320, adler32, crc32, crc32b, crc32c, fnv132, fnv1a32, fnv164, fnv1a64, joaat, murmur3a, murmur3c, murmur3f, xxh32, and xxh64",
            ));
        }
    }
    let Some(algorithm_var) = string_arg_or_materialize(&args[0], "hash_algorithm", module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web hash() dynamic algorithm must be a string value",
        ));
    };
    let Some(data_var) = string_arg_or_materialize(&args[1], "hash_value_arg", module)? else {
        return Ok(false);
    };
    let raw_output = args
        .get(2)
        .map(|arg| runtime_bool_arg(arg, module))
        .transpose()?
        .unwrap_or(RuntimeBoolArg::Static(false));
    emit_runtime_hash_name_value_to_stack(&algorithm_var, &data_var, raw_output, module);
    Ok(true)
}

fn hash_value_algorithm_and_data<'a>(
    call: &Expr,
    name: &str,
    args: &'a [Expr],
    module: &WasmModule,
) -> Result<Option<(i32, &'a Expr, bool)>, CompileError> {
    match name.to_ascii_lowercase().as_str() {
        "md5" | "sha1" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects one or two arguments", name),
                ));
            }
            let raw_output = args.get(1).map(literal_bool_arg).transpose()?.unwrap_or(false);
            let algorithm = if name.eq_ignore_ascii_case("md5") { 1 } else { 2 };
            Ok(Some((algorithm, &args[0], raw_output)))
        }
        "hash" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web hash() expects two or three arguments",
                ));
            }
            let raw_output = if let Some(raw_output) = args.get(2) {
                match literal_bool_arg(raw_output) {
                    Ok(raw_output) => raw_output,
                    Err(_) => return Ok(None),
                }
            } else {
                false
            };
            let Some(algorithm) = static_or_tracked_string_value(&args[0], module) else {
                return Ok(None);
            };
            let code = match algorithm.to_ascii_lowercase().as_str() {
                "md5" => 1,
                "sha1" => 2,
                "sha256" => 3,
                _ => return Ok(None),
            };
            Ok(Some((code, &args[1], raw_output)))
        }
        _ => Ok(None),
    }
}

fn host_hash_algorithm_supported(algorithm: &str) -> bool {
    matches!(
        algorithm,
        "md2"
            | "md4"
            | "md5"
            | "sha1"
            | "sha224"
            | "sha256"
            | "sha384"
            | "sha512"
            | "sha512/224"
            | "sha512/256"
            | "sha3-224"
            | "sha3-256"
            | "sha3-384"
            | "sha3-512"
            | "ripemd128"
            | "ripemd160"
            | "ripemd256"
            | "ripemd320"
            | "adler32"
            | "crc32b"
            | "crc32"
            | "crc32c"
            | "fnv132"
            | "fnv1a32"
            | "fnv164"
            | "fnv1a64"
            | "joaat"
            | "murmur3a"
            | "murmur3c"
            | "murmur3f"
            | "xxh32"
            | "xxh64"
    )
}

fn emit_runtime_hash_hex_value_to_stack(var: &str, algorithm: i32, module: &mut WasmModule) {
    let out_ptr = module.next_label("hash_out_ptr");
    let out_len = module.next_label("hash_out_len");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 64");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("i32.const {}", algorithm));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_hash_hex");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_runtime_hash_raw_value_to_stack(var: &str, algorithm: i32, module: &mut WasmModule) {
    let out_ptr = module.next_label("hash_raw_out_ptr");
    let out_len = module.next_label("hash_raw_out_len");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("i32.const {}", algorithm));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_hash_raw");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_runtime_hash_value_to_stack(
    var: &str,
    algorithm: i32,
    raw_output: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) {
    match raw_output {
        RuntimeBoolArg::Static(true) => emit_runtime_hash_raw_value_to_stack(var, algorithm, module),
        RuntimeBoolArg::Static(false) => emit_runtime_hash_hex_value_to_stack(var, algorithm, module),
        RuntimeBoolArg::Variable(local) => {
            let out_ptr = module.next_label("hash_dynamic_raw_out_ptr");
            let out_len = module.next_label("hash_dynamic_raw_out_len");
            module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(out_len.trim_start_matches('$').to_string());
            module.body().line("global.get $heap");
            module.body().line(&format!("local.set {}", out_ptr));
            module.body().line("global.get $heap");
            module.body().line("i32.const 64");
            module.body().line("i32.add");
            module.body().line("global.set $heap");
            module.body().line(&format!("local.get ${}", local));
            module.body().open("if (result i32)");
            emit_runtime_hash_args(var, algorithm, &out_ptr, module);
            module.body().line("call $host_hash_raw");
            module.body().line("else");
            emit_runtime_hash_args(var, algorithm, &out_ptr, module);
            module.body().line("call $host_hash_hex");
            module.body().close("end");
            module.body().line(&format!("local.set {}", out_len));
            module.body().line(&format!("local.get {}", out_ptr));
            module.body().line(&format!("local.get {}", out_len));
        }
    }
}

fn emit_runtime_hash_args(var: &str, algorithm: i32, out_ptr: &str, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", algorithm));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
}

fn emit_runtime_hash_name_value_to_stack(
    algorithm_var: &str,
    data_var: &str,
    raw_output: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("hash_name_out_ptr");
    let out_len = module.next_label("hash_name_out_len");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(match raw_output {
        RuntimeBoolArg::Static(true) => "i32.const 64",
        RuntimeBoolArg::Static(false) | RuntimeBoolArg::Variable(_) => "i32.const 128",
    });
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    match raw_output {
        RuntimeBoolArg::Static(true) => {
            emit_runtime_hash_name_args(algorithm_var, data_var, &out_ptr, module);
            module.body().line("call $host_hash_raw_name");
        }
        RuntimeBoolArg::Static(false) => {
            emit_runtime_hash_name_args(algorithm_var, data_var, &out_ptr, module);
            module.body().line("call $host_hash_hex_name");
        }
        RuntimeBoolArg::Variable(local) => {
            module.body().line(&format!("local.get ${}", local));
            module.body().open("if (result i32)");
            emit_runtime_hash_name_args(algorithm_var, data_var, &out_ptr, module);
            module.body().line("call $host_hash_raw_name");
            module.body().line("else");
            emit_runtime_hash_name_args(algorithm_var, data_var, &out_ptr, module);
            module.body().line("call $host_hash_hex_name");
            module.body().close("end");
        }
    }
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_runtime_hash_name_args(
    algorithm_var: &str,
    data_var: &str,
    out_ptr: &str,
    module: &mut WasmModule,
) {
    module
        .body()
        .line(&format!("local.get ${}_ptr", algorithm_var));
    module
        .body()
        .line(&format!("local.get ${}_len", algorithm_var));
    module.body().line(&format!("local.get ${}_ptr", data_var));
    module.body().line(&format!("local.get ${}_len", data_var));
    module.body().line(&format!("local.get {}", out_ptr));
}

pub(super) fn eval_literal_md5(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web md5() expects one or two literal arguments",
        ));
    }
    ensure_hash_hex_output(call, args.get(1), "md5")?;
    Ok(bin2hex(&md5_digest(literal_string_arg(&args[0])?.as_bytes())))
}

pub(super) fn eval_literal_sha1(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sha1() expects one or two literal arguments",
        ));
    }
    ensure_hash_hex_output(call, args.get(1), "sha1")?;
    Ok(bin2hex(&sha1_digest(literal_string_arg(&args[0])?.as_bytes())))
}

pub(super) fn eval_literal_hash(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    let [algo, data] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web hash() expects exactly two literal arguments",
        ));
    };
    let data = literal_string_arg(data)?.as_bytes();
    eval_hash_algorithm(algo, &literal_string_arg(algo)?.to_ascii_lowercase(), data)
}

fn eval_hash_with_algorithm_code(
    call: &Expr,
    algorithm: i32,
    data: &[u8],
) -> Result<String, CompileError> {
    Ok(bin2hex(&eval_hash_digest_with_algorithm_code(
        call, algorithm, data,
    )?))
}

fn eval_hash_digest_with_algorithm_code(
    call: &Expr,
    algorithm: i32,
    data: &[u8],
) -> Result<Vec<u8>, CompileError> {
    match algorithm {
        1 => Ok(md5_digest(data).to_vec()),
        2 => Ok(sha1_digest(data).to_vec()),
        3 => Ok(sha256_digest(data).to_vec()),
        _ => Err(CompileError::new(
            call.span,
            "wasm32-web hash() currently supports md5, sha1, and sha256",
        )),
    }
}

fn eval_hash_algorithm(
    algo: &Expr,
    algorithm: &str,
    data: &[u8],
) -> Result<String, CompileError> {
    match algorithm {
        "md5" => Ok(bin2hex(&md5_digest(data))),
        "sha1" => Ok(bin2hex(&sha1_digest(data))),
        "sha256" => Ok(bin2hex(&sha256_digest(data))),
        _ => Err(CompileError::new(
            algo.span,
            "wasm32-web hash() currently supports literal md5, sha1, and sha256 algorithms",
        )),
    }
}

fn ensure_hash_hex_output(
    call: &Expr,
    raw_output: Option<&Expr>,
    name: &str,
) -> Result<(), CompileError> {
    if raw_output.map(literal_bool_arg).transpose()?.unwrap_or(false) {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() raw binary output is not supported yet", name),
        ));
    }
    Ok(())
}

fn md5_digest(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14,
        20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4,
        11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut message = input.to_vec();
    let bit_len = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

    for chunk in message.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (index, word) in m.iter_mut().enumerate() {
            let start = index * 4;
            *word = u32::from_le_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | ((!b) & d), i)
            } else if i < 32 {
                ((d & b) | ((!d) & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * i) % 16)
            };
            let next = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(m[g])
                    .rotate_left(S[i]),
            );
            a = next;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let mut message = input.to_vec();
    let bit_len = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let mut h0 = 0x67452301u32;
    let mut h1 = 0xefcdab89u32;
    let mut h2 = 0x98badcfeu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xc3d2e1f0u32;

    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, word) in w.iter().enumerate() {
            let (f, k) = if i < 20 {
                (((b & c) | ((!b) & d)), 0x5a827999)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ed9eba1)
            } else if i < 60 {
                (((b & c) | (b & d) | (c & d)), 0x8f1bbcdc)
            } else {
                (b ^ c ^ d, 0xca62c1d6)
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn sha256_digest(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut message = input.to_vec();
    let bit_len = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (index, word) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}
