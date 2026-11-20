; expect: sat
(declare-const x Real)
(declare-const y Real)
(assert (= (+ (* 3 x) (* 2 y)) 6))
(assert (= x 0))
(check-sat)
