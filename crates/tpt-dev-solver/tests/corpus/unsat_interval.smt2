; expect: unsat
(declare-const x Real)
(assert (> x 1))
(assert (< x 0))
(check-sat)
