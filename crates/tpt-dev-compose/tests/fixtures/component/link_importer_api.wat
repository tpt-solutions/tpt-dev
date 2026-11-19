;; Imports an instance "api" that must export "f" as u32 -> u32. A provider with
;; the same function type fits.
(component
  (import "api" (instance
    (type $ft (func (param "x" u32) (result u32)))
    (export "f" (func (type $ft))))))
