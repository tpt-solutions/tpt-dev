(module
  (table (export "t") 2 4 funcref)
  (memory (export "m") 1)
  (global (export "g") i32 (i32.const 7))
  (global (export "gm") (mut i32) (i32.const 0))
  (func (export "f") (param i32) (result i32) (local.get 0))
)
