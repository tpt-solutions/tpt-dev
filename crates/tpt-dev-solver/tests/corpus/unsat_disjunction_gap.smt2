; expect: unsat
(declare-const x Real)
(assert (or (> x 5) (< x (- 5))))
(assert (>= x (- 1)))
(assert (<= x 1))
(check-sat)
