(component
  (@custom "note" "first")
  (import "host-log" (func (param "msg" string)))
  (component $inner
    (core module $m (func (export "f")))
  )
  (instance $child (instantiate $inner))
  (@custom "note" "second"))
