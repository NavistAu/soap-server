# Deferred Items - Phase 02 Full Spec Compliance

## Out-of-scope discoveries from plan 02-01

### Pre-existing failing dispatch.rs RPC tests (out of scope for 02-01)

Discovered during Task 2 when a clean recompile revealed 3 pre-existing failing tests
in the uncommitted dispatch.rs changes. These tests were already in the working tree
before plan 02-01 started, and they test RPC binding dispatch functionality not yet
implemented.

**Failing tests:**
- `dispatch::tests::build_dispatch_table_rpc_binding`
- `dispatch::tests::rpc_dispatch_by_wrapper_element`
- `dispatch::tests::build_dispatch_table_rpc_missing_namespace_falls_back_to_target_ns`

**Root cause:** `build_dispatch_table` does not yet implement RPC-style dispatch
(keying dispatch table by `(soap:body namespace, operation name)` instead of
`input element QName`). The tests were written ahead of the implementation.

**Also added as blocker fix:** `build_dispatch_table_for_service` function was missing
(referenced by `build_dispatch_table_for_service_isolates_operations` test). Added the
implementation in plan 02-01 as Rule 3 (blocking) deviation - this test now passes.

**Resolution needed:** A future plan must implement RPC binding dispatch in
`build_dispatch_table` (or a new `rpc_dispatch_key` path) so these 3 tests pass.
