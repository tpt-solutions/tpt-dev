(module
  (import "p" "t" (table 1 4 funcref))
  (import "p" "m" (memory 1))
  (import "p" "g" (global i32))
  (import "p" "gm" (global (mut i32)))
)
