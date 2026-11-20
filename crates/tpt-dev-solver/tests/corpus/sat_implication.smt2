; expect: sat
(declare-const x Real)
(declare-const y Real)
(assert (=> (> x 0) (> y 0)))
(assert (= x 1))
(check-sat)
