; expect: sat
(declare-const x Real)
(declare-const y Real)
(assert (= (+ x y) 3))
(assert (= (- x y) 1))
(check-sat)
