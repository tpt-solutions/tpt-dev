; expect: unsat
(declare-const x Real)
(declare-const y Real)
(assert (= (+ (* 3 x) (* 2 y)) 6))
(assert (= x 0))
(assert (= y 4))
(check-sat)
