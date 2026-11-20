; expect: sat
(declare-const x Real)
(declare-const y Real)
(declare-const z Real)
(assert (< x y))
(assert (< y z))
(assert (< z (+ x 10)))
(check-sat)
