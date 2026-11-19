(component
  (type $s (func (param "a" s32) (result s32)))
  (import "host-log" (func (type $s)))
  (import "limit" (type (eq $s))))
