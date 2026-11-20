; expect: sat
(declare-const x Real)
(declare-const y Real)
(declare-const z Real)
(assert (= (+ x y z) 10))
(assert (>= x 0))
(assert (>= y 0))
(assert (>= z 0))
(check-sat)
