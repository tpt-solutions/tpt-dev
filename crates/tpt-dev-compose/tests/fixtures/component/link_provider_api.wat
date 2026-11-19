;; Exports an instance "api" with one function "f" (u32 -> u32), declared with
;; an explicit instance type so the export carries its type.
(component
  (type $api_ty (instance
    (type $ft (func (param "x" u32) (result u32)))
    (export "f" (func (type $ft)))))
  (core module $m
    (func (export "g") (param i32) (result i32) local.get 0))
  (core instance $mi (instantiate $m))
  (alias core export $mi "g" (core func $g))
  (type $ft (func (param "x" u32) (result u32)))
  (func $lifted (type $ft) (canon lift (core func $g)))
  (instance $inst (export "f" (func $lifted)))
  (export "api" (instance $inst) (instance (type $api_ty))))
