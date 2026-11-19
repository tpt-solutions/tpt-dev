;; Outer aliases in component types: one scope out (the enclosing component) and
;; two scopes out (the outermost component), from a component type inside a nested
;; component.
(component
  (type $l (list u8))
  (component
    (alias outer 1 $l (type $m))
    (type $c (component
      (alias outer 2 $l (type))
      (alias outer 1 $m (type))
      (type (option 0))
    ))
    (type (option $m))
  )
)
