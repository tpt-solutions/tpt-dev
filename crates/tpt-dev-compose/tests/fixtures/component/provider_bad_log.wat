(component
  (type $s (func (param "a" s64) (result s64)))
  (core module $m
    (func (export "f") (param i64) (result i64)
      local.get 0))
  (core instance $i (instantiate $m))
  (alias core export $i "f" (core func $f))
  (func $lifted (type $s) (canon lift (core func $f)))
  (export "host-log" (func $lifted) (func (type $s))))
