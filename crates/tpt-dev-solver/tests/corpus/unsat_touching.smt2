; expect: unsat
(declare-const x Real)
(assert (< x 1))
(assert (>= x 1))
(check-sat)
