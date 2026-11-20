; expect: unsat
(declare-const x Real)
(assert (> x x))
(check-sat)
