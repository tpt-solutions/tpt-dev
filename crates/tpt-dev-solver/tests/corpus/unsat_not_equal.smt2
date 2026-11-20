; expect: unsat
(declare-const x Real)
(declare-const y Real)
(assert (not (= x y)))
(assert (= x y))
(check-sat)
