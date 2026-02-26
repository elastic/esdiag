## Context

Currently, the `esdiag` application processes massive Elastic Stack diagnostic files (frequently 1GB+) by streaming the JSON objects. Even though the streaming implementation avoids loading the entire array at once, individual heavy objects (like a single node's statistics in Elasticsearch, or complex `http` and `discovery` objects) are fully parsed into a DOM-like generic tree (`serde_json::Value`).

Because `serde_json::Value` allocates a new `BTreeMap` node for every nested object, and a new `String` for every key and value, a 10MB node object causes hundreds of thousands of tiny heap allocations. This fragments memory (resolved by swapping to `mimalloc`) and burns massive CPU cycles building and walking these intermediate trees, only for them to be immediately serialized back out to Elasticsearch verbatim.

## Goals / Non-Goals

**Goals:**
- Eliminate the CPU overhead of parsing flexible JSON objects into DOM trees (`Value`).
- Reduce the amount of memory allocated per streaming job by storing unparsed bytes.
- Switch all passive `Value` pass-through fields in our diagnostic structs to use `Box<serde_json::value::RawValue>`.

**Non-Goals:**
- Changing the streaming infrastructure (e.g., modifying `IndicesStatsVisitor` or `Receiver` logic).
- Refactoring `json_patch` mutation logic to support `RawValue`. (Any active mutations will either remain as explicit structs, or we will model the struct fully if a legacy `Value` was being patched).

## Decisions

1. **Enable the `raw_value` feature in `serde_json`:**
   By modifying `Cargo.toml` to include `features = ["raw_value"]`, Serde enables the opaque `RawValue` type.

2. **Replace `Option<serde_json::Value>` with `Option<Box<serde_json::value::RawValue>>`:**
   We will globally replace `serde_json::Value` with `Box<RawValue>`. `RawValue` validates that the bytes are valid JSON, but stops parsing there. It holds the string representation. When `serde_json` serializes the struct to export it, it writes those exact bytes directly to the output stream.

3. **Verify `json_patch` mutations:**
   We must audit the codebase (using `rg "patch" src/`) to ensure no logic currently performs a `.pointer_mut()` or `.apply()` on a field that we are migrating away from `Value`.

## Risks / Trade-offs

- [Risk] Memory lifetimes with `RawValue`. Because `RawValue` uses `Box`, it allocates one continuous block of heap memory per object. This is a massive improvement over `BTreeMap` fragmentation, but it still copies the uncompressed JSON bytes into RAM. → The true zero-copy `&'a RawValue` is impossible here since we are un-gzipping a stream, but the single allocation of a `Box` is exactly what we want for speed and fragmentation.
- [Risk] Broken tests or serializers. Some structs might implement custom `Serialize` or `Deserialize` traits that assume the field is a `Value`. → We will rely heavily on `cargo check` and `cargo test` to catch any type mismatches where the code expects a DOM structure.