;; Imports an instance "api" that must export "h", which no provider of "api"
;; above exports.
(component
  (import "api" (instance
    (type $ft (func (param "x" u32) (result u32)))
    (export "h" (func (type $ft))))))
