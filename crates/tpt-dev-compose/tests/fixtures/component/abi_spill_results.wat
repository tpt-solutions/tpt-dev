;; A result of two flat u32 values: over MAX_FLAT_RESULTS (1), so a lowered
;; function takes a return pointer, and a lifted one returns a pointer.
(component
  (type $t (func (param "a" u32) (result (tuple u32 u32))))
  (import "f" (func $f (type $t)))
  (core module $mem
    (memory (export "mem") 1)
    (func (export "g") (param i32) (result i32)
      i32.const 0))
  (core instance $mi (instantiate $mem))
  (alias core export $mi "mem" (core memory $m))
  (alias core export $mi "g" (core func $g))
  (core func $lowered (canon lower (func $f) (memory $m)))
  (func $lifted (type $t) (canon lift (core func $g) (memory $m)))
  (export "g" (func $lifted))
  (core module $user
    (import "h" "f" (func (param i32 i32))))
  (core instance (instantiate $user
    (with "h" (instance (export "f" (func $lowered)))))))
