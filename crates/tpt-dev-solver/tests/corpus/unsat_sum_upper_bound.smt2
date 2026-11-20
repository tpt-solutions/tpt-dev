; expect: unsat
(declare-const x Real)
(declare-const y Real)
(assert (> (+ x y) 6))
(assert (< x 2))
(assert (< y 3))
(check-sat)
