# Grammar catalog

WASM modules and highlight queries come from `tree-sitter-wasm@2.0.1` via jsDelivr. `catalog.json` pins the size and SHA-256 of each file from the npm tarball. Downloads begin only after the file view's Install action. Verified bundles are saved under `$XDG_DATA_HOME/cydonia/grammars/2.0.1` (default `~/.local/share/cydonia/grammars/2.0.1`).

To update the catalog, verify the npm tarball against its published integrity, regenerate asset sizes/checksums, and run:

```sh
CYDONIA_GRAMMAR_PACKAGE=/path/to/package/out cargo nextest run -p cydonia --lib -E 'test(all_catalog_grammars)' --run-ignored all
```

Haskell is omitted because this package's highlight query does not compile against its grammar. TypeScript/TSX, C++, and Vue include their base-language queries. Injection queries and locals are not installed.

On macOS, Wasmtime 48 uses `mprotect` rather than `MAP_JIT`; hardened bundles need `allow-unsigned-executable-memory` from `bundle/cydonia.entitlements`.
