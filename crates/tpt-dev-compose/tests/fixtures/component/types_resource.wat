;; Resource types: a fresh resource with a representation and a destructor, an
;; import of a sub-resource, and an imported instance that exports one.
(component
  (core module $m
    (func (export "dtor") (param i32)))
  (core instance $mi (instantiate $m))
  (alias core export $mi "dtor" (core func $dtor))
  (type $r (resource (rep i32) (dtor (func $dtor))))
  (import "sub" (type $s (sub resource)))
  (import "api" (instance $api
    (export "r" (type (sub resource)))))
  (export "r" (type $r)))
