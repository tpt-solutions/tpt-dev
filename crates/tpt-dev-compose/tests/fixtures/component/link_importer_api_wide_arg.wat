;; Imports an instance "api" whose "f" takes a u64, not a u32. No provider of
;; "api" above fits this.
(component
  (import "api" (instance
    (type $ft (func (param "x" u64) (result u32)))
    (export "f" (func (type $ft))))))
