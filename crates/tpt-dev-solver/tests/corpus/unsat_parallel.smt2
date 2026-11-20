; expect: unsat
(declare-const x Real)
(declare-const y Real)
(assert (= (+ x y) 3))
(assert (= (+ x y) 4))
(check-sat)
