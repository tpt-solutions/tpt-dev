(module
  (import "cycle_a" "g" (func $g (result i32)))
  (func (export "f") (result i32)
    call $g))
