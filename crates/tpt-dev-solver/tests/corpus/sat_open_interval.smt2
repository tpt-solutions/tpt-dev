; expect: sat
(declare-const x Real)
(assert (> x 0))
(assert (< x 1))
(check-sat)
