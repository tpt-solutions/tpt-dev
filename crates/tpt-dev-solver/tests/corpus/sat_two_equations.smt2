; expect: sat
(declare-const x Real)
(declare-const y Real)
(assert (= (+ (* 2 x) (* 3 y)) 7))
(assert (= (- x y) 0))
(check-sat)
