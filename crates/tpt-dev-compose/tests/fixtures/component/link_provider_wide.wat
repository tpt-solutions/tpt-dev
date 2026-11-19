;; Exports an instance "api" with two functions, "f" (u32 -> u32) and "g". A
;; provider may export more than an importer asks for.
(component
  (type $api_ty (instance
    (type $ft (func (param "x" u32) (result u32)))
    (export "f" (func (type $ft)))
    (export "g" (func (type $ft)))))
  (core module $m
    (func (export "g") (param i32) (result i32) local.get 0))
  (core instance $mi (instantiate $m))
  (alias core export $mi "g" (core func $g))
  (type $ft (func (param "x" u32) (result u32)))
  (func $lifted (type $ft) (canon lift (core func $g)))
  (instance $inst (export "f" (func $lifted)) (export "g" (func $lifted)))
  (export "api" (instance $inst) (instance (type $api_ty))))
