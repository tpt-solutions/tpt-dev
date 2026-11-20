; expect: unsat
(declare-const x Real)
(assert (= (* 2 x) 3))
(assert (= x 1))
(check-sat)
