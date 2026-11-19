(component
  (core module $m
    (func (export "add") (param i32 i32) (result i32)
      local.get 0
      local.get 1
      i32.add))
  (core instance $i (instantiate $m))
  (alias core export $i "add" (core func $add))
  (type $t (func (param "a" s32) (param "b" s32) (result s32)))
  (func $lifted (type $t) (canon lift (core func $add)))
  (instance $inst (export "add" (func $lifted)))
  (export "math" (instance $inst)))
