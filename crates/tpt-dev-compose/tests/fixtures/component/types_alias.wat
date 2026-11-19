;; Top-level aliases: an export of an instance import, and an outer alias from a
;; nested component type that refers back to a type of the enclosing component.
(component
  (type $f (func (param "a" u32) (result u32)))
  (import "api" (instance $api
    (export "f" (func (type $f)))))
  (alias export $api "f" (func $af))
  (core module $m
    (func (export "k") (param i32) (result i32) local.get 0))
  (core instance $mi (instantiate $m))
  (alias core export $mi "k" (core func $k))
  (type $c (component
    (alias outer 1 $f (type $g))
    (import "h" (func (type $g)))))
  (import "k2" (type $imported (eq $f)))
  (export "again" (func $af)))
