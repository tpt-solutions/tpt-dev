; expect: sat
(declare-const x Real)
(assert (or (> x 5) (< x (- 5))))
(check-sat)
