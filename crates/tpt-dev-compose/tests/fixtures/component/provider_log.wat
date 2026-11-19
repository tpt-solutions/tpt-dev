(component
  (type $s (func (param "a" s32) (result s32)))
  (type $n u32)
  (core module $m
    (func (export "f") (param i32) (result i32)
      local.get 0))
  (core instance $i (instantiate $m))
  (alias core export $i "f" (core func $f))
  (func $lifted (type $s) (canon lift (core func $f)))
  (export "host-log" (func $lifted) (func (type $s)))
  (export "limit" (type $n)))
