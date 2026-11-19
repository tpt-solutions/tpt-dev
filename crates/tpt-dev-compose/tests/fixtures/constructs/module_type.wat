;; A component type that declares a core module type. The module type has a
;; function type, an import and an export that use it, and an outer alias of the
;; component's own core function type.
(component
  (core type $f (func (param i32) (result i32)))
  (type $c (component
    (core type $mt (module
      (type $t (func (param i32)))
      (import "env" "f" (func (type $t)))
      (export "g" (func (type $t)))
      (alias outer 2 0 (type))
      (export "h" (func (type 1)))
    ))
    (import "m" (core module (export "" (func))))
  ))
)
