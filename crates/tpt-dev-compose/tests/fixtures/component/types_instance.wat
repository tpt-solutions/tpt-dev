;; An instance type declared inline, with a record, a function that uses it,
;; a nested component type, and the import of an instance of that type.
(component
  (type $iface (instance
    (type $rec (record (field "a" u8) (field "b" u32)))
    (export "rec" (type (eq $rec)))
    (type $g (func (param "x" u32) (result u32)))
    (export "f" (func (type $g)))
    (type $c (component
      (import "x" (func (type $g)))
      (export "y" (func (type $g)))))
    (export "c" (component (type $c)))))
  (import "api" (instance $api (type $iface)))
  (type $after (func (param "p" u32)))
  (export "api-again" (instance $api)))
