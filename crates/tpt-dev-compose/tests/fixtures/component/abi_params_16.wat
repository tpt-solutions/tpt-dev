;; 16 flat u32 parameters: exactly MAX_FLAT_PARAMS, so they stay in registers.
(component
  (type $t (func
    (param "p0" u32) (param "p1" u32) (param "p2" u32) (param "p3" u32)
    (param "p4" u32) (param "p5" u32) (param "p6" u32) (param "p7" u32)
    (param "p8" u32) (param "p9" u32) (param "p10" u32) (param "p11" u32)
    (param "p12" u32) (param "p13" u32) (param "p14" u32) (param "p15" u32)))
  (import "f" (func $f (type $t)))
  (core func $lowered (canon lower (func $f)))
  (core module $user
    (import "h" "f" (func (param i32 i32 i32 i32 i32 i32 i32 i32
                           i32 i32 i32 i32 i32 i32 i32 i32))))
  (core instance (instantiate $user
    (with "h" (instance (export "f" (func $lowered)))))))
