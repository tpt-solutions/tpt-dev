(module
  (import "cycle_b" "f" (func $f (result i32)))
  (func (export "g") (result i32)
    call $f))
