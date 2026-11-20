; expect: unsat
(declare-const x Real)
(declare-const y Real)
(assert (=> (> x 0) (> y 0)))
(assert (= x 1))
(assert (<= y 0))
(check-sat)
