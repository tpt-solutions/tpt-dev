;; Value kinds the parser decodes: error-context, stream, future, map, a fixed-length
;; list, and an async function type.
(component
  (type $s (stream u8))
  (type $f (future string))
  (type $m (map string u32))
  (type $fl (list u8 4))
  (type $e error-context)
  (type $af (func async (param "x" u8) (result u8)))
  (type $sf (func (param "a" $fl) (param "b" $e) (result $m)))
)
